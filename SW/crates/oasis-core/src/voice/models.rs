//! The model manifest built into the app (`docs/VOICE_MODEL_SETUP.md` §4.3): every voice model
//! file, where it is downloaded from, its size and SHA-256, and where it goes under the Models
//! folder. A downloaded file is used only when its SHA-256 matches; since the manifest ships inside
//! the signed app, the trust chain is the app's own.
//!
//! The list itself is `model_files.rs`, written by `scripts/publish-models.ps1`.

use super::placement::ModelSet;

/// One file of a models release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    /// Its name in the release.
    pub asset: &'static str,
    /// Where it goes under the Models folder, with `/`. For a zip, the folder it is unpacked into.
    pub path: &'static str,
    pub bytes: u64,
    /// Lower-case hex.
    pub sha256: &'static str,
    pub group: Group,
    /// A zip, unpacked into `path` once checked.
    pub unzip: bool,
}

/// What a file is for; the sets are made of groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// Parakeet TDT-CTC 110M (LIGHT).
    SpeechLight,
    /// Parakeet TDT 0.6b v2 (BALANCED, QUALITY).
    SpeechBalanced,
    /// S1-mini and the llama.cpp Vulkan runtime (QUALITY).
    Polish,
}

include!("model_files.rs");

impl Group {
    fn in_set(self, set: ModelSet) -> bool {
        matches!(
            (self, set),
            (Group::SpeechLight, ModelSet::Light) | (Group::SpeechBalanced, ModelSet::Balanced | ModelSet::Quality) | (Group::Polish, ModelSet::Quality)
        )
    }
}

/// The files `set` needs, in download order.
pub fn files_for(set: ModelSet) -> impl Iterator<Item = &'static ModelFile> {
    FILES.iter().filter(move |f| f.group.in_set(set))
}

/// Where `file` is downloaded from.
pub fn url(file: &ModelFile) -> String {
    format!("{BASE_URL}{}", file.asset)
}

/// The speech model folder name a set uses (the `Voice.ModelFileName` setting).
pub fn speech_folder(set: ModelSet) -> &'static str {
    match set {
        ModelSet::Light => "parakeet-tdt-ctc-110m",
        ModelSet::Balanced | ModelSet::Quality => "parakeet-tdt-0.6b-v2",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_is_well_formed() {
        assert!(!FILES.is_empty());
        for f in FILES {
            assert_eq!(f.sha256.len(), 64, "{}", f.asset);
            assert!(f.sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{}", f.asset);
            assert!(f.bytes > 0 && !f.path.starts_with('/') && !f.path.contains('\\') && !f.path.contains(".."), "{}", f.asset);
            assert!(f.asset.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)), "{}", f.asset);
        }
        let mut assets: Vec<&str> = FILES.iter().map(|f| f.asset).collect();
        assets.sort_unstable();
        assets.dedup();
        assert_eq!(assets.len(), FILES.len(), "asset names are unique in a release");
        assert!(BASE_URL.starts_with("https://github.com/") && BASE_URL.ends_with('/'));
    }

    #[test]
    fn each_set_has_its_speech_model_and_only_quality_polishes() {
        for set in [ModelSet::Light, ModelSet::Balanced, ModelSet::Quality] {
            let folder = speech_folder(set);
            assert!(files_for(set).any(|f| f.path.starts_with(&format!("stt/{folder}/"))), "{set:?}");
            assert_eq!(files_for(set).any(|f| f.group == Group::Polish), set == ModelSet::Quality, "{set:?}");
        }
        assert!(files_for(ModelSet::Quality).any(|f| f.unzip), "the runtime is a zip");
    }
}
