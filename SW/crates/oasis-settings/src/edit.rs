//! Changing the settings from a settings window, safely.
//!
//! Every edit is checked with the same rules the engine uses to run an action, so a button
//! cannot be saved in a form that would silently fail at the first press: a shortcut that does
//! not parse, a link the allowlist refuses, a mouse position that is not `x,y`. Fields this crate
//! does not model are kept: an edit changes only what it names.
//!
//! This is new in Rust: the C# app validated inside its WPF editors.

use std::fmt;

use oasis_core::actions::{try_allow, ActionKind, ActionStep, MouseClick, Next, UrlScope};
use oasis_core::display_label::{fit as fit_label, DEFAULT_MAX_LENGTH};
use oasis_core::matching::profile_targets::{infer_kind, website_match, APP_KIND, WEBSITE_KIND};
use oasis_core::shortcut::Chord;
use serde_json::{json, Value};

use crate::model::{Button, ConfigProfile, Profile, Settings};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    NoSuchProfile(usize),
    NoSuchConfig { profile: usize, config: usize },
    NoSuchButton { profile: usize, config: usize, button: usize },
    /// The change is not allowed; the text says why, for the person editing.
    Invalid(String),
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::NoSuchProfile(i) => write!(f, "There is no profile number {}.", i + 1),
            EditError::NoSuchConfig { profile, config } => {
                write!(f, "Profile {} has no configuration number {}.", profile + 1, config + 1)
            }
            EditError::NoSuchButton { button, .. } => write!(f, "There is no button {}.", button + 1),
            EditError::Invalid(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for EditError {}

fn invalid<T>(why: impl Into<String>) -> Result<T, EditError> {
    Err(EditError::Invalid(why.into()))
}

/// How many sets of buttons one profile can have (the C# window has six slots).
pub const MAX_SETS_PER_PROFILE: usize = 6;
/// Longest names and conditions, in characters. The window stops typing at these; this refuses the rest.
pub const MAX_PROFILE_NAME: usize = 40;
pub const MAX_SET_NAME: usize = 24;
pub const MAX_MATCH: usize = 200;

/// A name or condition no longer than `max` characters, or the reason it is refused.
fn check_length(text: &str, max: usize, what: &str) -> Result<(), EditError> {
    if text.chars().count() > max {
        return invalid(&format!("{what} can be at most {max} characters."));
    }
    Ok(())
}

// One file per thing being edited; `use super::*` gives each the imports and helpers above.
mod button;
mod profile;
mod set;

pub use button::{validate_action, ButtonEdit, ACTION_TYPES, STEP_TYPES, WRITABLE_STEP_TYPES};
pub use set::{SetEdit, PANEL_PRESETS, WHEN_OPTIONS};

/// The settings, button edit and step the tests of every edit file start from.
#[cfg(test)]
mod test_settings {
    use super::*;
    use crate::model::{ConfigProfile, Profile};

    pub(super) fn settings() -> Settings {
        let buttons = (0..3).map(|i| Button { kind: "Shortcut".into(), value: format!("F{}", i + 1), label: format!("B{i}"), steps: json!(""), ..Default::default() }).collect();
        Settings {
            profiles: vec![
                Profile {
                    name: "Default".into(),
                    context_match: "*".into(),
                    config_profiles: vec![ConfigProfile { name: "Default".into(), context_match: "*".into(), buttons, ..Default::default() }],
                    ..Default::default()
                },
                Profile {
                    name: "Chrome".into(),
                    context_match: "chrome".into(),
                    kind: "app".into(),
                    config_profiles: vec![ConfigProfile { name: "Chrome".into(), context_match: "*".into(), buttons: vec![Button::default(); 3], ..Default::default() }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    pub(super) fn edit(kind: &str, value: &str) -> ButtonEdit {
        ButtonEdit { kind: kind.into(), value: value.into(), label: " Go ".into(), steps: vec![] }
    }

    pub(super) fn step(kind: &str, value: &str) -> ActionStep {
        ActionStep { kind: kind.into(), value: value.into(), ..Default::default() }
    }
}
