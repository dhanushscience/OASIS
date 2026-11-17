//! Port of `SettingsMigrationPipeline`, `V0ToV1SettingsMigration` and `V1ToV2SettingsMigration`.
//!
//! Works on raw JSON, like the C# `JsonNode` code, so unknown fields pass through untouched.
//! Credential-store and icon-copy steps stay out of this crate (they need the OS): the Google
//! client secret found in the file is returned for the caller to store, and blanked here.

use oasis_core::matching::profile_targets::infer_kind;
use serde_json::{json, Map, Value};

use crate::schema::{is_retired_stt_model, DEFAULT_LLM_MODEL_FILE_NAME, DEFAULT_STT_MODEL_FOLDER};

pub struct MigrationResult {
    pub root: Value,
    pub changed: bool,
    /// `Meetings.GoogleClientSecret` as found, for the caller to move into the secret store.
    pub extracted_google_client_secret: Option<String>,
}

type Step = (u32, fn(&mut Map<String, Value>));

const STEPS: [Step; 2] = [(0, v0_to_v1), (1, v1_to_v2)];

/// A missing or non-integer `SchemaVersion` is schema 0.
pub fn read_version(root: &Value) -> u32 {
    root.get("SchemaVersion").and_then(Value::as_u64).map_or(0, |v| v as u32)
}

pub fn apply(mut root: Value) -> Result<MigrationResult, String> {
    let extracted = peek_google_client_secret(&root);
    let mut version = read_version(&root);
    let mut changed = false;

    for (from, step) in STEPS {
        if version != from {
            continue;
        }
        let object = root.as_object_mut().ok_or("Settings root must be a JSON object.")?;
        step(object);
        version = from + 1;
        object.insert("SchemaVersion".into(), json!(version));
        changed = true;
    }

    if extracted.is_some() {
        if let Some(meetings) = root.get_mut("Meetings").and_then(Value::as_object_mut) {
            meetings.insert("GoogleClientSecret".into(), json!(""));
            changed = true;
        }
    }
    Ok(MigrationResult { root, changed, extracted_google_client_secret: extracted })
}

fn peek_google_client_secret(root: &Value) -> Option<String> {
    let secret = root.get("Meetings")?.get("GoogleClientSecret")?.as_str()?.trim();
    (!secret.is_empty()).then(|| secret.to_string())
}

// ---- v0 → v1 ---------------------------------------------------------------------------

fn v0_to_v1(root: &mut Map<String, Value>) {
    for name in ["OverlayAppearance", "YouTube", "Voice", "Ai", "Meetings", "Outlook", "Google"] {
        ensure_object(root, name);
    }
    migrate_overlay(object_mut(root, "OverlayAppearance"));
    migrate_ai(object_mut(root, "Ai"));
    migrate_voice(object_mut(root, "Voice"));
    migrate_profiles(root);
    let meetings = object_mut(root, "Meetings");
    if meetings.contains_key("GoogleClientSecret") {
        meetings.insert("GoogleClientSecret".into(), json!(""));
    }
}

fn migrate_overlay(overlay: &mut Map<String, Value>) {
    if is_null(overlay, "LabelCornerRadius") {
        overlay.insert("LabelCornerRadius".into(), json!(22));
    }
    if is_null(overlay, "LabelFontOpacity") || is_null(overlay, "LabelBubbleOpacity") {
        // The old single opacity feeds both new ones. Keep its JSON form (1 stays 1, not 1.0).
        let legacy = overlay.get("LabelOpacity").filter(|v| v.is_number()).cloned();
        let legacy = legacy.unwrap_or_else(|| json!(1));
        if legacy.as_f64().unwrap_or(1.0) > 0.0 {
            for key in ["LabelFontOpacity", "LabelBubbleOpacity"] {
                if is_null(overlay, key) {
                    overlay.insert(key.into(), legacy.clone());
                }
            }
        }
    }
}

fn migrate_ai(ai: &mut Map<String, Value>) {
    let model = read_string(ai, "Model").unwrap_or_default();
    const OLD: [&str; 5] =
        ["gemini-2.0-flash", "gemini-2.0-flash-lite", "gemini-1.5-flash", "gemini-1.5-pro", "gemini-1.5-flash-8b"];
    if model.is_empty() || OLD.contains(&model.as_str()) {
        ai.insert("Model".into(), json!("gemini-2.5-flash"));
    }
}

