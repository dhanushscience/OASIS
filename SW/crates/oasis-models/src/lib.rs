//! # oasis-models
//!
//! STT, LLM and TTS are replaceable (DECISIONS D8). Each capability has one engine trait; each
//! backend (sherpa-onnx, llama.cpp, Piper, …) declares which files it needs. A user imports
//! local files, the registry checks them against the backend, and the selected model per
//! kind is what the app loads.
//!
//! | Module | Role |
//! |---|---|
//! | [`engine`] | `SttEngine`, `LlmEngine`, `TtsEngine` traits |
//! | [`backend`] | `BackendSpec`: id, kind, required files; import validation |
//! | [`registry`] | Installed models, per-kind selection, capability state |

pub mod backend;
pub mod engine;
pub mod registry;

pub use backend::{BackendSpec, FileRequirement, ModelKind};
pub use engine::{AudioClip, LlmEngine, ModelError, SttEngine, TtsEngine};
pub use registry::{InstalledModel, ModelRegistry};
