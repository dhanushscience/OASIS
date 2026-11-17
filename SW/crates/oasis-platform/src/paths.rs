//! Where OASIS keeps its files on each OS. Port of `Platform/OasisPaths.cs`.
//!
//! | | Roaming data (profiles, logs, secrets) | Local data (models, extension copy) |
//! |---|---|---|
//! | Windows | `%APPDATA%\ProjectOASIS` | `%LOCALAPPDATA%\ProjectOASIS` |
//! | macOS | `~/Library/Application Support/ProjectOASIS` | `~/Library/Application Support/ProjectOASIS` |
//! | Linux | `$XDG_CONFIG_HOME/ProjectOASIS` (`~/.config/…`) | `$XDG_DATA_HOME/ProjectOASIS` (`~/.local/share/…`) |
//!
//! On Windows the folder names are the C# app's (secrets, pad trust, logs are shared). The Rust app
//! keeps its own `profiles.json` in a `Rust` folder inside it (see [`OasisPaths::with_profiles_dir`]),
//! so the two apps never edit one profiles file. Models sit in *local* data so OneDrive never syncs gigabytes
//! of weights. Models are **never** downloaded automatically; [`OasisPaths::ensure_model_folders`]
//! only creates empty folders so the user has somewhere to put them.

use std::env;
use std::io;
use std::path::{Path, PathBuf};

pub const APP_FOLDER_NAME: &str = "ProjectOASIS";
pub const LEGACY_FOLDER_NAME: &str = "AdaptiveMacroPad";
pub const PROFILE_ICONS_FOLDER_NAME: &str = "profile_icons";
/// Where the Rust app keeps its own `profiles.json` (inside the roaming data folder), so it never
/// shares one with the C# app.
pub const RUST_PROFILES_FOLDER_NAME: &str = "Rust";

/// All paths, computed from two roots. Construct with [`OasisPaths::detect`] or, in tests,
/// [`OasisPaths::with_roots`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OasisPaths {
    pub app_data_root: PathBuf,
    pub local_app_data_root: PathBuf,
    /// The folder holding `profiles.json` and its signature. The roaming root, unless the app picks
    /// a folder of its own ([`OasisPaths::with_profiles_dir`]).
    pub profiles_dir: PathBuf,
}

