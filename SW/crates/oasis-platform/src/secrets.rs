//! The secrets OASIS keeps, by name, the same on every OS. Port of the names in
//! `Platform/CredentialStore.cs`. Backends map a name to storage (Windows: a DPAPI file in
//! `secrets/`, with the C# file name and entropy so existing files keep working).

use crate::ports::{PlatformError, PlatformResult, SecretStore};

pub const GEMINI_API_KEY: &str = "gemini";
pub const CURSOR_API_KEY: &str = "cursor";
pub const GROQ_API_KEY: &str = "groq";
pub const OUTLOOK_TOKEN: &str = "outlook-token";
pub const GOOGLE_TOKEN: &str = "google-token";
pub const GOOGLE_CLIENT_SECRET: &str = "google-client-secret";
pub const MARKETPLACE_SESSION: &str = "marketplace-session";
pub const MARKETPLACE_ANON_KEY: &str = "marketplace-anon";
pub const SPOTIFY_TOKEN: &str = "spotify-token";
pub const BROWSER_BRIDGE_TOKEN: &str = "bridge-token";

/// Stable per-user token for the localhost browser bridge. It stops other websites and
/// extensions spoofing YouTube events; it does not stop malware running as the same user.
/// An existing token of at least 32 characters is kept, anything else is replaced.
pub fn get_or_create_bridge_token(store: &dyn SecretStore) -> PlatformResult<String> {
    if let Some(existing) = store.get(BROWSER_BRIDGE_TOKEN)? {
        if existing.trim().len() >= 32 {
            return Ok(existing);
        }
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|e| PlatformError::Unavailable(format!("No random source: {e}")))?;
    let token: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    store.set(BROWSER_BRIDGE_TOKEN, &token)?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Memory(Mutex<HashMap<String, String>>);
    impl SecretStore for Memory {
        fn get(&self, key: &str) -> PlatformResult<Option<String>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        fn set(&self, key: &str, value: &str) -> PlatformResult<()> {
            self.0.lock().unwrap().insert(key.into(), value.into());
            Ok(())
        }
        fn clear(&self, key: &str) -> PlatformResult<()> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }

    #[test]
    fn bridge_token_is_created_once_as_64_hex_characters() {
        let store = Memory::default();
        let first = get_or_create_bridge_token(&store).unwrap();
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(get_or_create_bridge_token(&store).unwrap(), first);
    }

    #[test]
    fn a_short_token_is_replaced() {
        let store = Memory::default();
        store.set(BROWSER_BRIDGE_TOKEN, "short").unwrap();
        assert_eq!(get_or_create_bridge_token(&store).unwrap().len(), 64);
    }
}
