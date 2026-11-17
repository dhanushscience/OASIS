//! Port of `DpapiHmacSigner` minus DPAPI: HMAC-SHA256 over the exact text of `profiles.json`,
//! base64 in `profiles.sig`. The key comes from a [`KeyProvider`], so this part is the same on
//! every OS and is tested against values computed by .NET. On Windows the key is the
//! DPAPI-protected `secrets/profile_hmac.key` (see `windows::dpapi`).
//!
//! Tamper detection for hand-edits and interrupted saves, not a security boundary: code running
//! as the same user can read the key (C# decision H1).

use std::fs;
use std::io;
use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::profile_store::SettingsIntegrity;

pub trait KeyProvider: Send + Sync {
    /// The signing key, creating and storing one first if there is none yet.
    fn key(&self) -> io::Result<Vec<u8>>;
}

pub struct HmacSigner<K: KeyProvider> {
    signature_path: PathBuf,
    keys: K,
}

impl<K: KeyProvider> HmacSigner<K> {
    pub fn new(signature_path: PathBuf, keys: K) -> HmacSigner<K> {
        HmacSigner { signature_path, keys }
    }

    fn mac(&self, json: &str) -> io::Result<Hmac<Sha256>> {
        let key = self.keys.key()?;
        let mut mac = Hmac::<Sha256>::new_from_slice(&key)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        mac.update(json.as_bytes());
        Ok(mac)
    }
}

impl<K: KeyProvider> SettingsIntegrity for HmacSigner<K> {
    /// No signature file verifies (a fresh or pre-signature install), as in C#. A key that
    /// cannot be read does not: the file cannot be shown to be untouched.
    fn verify(&self, json: &str) -> bool {
        let Ok(text) = fs::read_to_string(&self.signature_path) else {
            return !self.signature_path.exists();
        };
        let Ok(expected) = STANDARD.decode(text.trim()) else { return false };
        self.mac(json).is_ok_and(|mac| mac.verify_slice(&expected).is_ok())
    }

    fn sign(&self, json: &str) -> io::Result<()> {
        let signature = STANDARD.encode(self.mac(json)?.finalize().into_bytes());
        crate::atomic_file::write_utf8(&self.signature_path, &signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedKey(Vec<u8>);
    impl KeyProvider for FixedKey {
        fn key(&self) -> io::Result<Vec<u8>> {
            Ok(self.0.clone())
        }
    }

    struct NoKey;
    impl KeyProvider for NoKey {
        fn key(&self) -> io::Result<Vec<u8>> {
            Err(io::Error::other("no key"))
        }
    }

    fn signer(name: &str) -> HmacSigner<FixedKey> {
        let dir = std::env::temp_dir().join(format!("oasis-hmac-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        HmacSigner::new(dir.join("profiles.sig"), FixedKey((1..=32).collect()))
    }

    const JSON: &str = r#"{"OverlayY": 1}"#;
    /// Computed with .NET `HMACSHA256` (key bytes 1..=32), base64.
    const DOTNET_SIGNATURE: &str = "lHBagOI/ctFBXhfj2KfyikDJqhcmWfVRPG/8Do/covM=";

    #[test]
    fn signature_matches_what_dotnet_computes() {
        let s = signer("vector");
        s.sign(JSON).unwrap();
        assert_eq!(fs::read_to_string(&s.signature_path).unwrap(), DOTNET_SIGNATURE);
    }

    #[test]
    fn verifies_a_dotnet_signature_with_surrounding_whitespace() {
        let s = signer("verify");
        fs::create_dir_all(s.signature_path.parent().unwrap()).unwrap();
        fs::write(&s.signature_path, format!("{DOTNET_SIGNATURE}\r\n")).unwrap();
        assert!(s.verify(JSON));
    }

    #[test]
    fn any_change_to_the_text_fails_verification() {
        let s = signer("tamper");
        s.sign(JSON).unwrap();
        assert!(s.verify(JSON));
        assert!(!s.verify(r#"{"OverlayY": 2}"#));
        assert!(!s.verify(&format!("{JSON}\n")));
    }

    #[test]
    fn missing_signature_file_verifies() {
        assert!(signer("missing").verify(JSON));
    }

    #[test]
    fn a_damaged_signature_file_does_not_verify() {
        let s = signer("damaged");
        fs::create_dir_all(s.signature_path.parent().unwrap()).unwrap();
        fs::write(&s.signature_path, "not base64!!").unwrap();
        assert!(!s.verify(JSON));
    }

    #[test]
    fn an_unreadable_key_does_not_verify_an_existing_signature() {
        let s = signer("nokey");
        s.sign(JSON).unwrap();
        let broken = HmacSigner::new(s.signature_path.clone(), NoKey);
        assert!(!broken.verify(JSON));
        assert!(broken.sign(JSON).is_err());
    }
}