fn env_path(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn home() -> Option<PathBuf> {
    env_path("HOME").or_else(|| env_path("USERPROFILE"))
}

impl OasisPaths {
    pub fn with_roots(
        app_data_root: impl Into<PathBuf>,
        local_app_data_root: impl Into<PathBuf>,
    ) -> OasisPaths {
        let app_data_root = app_data_root.into();
        OasisPaths {
            profiles_dir: app_data_root.clone(),
            app_data_root,
            local_app_data_root: local_app_data_root.into(),
        }
    }

    /// Keep `profiles.json` (and its signature) in `dir` instead of the roaming root.
    #[must_use]
    pub fn with_profiles_dir(mut self, dir: impl Into<PathBuf>) -> OasisPaths {
        self.profiles_dir = dir.into();
        self
    }

    /// The standard locations for this OS. `None` only if the OS gives no home directory.
    pub fn detect() -> Option<OasisPaths> {
        let (roaming, local) = if cfg!(target_os = "windows") {
            (env_path("APPDATA")?, env_path("LOCALAPPDATA")?)
        } else if cfg!(target_os = "macos") {
            let base = home()?.join("Library").join("Application Support");
            (base.clone(), base)
        } else {
            let h = home();
            let config =
                env_path("XDG_CONFIG_HOME").or_else(|| h.as_ref().map(|h| h.join(".config")))?;
            let data = env_path("XDG_DATA_HOME")
                .or_else(|| h.as_ref().map(|h| h.join(".local").join("share")))?;
            (config, data)
        };
        Some(OasisPaths::with_roots(
            roaming.join(APP_FOLDER_NAME),
            local.join(APP_FOLDER_NAME),
        ))
    }

    // Roaming data.
    pub fn profiles(&self) -> PathBuf {
        self.profiles_dir.join("profiles.json")
    }
    /// Where the first-run guide keeps its progress: beside the profiles, never inside the signed file.
    pub fn onboarding(&self) -> PathBuf {
        self.profiles_dir.join("onboarding.json")
    }
    pub fn profiles_signature(&self) -> PathBuf {
        self.profiles_dir.join("profiles.sig")
    }
    pub fn ble_log(&self) -> PathBuf {
        self.app_data_root.join("ble_log.txt")
    }
    pub fn trusted_device(&self) -> PathBuf {
        self.app_data_root.join("trusted_device.json")
    }
    pub fn security_log(&self) -> PathBuf {
        self.app_data_root.join("security.log")
    }
    pub fn intent_log(&self) -> PathBuf {
        self.app_data_root.join("intent.log")
    }
    pub fn overlay_ack_log(&self) -> PathBuf {
        self.app_data_root.join("overlay-ack.log")
    }
    /// Where each recording's time went (durations only, never the words); see the voice engine.
    pub fn voice_timing_log(&self) -> PathBuf {
        self.app_data_root.join("voice-timing.log")
    }
    /// Every dictation's text at each cleanup stage, raw to typed. Holds what was said; temporary,
    /// for tuning the rules (owner, 2026-11-12).
    pub fn dictation_trace_log(&self) -> PathBuf {
        self.app_data_root.join("dictation-trace.log")
    }
    pub fn crash_log(&self) -> PathBuf {
        self.app_data_root.join("crash.log")
    }
    pub fn secrets_folder(&self) -> PathBuf {
        self.app_data_root.join("secrets")
    }
    pub fn profile_icons_folder(&self) -> PathBuf {
        self.app_data_root.join(PROFILE_ICONS_FOLDER_NAME)
    }

    // Local data.
    pub fn extensions_root(&self) -> PathBuf {
        self.local_app_data_root.join("Extensions")
    }
    /// The kept dictations (a recording and its text each), when the user switched that on.
    pub fn dictation_history_folder(&self) -> PathBuf {
        self.local_app_data_root.join("DictationHistory")
    }
    pub fn models_folder(&self) -> PathBuf {
        self.local_app_data_root.join("Models")
    }
    pub fn stt_folder(&self) -> PathBuf {
        self.models_folder().join("stt")
    }
    pub fn llm_folder(&self) -> PathBuf {
        self.models_folder().join("llm")
    }
    pub fn tts_folder(&self) -> PathBuf {
        self.models_folder().join("tts")
    }
    pub fn vad_folder(&self) -> PathBuf {
        self.models_folder().join("vad")
    }
    pub fn silero_vad_model(&self) -> PathBuf {
        self.vad_folder().join("silero_vad.onnx")
    }
    pub fn stt_model_folder(&self, folder_name: &str) -> PathBuf {
        self.stt_folder().join(folder_name)
    }
    pub fn llm_model(&self, file_name: &str) -> PathBuf {
        self.llm_folder().join(file_name)
    }

    /// Creates the empty Models / stt / llm / tts / vad folders. Never downloads anything.
    pub fn ensure_model_folders(&self) -> io::Result<()> {
        for dir in [
            self.stt_folder(),
            self.llm_folder(),
            self.tts_folder(),
            self.vad_folder(),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// The pre-rename data folder of the C# app (`AdaptiveMacroPad`), next to the current one.
    pub fn legacy_app_data_root(&self) -> PathBuf {
        self.app_data_root
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join(LEGACY_FOLDER_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_csharp_app() {
        let p = OasisPaths::with_roots("/r/ProjectOASIS", "/l/ProjectOASIS");
        assert_eq!(p.profiles(), PathBuf::from("/r/ProjectOASIS/profiles.json"));
        // the Rust app keeps its profiles apart, and nothing else moves
        let own = p.clone().with_profiles_dir("/r/ProjectOASIS/Rust");
        assert_eq!(own.profiles(), PathBuf::from("/r/ProjectOASIS/Rust/profiles.json"));
        assert_eq!(own.profiles_signature(), PathBuf::from("/r/ProjectOASIS/Rust/profiles.sig"));
        assert_eq!(own.trusted_device(), p.trusted_device());
        assert_eq!(own.secrets_folder(), p.secrets_folder());
        assert_eq!(p.crash_log(), PathBuf::from("/r/ProjectOASIS/crash.log"));
        assert_eq!(p.stt_folder(), PathBuf::from("/l/ProjectOASIS/Models/stt"));
        assert_eq!(
            p.silero_vad_model(),
            PathBuf::from("/l/ProjectOASIS/Models/vad/silero_vad.onnx")
        );
        assert_eq!(
            p.legacy_app_data_root(),
            PathBuf::from("/r/AdaptiveMacroPad")
        );
    }

    #[test]
    fn detect_finds_a_home_on_this_machine() {
        let p = OasisPaths::detect().expect("a home directory");
        assert!(p.app_data_root.ends_with(APP_FOLDER_NAME));
        assert!(p.local_app_data_root.ends_with(APP_FOLDER_NAME));
    }
}
