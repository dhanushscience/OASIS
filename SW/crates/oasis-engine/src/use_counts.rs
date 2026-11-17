//! How often each profile's buttons are pressed, so the window can order profiles by use.
//!
//! Kept in `profile_use.json` beside `profiles.json`, not inside it: the profiles file is signed,
//! and rewriting it on every press would churn the signature.

use std::collections::BTreeMap;
use std::path::PathBuf;

use oasis_platform::atomic_file::write_utf8;

pub struct UseCounts {
    path: PathBuf,
    counts: BTreeMap<String, u64>,
}

impl UseCounts {
    /// Reads the file; a missing or unreadable one starts the counts at zero.
    pub fn load(path: PathBuf) -> UseCounts {
        let counts = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        UseCounts { path, counts }
    }

    /// Counts one press for `profile` and saves. A failed save keeps the count in memory.
    pub fn bump(&mut self, profile: &str) -> std::io::Result<()> {
        *self.counts.entry(profile.to_string()).or_default() += 1;
        let text = serde_json::to_string_pretty(&self.counts).map_err(std::io::Error::other)?;
        write_utf8(&self.path, &text)
    }

    pub fn counts(&self) -> BTreeMap<String, u64> {
        self.counts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_survive_a_restart_and_a_bad_file_starts_at_zero() {
        let dir = std::env::temp_dir().join(format!("oasis-use-counts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("profile_use.json");
        let _ = std::fs::remove_file(&path);

        let mut counts = UseCounts::load(path.clone());
        counts.bump("VS Code").unwrap();
        counts.bump("VS Code").unwrap();
        counts.bump("Chrome").unwrap();
        let again = UseCounts::load(path.clone()).counts();
        assert_eq!((again["VS Code"], again["Chrome"]), (2, 1));

        std::fs::write(&path, "not json").unwrap();
        assert!(UseCounts::load(path).counts().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
