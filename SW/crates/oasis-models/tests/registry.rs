use oasis_models::{BackendSpec, FileRequirement, ModelError, ModelKind, ModelRegistry};

struct Sherpa;
impl BackendSpec for Sherpa {
    fn id(&self) -> &'static str {
        "sherpa-onnx"
    }
    fn kind(&self) -> ModelKind {
        ModelKind::Stt
    }
    fn required_files(&self) -> &[FileRequirement] {
        &[FileRequirement::Extension("onnx"), FileRequirement::Named("tokens.txt")]
    }
}

fn registry() -> ModelRegistry {
    let mut r = ModelRegistry::new();
    r.register_backend(Box::new(Sherpa));
    r
}

#[test]
fn import_with_all_files_succeeds_and_selects_first() {
    let mut r = registry();
    r.import("small", "sherpa-onnx", &["model.ONNX", "tokens.txt", "extra.md"]).unwrap();
    assert_eq!(r.selected(ModelKind::Stt).unwrap().id, "small");
    assert!(r.capability_state(ModelKind::Stt).1.is_available());
}

#[test]
fn import_names_every_missing_file() {
    let mut r = registry();
    let err = r.import("bad", "sherpa-onnx", &["readme.md"]).unwrap_err();
    assert_eq!(
        err,
        ModelError::MissingFiles(vec!["a .onnx file".into(), "tokens.txt".into()])
    );
    assert!(r.selected(ModelKind::Stt).is_none());
}

#[test]
fn unknown_backend_is_refused() {
    let mut r = registry();
    assert_eq!(
        r.import("x", "nope", &[]).unwrap_err(),
        ModelError::UnknownBackend("nope".into())
    );
}

#[test]
fn second_import_does_not_change_selection_until_selected() {
    let mut r = registry();
    r.import("a", "sherpa-onnx", &["a.onnx", "tokens.txt"]).unwrap();
    r.import("b", "sherpa-onnx", &["b.onnx", "tokens.txt"]).unwrap();
    assert_eq!(r.selected(ModelKind::Stt).unwrap().id, "a");
    assert!(r.select(ModelKind::Stt, "b"));
    assert!(!r.select(ModelKind::Stt, "missing"));
    assert_eq!(r.selected(ModelKind::Stt).unwrap().id, "b");
}

#[test]
fn nothing_installed_reads_as_missing_with_a_reason() {
    let r = registry();
    let (_, state) = r.capability_state(ModelKind::Llm);
    assert!(!state.is_available());
}
