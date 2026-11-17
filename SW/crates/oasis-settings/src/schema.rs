//! Port of `SettingsSchema` (`Oasis.Core/Settings/SettingsSnapshot.cs`) and
//! `IntegrityLoadPolicy`.

pub const CURRENT_SCHEMA: u32 = 2;
pub const DEFAULT_STT_MODEL_FOLDER: &str = "parakeet-tdt-0.6b-v2";
pub const DEFAULT_LLM_MODEL_FILE_NAME: &str = "gemma-4-E2B-it-Q3_K_M.gguf";

/// A stored speech-model name from an engine OASIS no longer has: Whisper GGML files, or a
/// Moonshine folder. Such a name is replaced by [`DEFAULT_STT_MODEL_FOLDER`].
pub fn is_retired_stt_model(path_or_name: &str) -> bool {
    let normalised = path_or_name.replace('\\', "/");
    let name = normalised.rsplit('/').next().unwrap_or("").to_ascii_lowercase();
    name.contains("ggml")
        || name.contains("whisper")
        || name.contains("moonshine")
        || name.ends_with(".bin")
}

/// H1: the profile signature is tamper detection (hand-edits, interrupted save), not a
/// security boundary. Never persist, which re-signs, while a mismatch is unresolved.
pub fn may_persist(signature_verified: bool, user_accepted_edited: bool) -> bool {
    signature_verified || user_accepted_edited
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_models_are_recognised_by_file_name() {
        assert!(is_retired_stt_model("ggml-tiny.bin"));
        assert!(is_retired_stt_model(r"C:\models\Whisper-small"));
        assert!(is_retired_stt_model("models/moonshine-base-en"));
        assert!(is_retired_stt_model("anything.BIN"));
        assert!(!is_retired_stt_model(DEFAULT_STT_MODEL_FOLDER));
        // Only the last path segment counts.
        assert!(!is_retired_stt_model("whisper-folder/parakeet-tdt-0.6b-v2"));
    }

    #[test]
    fn persisting_needs_a_verified_signature_or_the_users_acceptance() {
        assert!(may_persist(true, false));
        assert!(may_persist(false, true));
        assert!(!may_persist(false, false));
    }
}
