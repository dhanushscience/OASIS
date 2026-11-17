//! Installed models and which one is selected per kind. Defaults are registered the same way
//! as imports (DECISIONS D9), so replacing one is the same path as adding one.

use std::collections::BTreeMap;

use oasis_core::capability::{Capability, CapabilityState};

use crate::backend::{BackendSpec, ModelKind};
use crate::engine::ModelError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledModel {
    pub id: String,
    pub kind: ModelKind,
    pub backend: String,
}

#[derive(Default)]
pub struct ModelRegistry {
    backends: Vec<Box<dyn BackendSpec>>,
    models: Vec<InstalledModel>,
    selected: BTreeMap<ModelKind, String>,
}

impl ModelRegistry {
    pub fn new() -> ModelRegistry {
        ModelRegistry::default()
    }

    pub fn register_backend(&mut self, backend: Box<dyn BackendSpec>) {
        self.backends.push(backend);
    }

    pub fn backend(&self, id: &str) -> Option<&dyn BackendSpec> {
        self.backends.iter().find(|b| b.id() == id).map(|b| b.as_ref())
    }

    /// Validate a local file set against `backend_id` and record the model. The caller (the
    /// platform layer) copies the files only after this returns `Ok`. A model id that already
    /// exists is replaced. The first model of a kind becomes the selection.
    pub fn import(
        &mut self,
        model_id: &str,
        backend_id: &str,
        file_names: &[&str],
    ) -> Result<InstalledModel, ModelError> {
        let backend = self
            .backend(backend_id)
            .ok_or_else(|| ModelError::UnknownBackend(backend_id.to_string()))?;
        backend.validate_import(file_names)?;
        let model = InstalledModel {
            id: model_id.to_string(),
            kind: backend.kind(),
            backend: backend_id.to_string(),
        };
        self.models.retain(|m| !(m.id == model.id && m.kind == model.kind));
        self.models.push(model.clone());
        self.selected.entry(model.kind).or_insert_with(|| model.id.clone());
        Ok(model)
    }

    pub fn models(&self, kind: ModelKind) -> impl Iterator<Item = &InstalledModel> {
        self.models.iter().filter(move |m| m.kind == kind)
    }

    /// Select an installed model. Returns false if no such model of that kind exists.
    pub fn select(&mut self, kind: ModelKind, model_id: &str) -> bool {
        if self.models(kind).any(|m| m.id == model_id) {
            self.selected.insert(kind, model_id.to_string());
            true
        } else {
            false
        }
    }

    pub fn selected(&self, kind: ModelKind) -> Option<&InstalledModel> {
        let id = self.selected.get(&kind)?;
        self.models(kind).find(|m| &m.id == id)
    }

    /// The capability this kind feeds, as the UI should show it. No silent fallback: if the
    /// selected model is gone the state says so.
    pub fn capability_state(&self, kind: ModelKind) -> (Capability, CapabilityState) {
        let capability = match kind {
            ModelKind::Stt => Capability::SpeechToText,
            ModelKind::Llm => Capability::LocalPlanner,
            ModelKind::Tts => Capability::TextToSpeech,
        };
        let state = match self.selected(kind) {
            Some(_) => CapabilityState::Available,
            None => CapabilityState::missing("No model selected. Import one in settings."),
        };
        (capability, state)
    }
}
