//! Reads and writes `profiles.json` in the data folder. Port of `FileSettingsRepository`,
//! `LastGoodSettingsFile` and the load rules of the C# app. Same file names (D4).
//!
//! - `load` falls back to `profiles.json.last-good` when the main file is not valid JSON, and
//!   says so, so the UI can tell the user instead of silently using older data. A schema newer
//!   than this build is an error, never a reason to fall back.
//! - With a [`SettingsIntegrity`] set, `load` checks the signature. A mismatch is reported and
//!   `save` refuses until the user accepts the edited file (`accept_edited`), as in
//!   `IntegrityLoadPolicy`. A missing `.sig` verifies, as in C#.
//! - `save` writes atomically, signs, then refreshes `.last-good` (the C# order).
//! - `save` refuses if `profiles.json` changed on disk since this store last read or wrote it
//!   (the C# app, or a person, edited it meanwhile): overwriting would silently throw that work
//!   away. The caller reloads and applies its change again.
//! - Without a [`SettingsIntegrity`], `.sig` is neither checked nor written.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use oasis_settings::{may_persist, Loaded, Settings, SettingsError};
use sha2::{Digest, Sha256};

use crate::atomic_file;

pub type Fingerprint = [u8; 32];

fn fingerprint(text: &str) -> Fingerprint {
    Sha256::digest(text.as_bytes()).into()
}

/// Why a save did not happen.
#[derive(Debug)]
pub enum SaveError {
    /// The file failed its signature check and the user has not accepted it. Saving would sign
    /// over someone else's edit.
    Blocked,
    /// `profiles.json` was changed by another program since it was loaded. Reload first.
    ChangedOnDisk,
    Io(io::Error),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Blocked => f.write_str(
                "profiles.json was edited outside OASIS and has not been accepted, so it will not be overwritten.",
            ),
            SaveError::ChangedOnDisk => f.write_str(
                "profiles.json was changed by another program (the C# OASIS app?) since it was loaded. Reload it, then make the change again.",
            ),
            SaveError::Io(e) => write!(f, "Could not write profiles.json: {e}"),
        }
    }
}

impl std::error::Error for SaveError {}

impl From<io::Error> for SaveError {
    fn from(e: io::Error) -> SaveError {
        SaveError::Io(e)
    }
}

/// Port of `ISettingsIntegrity`: tamper detection for `profiles.json`.
pub trait SettingsIntegrity: Send + Sync {
    /// `true` if `json` matches its signature, or there is no signature yet.
    fn verify(&self, json: &str) -> bool;
    fn sign(&self, json: &str) -> io::Result<()>;
}

#[derive(Debug)]
pub enum LoadOutcome {
    /// Fresh install: nothing on disk yet.
    NoFile,
    Loaded { loaded: Loaded, signature_verified: bool },
    /// The main file was bad (`reason`); this is the last-good copy.
    RecoveredFromLastGood { loaded: Loaded, signature_verified: bool, reason: String },
}

pub struct ProfileStore {
    path: PathBuf,
    integrity: Option<Box<dyn SettingsIntegrity>>,
    /// Set by a load whose signature did not match; cleared by `accept_edited`.
    unresolved_mismatch: AtomicBool,
    /// What the file held when this store last read or wrote it; `None` if it was absent or had
    /// to be recovered from the last-good copy (then there is nothing to protect).
    known: Mutex<Option<Fingerprint>>,
}

impl ProfileStore {
    pub fn new(data_dir: &Path) -> ProfileStore {
        ProfileStore {
            path: data_dir.join("profiles.json"),
            integrity: None,
            unresolved_mismatch: AtomicBool::new(false),
            known: Mutex::new(None),
        }
    }

    fn remember(&self, fingerprint: Option<Fingerprint>) {
        *self.known.lock().unwrap_or_else(|e| e.into_inner()) = fingerprint;
    }

