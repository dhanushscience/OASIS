//! Typed view of `profiles.json`. Field names are the C# PascalCase names. Anything not listed
//! is held in `extra` and serialised back as it was.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value};

/// Extra fields we do not model yet.
type Extra = Map<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Settings {
    /// A file without this field is schema 0 (not the default of a brand-new settings object).
    #[serde(default = "legacy_schema")]
    pub schema_version: u32,
    /// Screen position. C# stores a double; kept as the JSON number so 0 stays 0. `-1` = unset.
    pub overlay_x: Number,
    pub overlay_y: Number,
    pub show_overlay_labels: bool,
    pub allow_run_script_actions: bool,
    pub profiles: Vec<Profile>,
    /// `OverlayAppearance`, `Voice`, `Ai`, `QuickLaunch`, … until each is ported.
    #[serde(flatten)]
    pub extra: Extra,
}

fn legacy_schema() -> u32 {
    0
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            schema_version: crate::CURRENT_SCHEMA,
            overlay_x: Number::from(-1),
            overlay_y: Number::from(-1),
            show_overlay_labels: true,
            allow_run_script_actions: false,
            profiles: Vec::new(),
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Profile {
    pub name: String,
    pub context_match: String,
    /// `""`, `"app"` or `"website"`.
    pub kind: String,
    pub label_mode: String,
    pub multi_config_enabled: bool,
    pub icon_emoji: String,
    pub icon_path: String,
    pub config_profiles: Vec<ConfigProfile>,
    /// Pre-v2 layout: buttons directly on the profile. `null` once migrated.
    pub buttons: Option<Vec<Button>>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct ConfigProfile {
    pub name: String,
    pub context_match: String,
    pub panel_id: String,
    pub panel_label: String,
    pub focus_class_name: String,
    pub ancestor_hint: String,
    pub when: String,
    /// How this set shows the overlay: `""` follows the profile, `show` or `hide`.
    pub label_mode: String,
    pub buttons: Vec<Button>,
    #[serde(flatten)]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Button {
    /// `LaunchApp`, `OpenUrl`, `Shortcut`, … kept as text so an unknown type survives.
    #[serde(rename = "Type")]
    pub kind: String,
    pub value: String,
    pub label: String,
    /// `""` for a single action, or an array for a multi-step one. Not interpreted here yet.
    pub steps: Value,
    #[serde(flatten)]
    pub extra: Extra,
}
