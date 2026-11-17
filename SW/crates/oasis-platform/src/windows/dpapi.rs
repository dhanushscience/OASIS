//! DPAPI-backed secrets, same files and entropy as the C# app so existing ones keep working.
//! Port of `Platform/CredentialStore.cs` (storage half) and `DpapiHmacSigner.GetOrCreateKey`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use oasis_dpapi::{protect, unprotect};

use crate::atomic_file;
use crate::integrity::KeyProvider;
use crate::ports::{PlatformError, PlatformResult, SecretStore};
use crate::secrets::*;

/// `DpapiHmacSigner.Entropy`. Must not change, or existing `profile_hmac.key` files stop opening.
const PROFILE_KEY_ENTROPY: &[u8] = b"ProjectOASIS.ProfileIntegrity.v1";

/// Secret name → (file in `secrets/`, DPAPI entropy, trim before storing). From `CredentialStore`.
const SECRETS: [(&str, &str, &str, bool); 10] = [
    (GEMINI_API_KEY, "gemini.key", "OasisShell.GeminiApiKey", true),
    (CURSOR_API_KEY, "cursor.key", "OasisShell.CursorApiKey", false),
    (GROQ_API_KEY, "groq.key", "OasisShell.GroqApiKey", false),
    (OUTLOOK_TOKEN, "outlook.token", "OasisShell.OutlookToken", false),
    (GOOGLE_TOKEN, "google.token", "OasisShell.GoogleToken", false),
    (GOOGLE_CLIENT_SECRET, "google.client_secret", "OasisShell.GoogleClientSecret", false),
    (MARKETPLACE_SESSION, "marketplace.session", "OasisShell.MarketplaceSession", false),
    (MARKETPLACE_ANON_KEY, "marketplace.anon", "OasisShell.MarketplaceAnonKey", false),
    (SPOTIFY_TOKEN, "spotify.token", "OasisShell.SpotifyToken", false),
    (BROWSER_BRIDGE_TOKEN, "bridge.token", "OasisShell.BrowserBridgeToken", false),
];

pub struct DpapiSecretStore {
    folder: PathBuf,
}

impl DpapiSecretStore {
    /// `folder` is `OasisPaths::secrets_folder()`.
    pub fn new(folder: PathBuf) -> DpapiSecretStore {
        DpapiSecretStore { folder }
    }

    fn entry(&self, key: &str) -> PlatformResult<(PathBuf, &'static str, bool)> {
        SECRETS
            .iter()
            .find(|(name, ..)| *name == key)
            .map(|(_, file, entropy, trim)| (self.folder.join(file), *entropy, *trim))
            .ok_or_else(|| PlatformError::Unsupported(format!("Unknown secret '{key}'.")))
    }
}

fn io_error(e: io::Error) -> PlatformError {
    PlatformError::Io(e.to_string())
}

impl SecretStore for DpapiSecretStore {
    /// `None` if never set. A file that cannot be decrypted (another Windows user, damaged)
    /// is an error the UI can show; C# returned null for it.
    fn get(&self, key: &str) -> PlatformResult<Option<String>> {
        let (path, entropy, _) = self.entry(key)?;
        let blob = match fs::read(&path) {
            Ok(blob) => blob,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(io_error(e)),
        };
        let plain = unprotect(&blob, entropy.as_bytes())
            .map_err(|e| PlatformError::Unavailable(format!("Saved {key} could not be read: {e}")))?;
        String::from_utf8(plain)
            .map(Some)
            .map_err(|_| PlatformError::Unavailable(format!("Saved {key} is not valid text.")))
    }

    /// A blank value clears the secret, as in C#.
    fn set(&self, key: &str, value: &str) -> PlatformResult<()> {
        let (path, entropy, trim) = self.entry(key)?;
        if value.trim().is_empty() {
            return remove(&path);
        }
        let text = if trim { value.trim() } else { value };
        let blob = protect(text.as_bytes(), entropy.as_bytes())
            .map_err(|e| PlatformError::Unavailable(e.to_string()))?;
        atomic_file::write_bytes(&path, &blob).map_err(io_error)
    }

    fn clear(&self, key: &str) -> PlatformResult<()> {
        remove(&self.entry(key)?.0)
    }
}

fn remove(path: &Path) -> PlatformResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error(e)),
    }
}

/// The profile-signing key: 32 random bytes in `secrets/profile_hmac.key`, DPAPI-protected.
pub struct DpapiKeyFile {
    path: PathBuf,
}

impl DpapiKeyFile {
    pub fn new(path: PathBuf) -> DpapiKeyFile {
        DpapiKeyFile { path }
    }
}

