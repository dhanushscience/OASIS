//! Ports of `SettingsMigrationTests` (C#), run against the same corpus files, plus round-trip
//! and typed-load checks.

use oasis_settings::migrate::{apply, read_version};
use oasis_settings::schema::{DEFAULT_LLM_MODEL_FILE_NAME, DEFAULT_STT_MODEL_FOLDER};
use oasis_settings::{load, to_json, SettingsError, CURRENT_SCHEMA};
use serde_json::{json, Value};

const V0: &str = include_str!("corpus/v0.json");
const V1: &str = include_str!("corpus/v1.json");
const HAND_EDITED: &str = include_str!("corpus/hand-edited.json");
const CORRUPT: &str = include_str!("corpus/corrupt.json");

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[test]
fn v0_file_migrates_without_dropping_profiles() {
    let result = apply(parse(V0)).unwrap();
    let root = &result.root;

    assert!(result.changed);
    assert_eq!(result.extracted_google_client_secret.as_deref(), Some("should-not-survive"));
    assert_eq!(read_version(root), CURRENT_SCHEMA);
    assert_eq!(root["Meetings"]["GoogleClientSecret"], "");

    assert_eq!(root["Ai"]["Model"], "gemini-2.5-flash");
    assert_eq!(root["Voice"]["ModelFileName"], DEFAULT_STT_MODEL_FOLDER);
    assert_eq!(root["Voice"]["LlmModelFileName"], DEFAULT_LLM_MODEL_FILE_NAME);
    assert_eq!(root["Voice"]["IntentMode"], 3);

    let profile = &root["Profiles"][0];
    assert!(profile.get("Buttons").is_none());
    let buttons = profile["ConfigProfiles"][0]["Buttons"].as_array().unwrap();
    assert_eq!(buttons.len(), 3);
    assert_eq!(buttons[0]["Label"], "Copy");
    assert_eq!(buttons[1]["Label"], "Paste");
    assert_eq!(buttons[2], json!({ "Type": "Shortcut", "Value": "", "Label": "" }));

    assert_eq!(root["OverlayAppearance"]["LabelSpacing"], 8);
    assert_eq!(root["OverlayAppearance"]["IndicatorSpacing"], 14);
    assert_eq!(root["OverlayAppearance"]["LabelFontOpacity"], 0.5);
    assert_eq!(root["OverlayAppearance"]["LabelBubbleOpacity"], 0.5);
    assert_eq!(root["OverlayAppearance"]["LabelCornerRadius"], 22);
}

#[test]
fn v0_creates_the_sections_it_lacks() {
    let root = apply(json!({})).unwrap().root;
    for section in ["OverlayAppearance", "YouTube", "Voice", "Ai", "Meetings", "Outlook", "Google"] {
        assert!(root[section].is_object(), "{section}");
    }
}

#[test]
fn v1_file_moves_to_v2_and_keeps_its_buttons() {
    let result = apply(parse(V1)).unwrap();
    assert!(result.changed);
    assert_eq!(read_version(&result.root), 2);
    assert_eq!(result.root["Profiles"][0]["ConfigProfiles"][0]["Buttons"][2]["Label"], "Desktop");
}

#[test]
fn v2_file_is_stable() {
    let once = apply(parse(V1)).unwrap().root;
    let again = apply(once.clone()).unwrap();
    assert!(!again.changed);
    assert_eq!(again.root, once);
}