    pub fn with_integrity(mut self, integrity: Box<dyn SettingsIntegrity>) -> ProfileStore {
        self.integrity = Some(integrity);
        self
    }

    fn last_good(&self) -> PathBuf {
        self.path.with_file_name("profiles.json.last-good")
    }

    fn verified(&self, text: &str) -> bool {
        self.integrity.as_ref().is_none_or(|i| i.verify(text))
    }

    pub fn load(&self) -> Result<LoadOutcome, SettingsError> {
        let main = match fs::read_to_string(&self.path) {
            Ok(text) => oasis_settings::load(&text).map(|loaded| (loaded, text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound && !self.last_good().exists() => {
                return Ok(LoadOutcome::NoFile)
            }
            Err(e) => Err(SettingsError::Parse(e.to_string())),
        };
        let outcome = match main {
            Ok((loaded, text)) => {
                self.remember(Some(fingerprint(&text)));
                LoadOutcome::Loaded { loaded, signature_verified: self.verified(&text) }
            }
            // A newer schema is not corruption: do not paper over it with an older copy.
            Err(e @ SettingsError::TooNew { .. }) => return Err(e),
            Err(first) => {
                let recovered = fs::read_to_string(self.last_good())
                    .ok()
                    .and_then(|text| oasis_settings::load(&text).ok().map(|l| (l, text)));
                let Some((loaded, text)) = recovered else { return Err(first) };
                // The main file is damaged; saving over it is the repair.
                self.remember(None);
                LoadOutcome::RecoveredFromLastGood {
                    loaded,
                    signature_verified: self.verified(&text),
                    reason: first.to_string(),
                }
            }
        };
        let verified = match &outcome {
            LoadOutcome::Loaded { signature_verified, .. }
            | LoadOutcome::RecoveredFromLastGood { signature_verified, .. } => *signature_verified,
            LoadOutcome::NoFile => true,
        };
        self.unresolved_mismatch.store(!verified, Ordering::SeqCst);
        Ok(outcome)
    }

    /// The fingerprint of `profiles.json` if it no longer matches what this store last read or wrote
    /// (another program saved it). `None` while it matches, is unreadable, or nothing was loaded yet.
    pub fn changed_on_disk(&self) -> Option<Fingerprint> {
        let expected = (*self.known.lock().unwrap_or_else(|e| e.into_inner()))?;
        let now = fingerprint(&fs::read_to_string(&self.path).ok()?);
        (now != expected).then_some(now)
    }

    /// The user chose to keep a file whose signature did not match. Saving is allowed again.
    pub fn accept_edited(&self) {
        self.unresolved_mismatch.store(false, Ordering::SeqCst);
    }

    pub fn save(&self, settings: &Settings) -> Result<(), SaveError> {
        let unresolved = self.unresolved_mismatch.load(Ordering::SeqCst);
        if !may_persist(!unresolved, false) {
            return Err(SaveError::Blocked);
        }
        // Hold the lock across the check and the write so two saves cannot both pass the check.
        let mut known = self.known.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(expected) = *known {
            if let Ok(on_disk) = fs::read_to_string(&self.path) {
                if fingerprint(&on_disk) != expected {
                    return Err(SaveError::ChangedOnDisk);
                }
            }
        }
        let json = oasis_settings::to_json(settings);
        atomic_file::write_utf8(&self.path, &json)?;
        // The file is now the new one even if signing fails below, so remember it now.
        *known = Some(fingerprint(&json));
        if let Some(integrity) = &self.integrity {
            integrity.sign(&json)?;
        }
        atomic_file::write_utf8(&self.last_good(), &json)?;
        *known = Some(fingerprint(&json));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Signs by writing the text's length; verifies against that.
    struct LengthIntegrity {
        sig: PathBuf,
    }
    impl SettingsIntegrity for LengthIntegrity {
        fn verify(&self, json: &str) -> bool {
            match fs::read_to_string(&self.sig) {
                Ok(s) => s == format!("len:{}", json.len()),
                Err(_) => true,
            }
        }
        fn sign(&self, json: &str) -> io::Result<()> {
            fs::write(&self.sig, format!("len:{}", json.len()))
        }
    }

    fn signed_store(dir: &Path) -> ProfileStore {
        ProfileStore::new(dir)
            .with_integrity(Box::new(LengthIntegrity { sig: dir.join("profiles.sig") }))
    }

    fn settings(y: i32) -> Settings {
        Settings { overlay_y: y.into(), ..Settings::default() }
    }

    struct FailingSigner;
    impl SettingsIntegrity for FailingSigner {
        fn verify(&self, _: &str) -> bool {
            true
        }
        fn sign(&self, _: &str) -> io::Result<()> {
            Err(io::Error::other("no key"))
        }
    }

    #[test]
    fn a_signing_failure_does_not_lock_later_saves() {
        let dir = temp_dir("signfail");
        ProfileStore::new(&dir).save(&settings(0)).unwrap();
        let store = ProfileStore::new(&dir).with_integrity(Box::new(FailingSigner));
        store.load().unwrap();
        assert!(matches!(store.save(&settings(1)), Err(SaveError::Io(_))));
        // The file was written; the store knows it, so the next save is not "changed on disk".
        assert!(matches!(store.save(&settings(2)), Err(SaveError::Io(_))));
    }

    #[test]
    fn empty_folder_is_a_fresh_install() {
        let store = ProfileStore::new(&temp_dir("fresh"));
        assert!(matches!(store.load().unwrap(), LoadOutcome::NoFile));
    }

    #[test]
    fn save_then_load_round_trips_and_leaves_sig_alone_without_integrity() {
        let dir = temp_dir("roundtrip");
        fs::write(dir.join("profiles.sig"), "old-sig").unwrap();
        let store = ProfileStore::new(&dir);
        store.save(&settings(9)).unwrap();
        let LoadOutcome::Loaded { loaded, signature_verified } = store.load().unwrap() else {
            panic!("expected Loaded")
        };
        assert!(signature_verified);
        assert_eq!(loaded.settings.overlay_y, 9.into());
        assert_eq!(fs::read_to_string(dir.join("profiles.sig")).unwrap(), "old-sig");
    }

    #[test]
    fn saved_file_verifies_and_a_hand_edit_does_not() {
        let dir = temp_dir("signed");
        let store = signed_store(&dir);
        store.save(&settings(1)).unwrap();
        assert!(matches!(
            store.load().unwrap(),
            LoadOutcome::Loaded { signature_verified: true, .. }
        ));

        let edited = fs::read_to_string(dir.join("profiles.json")).unwrap().replace("1", "22");
        fs::write(dir.join("profiles.json"), edited).unwrap();
        assert!(matches!(
            store.load().unwrap(),
            LoadOutcome::Loaded { signature_verified: false, .. }
        ));
    }

    #[test]
    fn save_is_refused_after_a_mismatch_until_the_user_accepts() {
        let dir = temp_dir("mismatch");
        let store = signed_store(&dir);
        store.save(&settings(1)).unwrap();
        fs::write(dir.join("profiles.json"), r#"{"SchemaVersion":2,"OverlayY":5}"#).unwrap();
        store.load().unwrap();

        assert!(matches!(store.save(&settings(2)), Err(SaveError::Blocked)));

        store.accept_edited();
        store.save(&settings(2)).unwrap();
        assert!(matches!(
            store.load().unwrap(),
            LoadOutcome::Loaded { signature_verified: true, .. }
        ));
    }

    #[test]
    fn save_is_refused_if_another_program_changed_the_file_since_it_was_loaded() {
        let dir = temp_dir("conflict");
        let store = ProfileStore::new(&dir);
        store.save(&settings(1)).unwrap();
        store.load().unwrap();

        // The C# app (or a person) saves its own change in between.
        let theirs = oasis_settings::to_json(&settings(77));
        fs::write(dir.join("profiles.json"), &theirs).unwrap();

        assert!(matches!(store.save(&settings(2)), Err(SaveError::ChangedOnDisk)));
        assert_eq!(fs::read_to_string(dir.join("profiles.json")).unwrap(), theirs, "their file is untouched");

        // After reloading, the change can be applied on top of theirs.
        let LoadOutcome::Loaded { loaded, .. } = store.load().unwrap() else { panic!("expected Loaded") };
        assert_eq!(loaded.settings.overlay_y, 77.into());
        store.save(&settings(2)).unwrap();
    }

    #[test]
    fn our_own_saves_do_not_count_as_somebody_elses_change() {
        let dir = temp_dir("own-saves");
        let store = ProfileStore::new(&dir);
        store.save(&settings(1)).unwrap();
        store.save(&settings(2)).unwrap();
        store.save(&settings(3)).unwrap();
        assert!(matches!(store.load().unwrap(), LoadOutcome::Loaded { .. }));
    }

    #[test]
    fn a_first_save_into_an_empty_folder_has_nothing_to_conflict_with() {
        let store = ProfileStore::new(&temp_dir("first"));
        store.save(&settings(1)).unwrap();
    }

    #[test]
    fn saving_over_a_damaged_file_is_allowed_because_that_is_the_repair() {
        let dir = temp_dir("repair");
        let store = ProfileStore::new(&dir);
        store.save(&settings(3)).unwrap();
        fs::write(dir.join("profiles.json"), "{ truncated").unwrap();
        assert!(matches!(store.load().unwrap(), LoadOutcome::RecoveredFromLastGood { .. }));
        store.save(&settings(4)).unwrap();
        assert!(matches!(store.load().unwrap(), LoadOutcome::Loaded { .. }));
    }

    #[test]
    fn missing_signature_file_verifies() {
        let dir = temp_dir("nosig");
        fs::write(dir.join("profiles.json"), r#"{"SchemaVersion":2}"#).unwrap();
        assert!(matches!(
            signed_store(&dir).load().unwrap(),
            LoadOutcome::Loaded { signature_verified: true, .. }
        ));
    }

    #[test]
    fn corrupt_main_file_recovers_from_last_good_and_says_why() {
        let dir = temp_dir("recover");
        let store = ProfileStore::new(&dir);
        store.save(&settings(3)).unwrap();
        fs::write(dir.join("profiles.json"), "{ truncated").unwrap();
        let LoadOutcome::RecoveredFromLastGood { loaded, reason, .. } = store.load().unwrap()
        else {
            panic!("expected recovery")
        };
        assert_eq!(loaded.settings.overlay_y, 3.into());
        assert!(!reason.is_empty());
    }

    #[test]
    fn newer_schema_is_not_replaced_by_last_good() {
        let dir = temp_dir("newer");
        let store = ProfileStore::new(&dir);
        store.save(&Settings::default()).unwrap();
        fs::write(dir.join("profiles.json"), r#"{"SchemaVersion": 99}"#).unwrap();
        assert!(matches!(store.load(), Err(SettingsError::TooNew { found: 99 })));
    }

    #[test]
    fn corrupt_with_no_last_good_is_an_error() {
        let dir = temp_dir("nolast");
        fs::write(dir.join("profiles.json"), "garbage").unwrap();
        assert!(ProfileStore::new(&dir).load().is_err());
    }

    #[test]
    fn store_is_usable_across_threads() {
        let dir = temp_dir("threads");
        let store = Arc::new(ProfileStore::new(&dir));
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || store.save(&settings(i)).unwrap())
            })
            .collect();
        handles.into_iter().for_each(|h| h.join().unwrap());
        assert!(matches!(store.load().unwrap(), LoadOutcome::Loaded { .. }));
    }
}