impl KeyProvider for DpapiKeyFile {
    fn key(&self) -> io::Result<Vec<u8>> {
        match fs::read(&self.path) {
            Ok(blob) => return unprotect(&blob, PROFILE_KEY_ENTROPY).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut key = vec![0u8; 32];
        getrandom::fill(&mut key).map_err(|e| io::Error::other(format!("No random source: {e}")))?;
        let blob = protect(&key, PROFILE_KEY_ENTROPY).map_err(io::Error::other)?;
        atomic_file::write_bytes(&self.path, &blob)?;
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrity::HmacSigner;
    use crate::profile_store::{LoadOutcome, ProfileStore, SettingsIntegrity};
    use oasis_settings::Settings;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-dpapi-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn secrets_round_trip_and_use_the_csharp_file_names() {
        let dir = temp_dir("secrets");
        let store = DpapiSecretStore::new(dir.clone());
        assert_eq!(store.get(GROQ_API_KEY).unwrap(), None);
        store.set(GROQ_API_KEY, "gsk_test").unwrap();
        assert!(dir.join("groq.key").exists());
        assert_eq!(store.get(GROQ_API_KEY).unwrap().as_deref(), Some("gsk_test"));
        store.clear(GROQ_API_KEY).unwrap();
        assert_eq!(store.get(GROQ_API_KEY).unwrap(), None);
        store.clear(GROQ_API_KEY).unwrap();
    }

    #[test]
    fn only_the_gemini_key_is_trimmed_and_a_blank_value_clears() {
        let store = DpapiSecretStore::new(temp_dir("trim"));
        store.set(GEMINI_API_KEY, "  key  ").unwrap();
        assert_eq!(store.get(GEMINI_API_KEY).unwrap().as_deref(), Some("key"));
        store.set(CURSOR_API_KEY, "  key  ").unwrap();
        assert_eq!(store.get(CURSOR_API_KEY).unwrap().as_deref(), Some("  key  "));
        store.set(GEMINI_API_KEY, "   ").unwrap();
        assert_eq!(store.get(GEMINI_API_KEY).unwrap(), None);
    }

    #[test]
    fn each_secret_has_its_own_entropy() {
        let dir = temp_dir("entropy");
        let store = DpapiSecretStore::new(dir.clone());
        store.set(GROQ_API_KEY, "v").unwrap();
        fs::copy(dir.join("groq.key"), dir.join("cursor.key")).unwrap();
        assert!(matches!(store.get(CURSOR_API_KEY), Err(PlatformError::Unavailable(_))));
    }

    #[test]
    fn unknown_names_are_refused() {
        let store = DpapiSecretStore::new(temp_dir("unknown"));
        assert!(matches!(store.get("nope"), Err(PlatformError::Unsupported(_))));
    }

    #[test]
    fn bridge_token_survives_through_the_store() {
        let store = DpapiSecretStore::new(temp_dir("bridge"));
        let token = get_or_create_bridge_token(&store).unwrap();
        assert_eq!(get_or_create_bridge_token(&store).unwrap(), token);
    }

    #[test]
    fn key_is_created_once_and_reused() {
        let dir = temp_dir("key");
        let keys = DpapiKeyFile::new(dir.join("profile_hmac.key"));
        let first = keys.key().unwrap();
        assert_eq!(first.len(), 32);
        assert_eq!(keys.key().unwrap(), first);
        assert_ne!(fs::read(dir.join("profile_hmac.key")).unwrap(), first);
    }

    #[test]
    fn profile_store_signs_verifies_and_catches_a_hand_edit_with_dpapi() {
        let dir = temp_dir("store");
        let signer = HmacSigner::new(dir.join("profiles.sig"), DpapiKeyFile::new(dir.join("k")));
        let store = ProfileStore::new(&dir).with_integrity(Box::new(signer));
        store.save(&Settings::default()).unwrap();
        assert!(matches!(
            store.load().unwrap(),
            LoadOutcome::Loaded { signature_verified: true, .. }
        ));

        let edited = fs::read_to_string(dir.join("profiles.json")).unwrap().replace("true", "false");
        fs::write(dir.join("profiles.json"), edited).unwrap();
        assert!(matches!(
            store.load().unwrap(),
            LoadOutcome::Loaded { signature_verified: false, .. }
        ));
    }

    /// Opt-in: does the real signature in a real data folder verify? Reads the real DPAPI key
    /// and prints nothing from it. `OASIS_REAL_DATA_DIR=C:\Users\<you>\AppData\Roaming\ProjectOASIS`
    #[test]
    fn real_profiles_signature_verifies_when_a_data_folder_is_supplied() {
        let Ok(dir) = std::env::var("OASIS_REAL_DATA_DIR") else { return };
        let dir = PathBuf::from(dir);
        let signer = HmacSigner::new(
            dir.join("profiles.sig"),
            DpapiKeyFile::new(dir.join("secrets").join("profile_hmac.key")),
        );
        let json = fs::read_to_string(dir.join("profiles.json")).unwrap();
        assert!(signer.verify(&json), "the C# signature did not verify");
    }
}
