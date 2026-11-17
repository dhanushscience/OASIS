//! What a backend needs from an imported model. Import is local files only, so the one check
//! is that every required file is present.

use crate::engine::ModelError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModelKind {
    Stt,
    Llm,
    Tts,
}

impl ModelKind {
    pub const ALL: [ModelKind; 3] = [ModelKind::Stt, ModelKind::Llm, ModelKind::Tts];
}

/// One file a backend requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRequirement {
    /// Exact file name, e.g. `tokens.txt`.
    Named(&'static str),
    /// Any file with this extension (no dot), e.g. `gguf`.
    Extension(&'static str),
}

impl FileRequirement {
    fn matches(&self, file_name: &str) -> bool {
        match self {
            FileRequirement::Named(name) => file_name.eq_ignore_ascii_case(name),
            FileRequirement::Extension(ext) => file_name
                .rsplit_once('.')
                .is_some_and(|(_, e)| e.eq_ignore_ascii_case(ext)),
        }
    }

    fn describe(&self) -> String {
        match self {
            FileRequirement::Named(name) => (*name).to_string(),
            FileRequirement::Extension(ext) => format!("a .{ext} file"),
        }
    }
}

/// A backend's declaration. Loading the engine itself is the adapter's job; this is the part
/// that is pure and testable.
pub trait BackendSpec: Send + Sync {
    fn id(&self) -> &'static str;
    fn kind(&self) -> ModelKind;
    fn required_files(&self) -> &[FileRequirement];

    /// Check the supplied file names (not paths) against the requirements.
    fn validate_import(&self, file_names: &[&str]) -> Result<(), ModelError> {
        let missing: Vec<String> = self
            .required_files()
            .iter()
            .filter(|req| !file_names.iter().any(|name| req.matches(name)))
            .map(FileRequirement::describe)
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(ModelError::MissingFiles(missing))
        }
    }
}
