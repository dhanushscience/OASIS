//! Editing by voice: a short phrase said on its own presses its keys ("copy" is Ctrl+C, "select
//! line" is Home then Shift+End). Only the whole phrase counts: "copy the file" is not in the table
//! and stays text. The keys are ordinary `Shortcut` steps, so the safety tiers apply unchanged.
//! The Voice setting `EditCommands` (on when absent) switches the table off.

use crate::actions::{ActionKind, ActionStep, ButtonAction};

/// Each phrase as [`super::text::normalize`] leaves it, and the keys it presses in order. "ctrl x"
/// is looked up as "control x".
const TABLE: &[(&str, &[&str])] = &[
    // Edit
    ("select all", &["Ctrl+A"]),
    ("select everything", &["Ctrl+A"]),
    ("copy", &["Ctrl+C"]),
    ("copy that", &["Ctrl+C"]),
    ("paste", &["Ctrl+V"]),
    ("paste it", &["Ctrl+V"]),
    ("paste that", &["Ctrl+V"]),
    ("cut", &["Ctrl+X"]),
    ("cut that", &["Ctrl+X"]),
    ("undo", &["Ctrl+Z"]),
    ("undo that", &["Ctrl+Z"]),
    ("redo", &["Ctrl+Y"]),
    ("paste as plain text", &["Ctrl+Shift+V"]),
    ("save", &["Ctrl+S"]),
    ("save this", &["Ctrl+S"]),
    ("save that", &["Ctrl+S"]),
    ("save it", &["Ctrl+S"]),
    ("find", &["Ctrl+F"]),
    ("bold", &["Ctrl+B"]),
    ("italic", &["Ctrl+I"]),
    ("underline", &["Ctrl+U"]),
    // Spoken chords, only these safe ones ("ctrl t" alone stays a non-command)
    ("control a", &["Ctrl+A"]),
    ("control c", &["Ctrl+C"]),
    ("control see", &["Ctrl+C"]),
    ("control v", &["Ctrl+V"]),
    ("control x", &["Ctrl+X"]),
    ("control z", &["Ctrl+Z"]),
    ("control zed", &["Ctrl+Z"]),
    ("control zee", &["Ctrl+Z"]),
    ("control y", &["Ctrl+Y"]),
    ("control s", &["Ctrl+S"]),
    ("control f", &["Ctrl+F"]),
    // Keys
    ("press enter", &["Enter"]),
    ("enter", &["Enter"]),
    ("press tab", &["Tab"]),
    ("press escape", &["Esc"]),
    ("escape", &["Esc"]),
    ("press backspace", &["Backspace"]),
    ("press space", &["Space"]),
    // Move
    ("go to start of line", &["Home"]),
    ("go to the start of the line", &["Home"]),
    ("go to beginning of line", &["Home"]),
    ("go to the beginning of the line", &["Home"]),
    ("go to end of line", &["End"]),
    ("go to the end of the line", &["End"]),
    ("go to start of document", &["Ctrl+Home"]),
    ("go to the start of the document", &["Ctrl+Home"]),
    ("go to beginning of document", &["Ctrl+Home"]),
    ("go to the beginning of the document", &["Ctrl+Home"]),
    ("go to end of document", &["Ctrl+End"]),
    ("go to the end of the document", &["Ctrl+End"]),
    ("page up", &["PageUp"]),
    ("page down", &["PageDown"]),
    ("scroll up", &["PageUp"]),
    ("scroll down", &["PageDown"]),
    // Select. A word: to the next word's start, then back with Shift, so the cursor anywhere in the
    // word (or at its start) selects that word and the space after it, as Windows does.
    ("select line", &["Home", "Shift+End"]),
    ("select the line", &["Home", "Shift+End"]),
    ("select this line", &["Home", "Shift+End"]),
    ("select word", &["Ctrl+Right", "Ctrl+Shift+Left"]),
    ("select the word", &["Ctrl+Right", "Ctrl+Shift+Left"]),
    ("select this word", &["Ctrl+Right", "Ctrl+Shift+Left"]),
    ("select to end of line", &["Shift+End"]),
    ("select to the end of the line", &["Shift+End"]),
    ("select to start of line", &["Shift+Home"]),
    ("select to the start of the line", &["Shift+Home"]),
    ("select to beginning of line", &["Shift+Home"]),
    ("select to the beginning of the line", &["Shift+Home"]),
    // Browser
    ("refresh", &["F5"]),
    ("refresh page", &["F5"]),
    ("refresh the page", &["F5"]),
    ("reload", &["F5"]),
    ("reload page", &["F5"]),
    ("reload the page", &["F5"]),
    ("go back", &["Alt+Left"]),
    ("go forward", &["Alt+Right"]),
    ("next tab", &["Ctrl+Tab"]),
    ("previous tab", &["Ctrl+Shift+Tab"]),
    // The key marked "+" is "=" without Shift; Ctrl with it zooms in every browser.
    ("zoom in", &["Ctrl+="]),
    ("zoom out", &["Ctrl+-"]),
];

/// The keys a phrase said on its own presses, or `None`. `normalized` is as
/// [`super::text::normalize`] leaves it.
pub fn keys(normalized: &str) -> Option<&'static [&'static str]> {
    let said = normalized.strip_prefix("ctrl ").map_or_else(|| normalized.to_string(), |rest| format!("control {rest}"));
    TABLE.iter().find(|(phrase, _)| *phrase == said).map(|(_, keys)| *keys)
}

/// The action for a phrase in the table, and its label: one `Shortcut`, or an automation of
/// `Shortcut` steps when it takes more than one press.
pub fn lookup(normalized: &str) -> Option<(ButtonAction, String)> {
    let keys = keys(normalized)?;
    let action = match keys {
        [one] => ButtonAction { kind: ActionKind::Shortcut.as_str().into(), value: (*one).into(), steps: vec![] },
        _ => ButtonAction {
            kind: ActionKind::Automation.as_str().into(),
            value: String::new(),
            steps: keys.iter().map(|k| ActionStep { kind: ActionKind::Shortcut.as_str().into(), value: (*k).into(), ..Default::default() }).collect(),
        },
    };
    Some((action, keys.join(", ")))
}
