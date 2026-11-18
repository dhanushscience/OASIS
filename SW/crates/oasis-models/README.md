# oasis-models

## Overview
Replaceable STT, LLM and TTS (DECISIONS D8). One engine trait per capability (`SttEngine`, `LlmEngine`, `TtsEngine`), a `BackendSpec` that says which files a backend needs, and a `ModelRegistry` that tracks installed models and the selected one per kind. We ship the defaults that work for us (D9: sherpa-onnx, llama.cpp, Piper); defaults and user imports go through the same path.

Used at import time (validate the files) and at startup (resolve the selection and report its capability state).

Where it stands: only `SttEngine`, `AudioClip` and `ModelError` are used by other crates (`oasis-engine`'s `GroqStt` implements `SttEngine`, as does `oasis-voice`'s `SherpaTransducer`). `ModelRegistry` and `BackendSpec` are tested here but nothing outside this crate uses them yet. `LlmEngine` and `TtsEngine` have no implementors. Selection is not stored in settings and the file copy is not in `oasis-platform`.

## Interface
- `src/engine.rs`: `SttEngine`, `LlmEngine`, `TtsEngine` traits; `AudioClip { samples: Vec<f32>, sample_rate: u32 }`; `ModelError` (`MissingFiles`, `UnknownBackend`, `Failed`).
- `src/backend.rs`: `ModelKind` (`Stt`, `Llm`, `Tts`, `ModelKind::ALL`); `FileRequirement` (`Named`, `Extension`); trait `BackendSpec` (`id`, `kind`, `required_files`, `validate_import`).
- `src/registry.rs`: `ModelRegistry` (`register_backend`, `backend`, `import`, `models`, `select`, `selected`, `capability_state`); `InstalledModel { id, kind, backend }`.
- `src/lib.rs`: module declarations and re-exports of the above.

## Invariants
- Import is local files only. `ModelRegistry::import` checks file names against the backend's required files and returns `MissingFiles` naming every absent one; file name matching is ASCII case-insensitive.
- An unregistered backend id yields `ModelError::UnknownBackend`.
- Importing an id that already exists for that kind replaces it; the first model of a kind becomes the selection.
- `select` returns false when no such model of that kind is installed.
- No silent fallback: no selected model means `CapabilityState::Missing` with a reason (`capability_state`).
- Engines are blocking. The caller picks the thread: the engine runs speech-to-text on its `oasis-speech` thread, and polish and dictation cleanup on an action worker.

## Side Effects
- None. The crate does no I/O: after `import` succeeds, `oasis-platform` copies the files into the models folder. Only mutation is the in-memory registry.

## Verification
- Run: `cargo test -p oasis-models` (tests in `tests/registry.rs`: import, selection, capability)