fn migrate_voice(voice: &mut Map<String, Value>) {
    let llm = read_string(voice, "LlmModelFileName").unwrap_or_default();
    if llm.is_empty() || llm.to_ascii_lowercase().contains("qwen") {
        voice.insert("LlmModelFileName".into(), json!(DEFAULT_LLM_MODEL_FILE_NAME));
        let intent = voice.get("IntentMode").and_then(Value::as_i64).unwrap_or(0);
        if intent == 0 {
            voice.insert("IntentMode".into(), json!(3));
        }
    }
    let stt = read_string(voice, "ModelFileName").unwrap_or_default();
    if stt.is_empty() || is_retired_stt_model(&stt) {
        voice.insert("ModelFileName".into(), json!(DEFAULT_STT_MODEL_FOLDER));
    }
}

fn migrate_profiles(root: &mut Map<String, Value>) {
    let Some(profiles) = root.get_mut("Profiles").and_then(Value::as_array_mut) else { return };
    for profile in profiles.iter_mut().filter_map(Value::as_object_mut) {
        let has_configs =
            profile.get("ConfigProfiles").and_then(Value::as_array).is_some_and(|c| !c.is_empty());
        if !has_configs {
            let name = read_string(profile, "Name").unwrap_or_else(|| "Default".into());
            let mut primary = Map::new();
            primary.insert("Name".into(), json!(name));
            primary.insert("ContextMatch".into(), json!("*"));
            if let Some(buttons) = profile.get("Buttons").and_then(Value::as_array) {
                if !buttons.is_empty() {
                    let mut copy: Vec<Value> = buttons.iter().take(3).cloned().collect();
                    while copy.len() < 3 {
                        copy.push(empty_button());
                    }
                    primary.insert("Buttons".into(), Value::Array(copy));
                }
            }
            profile.shift_remove("Buttons");
            profile.insert("ConfigProfiles".into(), json!([Value::Object(primary)]));
        }

        let configs = profile
            .get_mut("ConfigProfiles")
            .and_then(Value::as_array_mut)
            .expect("ConfigProfiles was just ensured");
        let mut extra_config = configs.len() > 1;
        for config in configs.iter_mut().filter_map(Value::as_object_mut) {
            pad_buttons(config);
            // A config without a match counts as a specific one, as in C#.
            let context_match = read_string(config, "ContextMatch").unwrap_or_default();
            if context_match.trim() != "*" {
                extra_config = true;
            }
        }
        if extra_config {
            profile.insert("MultiConfigEnabled".into(), json!(true));
        }
    }
}

fn pad_buttons(config: &mut Map<String, Value>) {
    match config.get_mut("Buttons").and_then(Value::as_array_mut) {
        Some(buttons) => {
            while buttons.len() < 3 {
                buttons.push(empty_button());
            }
        }
        None => {
            config.insert("Buttons".into(), json!([empty_button(), empty_button(), empty_button()]));
        }
    }
}

fn empty_button() -> Value {
    json!({ "Type": "Shortcut", "Value": "", "Label": "" })
}

// ---- v1 → v2 ---------------------------------------------------------------------------

/// Profiles saved before `Kind` existed get the kind the old guess gave them, once.
fn v1_to_v2(root: &mut Map<String, Value>) {
    let Some(profiles) = root.get_mut("Profiles").and_then(Value::as_array_mut) else { return };
    for profile in profiles.iter_mut().filter_map(Value::as_object_mut) {
        let existing = profile.get("Kind").and_then(Value::as_str).unwrap_or("");
        if !existing.trim().is_empty() {
            continue;
        }
        let context_match = profile.get("ContextMatch").and_then(Value::as_str);
        profile.insert("Kind".into(), json!(infer_kind(context_match)));
    }
}

// ---- helpers ---------------------------------------------------------------------------

fn ensure_object(root: &mut Map<String, Value>, name: &str) {
    if !root.get(name).is_some_and(Value::is_object) {
        root.insert(name.into(), Value::Object(Map::new()));
    }
}

fn object_mut<'a>(root: &'a mut Map<String, Value>, name: &str) -> &'a mut Map<String, Value> {
    root.get_mut(name).and_then(Value::as_object_mut).expect("section was just ensured")
}

fn is_null(object: &Map<String, Value>, key: &str) -> bool {
    object.get(key).is_none_or(Value::is_null)
}

/// C# `ReadString`: a string as is, a missing or null value as `None`, anything else as its
/// JSON text.
fn read_string(object: &Map<String, Value>, key: &str) -> Option<String> {
    match object.get(key)? {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}