#[test]
fn v1_profiles_get_the_kind_they_were_shown_as_and_an_app_named_like_a_site_stays_an_app() {
    let root = apply(json!({
        "SchemaVersion": 1,
        "Profiles": [
            { "Name": "Default", "ContextMatch": "*" },
            { "Name": "Claude", "ContextMatch": "Claude" },
            { "Name": "NETFLIX", "ContextMatch": "netflix.com" },
            { "Name": "Youtube", "ContextMatch": "youtube" },
            { "Name": "VS CODE", "ContextMatch": "Code" },
            { "Name": "Picked", "ContextMatch": "x", "Kind": "website" }
        ]
    }))
    .unwrap()
    .root;

    let kinds: Vec<&str> =
        root["Profiles"].as_array().unwrap().iter().map(|p| p["Kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["", "app", "website", "website", "app", "website"]);
}

#[test]
fn hand_edited_v1_keeps_custom_labels() {
    let root = apply(parse(HAND_EDITED)).unwrap().root;
    assert_eq!(read_version(&root), CURRENT_SCHEMA);
    assert_eq!(root["Profiles"][0]["Name"], "Hand edited");
    assert_eq!(root["Profiles"][0]["ConfigProfiles"][0]["Buttons"][2]["Label"], "Undo");
}

#[test]
fn corrupt_file_is_a_parse_error_not_a_panic() {
    assert!(matches!(load(CORRUPT), Err(SettingsError::Parse(_))));
}

// ---- behaviour the C# tests do not pin, read from the C# source ------------------------

#[test]
fn profile_with_extra_configs_turns_multi_config_on() {
    let root = apply(json!({
        "Profiles": [
            { "Name": "One", "ConfigProfiles": [ { "ContextMatch": "*" } ] },
            { "Name": "Two", "ConfigProfiles": [ { "ContextMatch": "*" }, { "ContextMatch": "*" } ] },
            { "Name": "Specific", "ConfigProfiles": [ { "ContextMatch": "chrome" } ] }
        ]
    }))
    .unwrap()
    .root;
    let multi: Vec<bool> = root["Profiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.get("MultiConfigEnabled").is_some())
        .collect();
    assert_eq!(multi, [false, true, true]);
    // Every config is padded to three buttons.
    assert_eq!(root["Profiles"][0]["ConfigProfiles"][0]["Buttons"].as_array().unwrap().len(), 3);
}

#[test]
fn legacy_profile_without_buttons_still_gets_a_config_of_three_empty_buttons() {
    let root = apply(json!({ "Profiles": [ { "Name": "Bare" } ] })).unwrap().root;
    let config = &root["Profiles"][0]["ConfigProfiles"][0];
    assert_eq!(config["Name"], "Bare");
    assert_eq!(config["ContextMatch"], "*");
    assert_eq!(config["Buttons"].as_array().unwrap().len(), 3);
}

#[test]
fn legacy_buttons_beyond_three_are_dropped_as_in_csharp() {
    let buttons: Vec<Value> =
        (0..5).map(|i| json!({ "Type": "Shortcut", "Value": "", "Label": format!("B{i}") })).collect();
    let root = apply(json!({ "Profiles": [ { "Name": "P", "Buttons": buttons } ] })).unwrap().root;
    let kept = root["Profiles"][0]["ConfigProfiles"][0]["Buttons"].as_array().unwrap();
    assert_eq!(kept.len(), 3);
    assert_eq!(kept[2]["Label"], "B2");
}

#[test]
fn old_gemini_models_and_qwen_are_replaced_but_a_chosen_model_is_kept() {
    let root = apply(json!({
        "Ai": { "Model": "gemini-3-pro" },
        "Voice": { "LlmModelFileName": "my-own.gguf", "IntentMode": 1, "ModelFileName": "parakeet-x" }
    }))
    .unwrap()
    .root;
    assert_eq!(root["Ai"]["Model"], "gemini-3-pro");
    assert_eq!(root["Voice"]["LlmModelFileName"], "my-own.gguf");
    assert_eq!(root["Voice"]["IntentMode"], 1);
    assert_eq!(root["Voice"]["ModelFileName"], "parakeet-x");
}

#[test]
fn a_blank_google_secret_is_not_reported_as_extracted() {
    let result = apply(json!({ "Meetings": { "GoogleClientSecret": "  " } })).unwrap();
    assert_eq!(result.extracted_google_client_secret, None);
}

// ---- typed load ------------------------------------------------------------------------

#[test]
fn load_migrates_and_reads_typed_fields() {
    let loaded = load(V0).unwrap();
    assert_eq!(loaded.migrated_from, Some(0));
    assert_eq!(loaded.extracted_google_client_secret.as_deref(), Some("should-not-survive"));
    let s = loaded.settings;
    assert_eq!(s.schema_version, CURRENT_SCHEMA);
    assert_eq!(s.overlay_x, 10.into());
    assert_eq!(s.profiles[0].config_profiles[0].buttons[0].label, "Copy");
}

#[test]
fn load_then_save_keeps_unmodelled_sections() {
    let out = parse(&to_json(&load(V1).unwrap().settings));
    assert_eq!(out["Voice"]["IntentMode"], 3);
    assert_eq!(out["OverlayAppearance"]["LabelCornerRadius"], 22);
    // Inside a section, keys keep the order they had in the file.
    let keys: Vec<&String> = out["OverlayAppearance"].as_object().unwrap().keys().collect();
    assert_eq!(keys.first().map(|k| k.as_str()), Some("IndicatorSpacing"));
}

#[test]
fn a_buttons_steps_survive_as_string_or_array() {
    let text = r#"{"SchemaVersion":2,"Profiles":[{"Name":"P","ConfigProfiles":[{"Buttons":[
        {"Type":"A","Value":"","Label":"","Steps":""},
        {"Type":"B","Value":"","Label":"","Steps":[{"x":1}]}]}]}]}"#;
    let out = parse(&to_json(&load(text).unwrap().settings));
    let buttons = &out["Profiles"][0]["ConfigProfiles"][0]["Buttons"];
    assert_eq!(buttons[0]["Steps"], "");
    assert_eq!(buttons[1]["Steps"], json!([{ "x": 1 }]));
}

#[test]
fn newer_schema_is_refused() {
    assert!(matches!(load(r#"{"SchemaVersion": 99}"#), Err(SettingsError::TooNew { found: 99 })));
}

#[test]
fn a_root_that_is_not_an_object_is_a_parse_error() {
    assert!(matches!(load("[1, 2]"), Err(SettingsError::Parse(_))));
}

/// Round-trip the owner's real file when `OASIS_REAL_PROFILES` points at it.
#[test]
fn real_file_round_trips_when_available() {
    let Ok(path) = std::env::var("OASIS_REAL_PROFILES") else { return };
    let text = std::fs::read_to_string(path).unwrap();
    let loaded = load(&text).unwrap();
    assert_eq!(parse(&to_json(&loaded.settings)), parse(&text));
}
