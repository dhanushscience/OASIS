//! Which ways of pressing the three buttons are switched on, and the keys for the keyboard way.
//! Kept in `profiles.json` beside the system switches: `InputHardware` (default on, as before),
//! `InputKeyboard` (default off), `InputKeys` (one shortcut per button) and `InputKeysExclusive`
//! (default on: a chosen key is kept from the program in front, so it does not also type).

use serde_json::Value;

use crate::model::Settings;

const HARDWARE: &str = "InputHardware";
const KEYBOARD: &str = "InputKeyboard";
const KEYS: &str = "InputKeys";
const EXCLUSIVE: &str = "InputKeysExclusive";

/// Shortcuts that are offered until the person picks their own: the horizontal set, three keys in a row.
pub const DEFAULT_KEYS: [&str; 3] = ["F1", "F2", "F3"];
/// The vertical set: the three keys down the left edge of the keyboard.
pub const VERTICAL_KEYS: [&str; 3] = ["`", "Tab", "CapsLock"];

/// The keys that go with a vertical or a horizontal pop-up.
pub fn preset_keys(vertical: bool) -> [&'static str; 3] {
    if vertical { VERTICAL_KEYS } else { DEFAULT_KEYS }
}

/// The keys are one of the two sets OASIS offers, not ones the person chose.
fn is_preset(keys: &[String; 3]) -> bool {
    let same = |set: [&str; 3]| keys.iter().zip(set).all(|(k, p)| oasis_core::input::Binding::parse(k) == oasis_core::input::Binding::parse(p));
    same(DEFAULT_KEYS) || same(VERTICAL_KEYS)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSettings {
    /// The OASIS pad over Bluetooth.
    pub hardware: bool,
    /// The three buttons from the keyboard.
    pub keyboard: bool,
    /// The shortcut for each button, as text (`F1`, `Ctrl+Alt+K`). Empty = none.
    pub keys: [String; 3],
    /// Keep a chosen shortcut from the program in front.
    pub exclusive: bool,
}

impl Default for InputSettings {
    fn default() -> InputSettings {
        InputSettings { hardware: true, keyboard: false, keys: DEFAULT_KEYS.map(String::from), exclusive: true }
    }
}

impl From<&Settings> for InputSettings {
    fn from(settings: &Settings) -> InputSettings {
        let flag = |key: &str, default: bool| settings.extra.get(key).and_then(Value::as_bool).unwrap_or(default);
        let stored = settings.extra.get(KEYS).and_then(Value::as_array);
        let mut keys = DEFAULT_KEYS.map(String::from);
        if let Some(list) = stored {
            for (slot, value) in keys.iter_mut().zip(list) {
                *slot = value.as_str().unwrap_or("").trim().to_string();
            }
        }
        InputSettings { hardware: flag(HARDWARE, true), keyboard: flag(KEYBOARD, false), keys, exclusive: flag(EXCLUSIVE, true) }
    }
}

/// Why a set of shortcuts was refused (a sentence for the person).
pub fn check_keys(keys: &[String; 3]) -> Result<(), String> {
    let mut seen = Vec::new();
    for (i, text) in keys.iter().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        let chord = oasis_core::shortcut::Chord::parse(text).map_err(|e| format!("Button {}: {e}", i + 1))?;
        let Some(binding) = oasis_core::input::Binding::from_chord(&chord) else {
            return Err(format!("Button {}: use one key, with Ctrl, Alt, Shift or Win if you like, or one of those on its own.", i + 1));
        };
        if seen.contains(&binding) {
            return Err(format!("Button {}: that is the same shortcut as another button.", i + 1));
        }
        seen.push(binding);
    }
    Ok(())
}

impl Settings {
    /// Save the input choices. Refused (nothing changed) if the shortcuts do not make sense.
    pub fn set_input(&mut self, input: &InputSettings) -> Result<(), String> {
        check_keys(&input.keys)?;
        if !input.hardware && !input.keyboard {
            return Err("Keep at least one way of pressing the buttons switched on.".into());
        }
        self.extra.insert(HARDWARE.into(), Value::Bool(input.hardware));
        self.extra.insert(KEYBOARD.into(), Value::Bool(input.keyboard));
        self.extra.insert(KEYS.into(), Value::Array(input.keys.iter().map(|k| Value::String(k.trim().to_string())).collect()));
        self.extra.insert(EXCLUSIVE.into(), Value::Bool(input.exclusive));
        Ok(())
    }

    /// The answer to "a vertical or a horizontal pop-up?": the overlay goes down the left edge or in a
    /// row at the bottom (back on its edge if it was dragged), the voice pop-up follows, and the keys
    /// become the matching set, unless the person has already chosen keys of their own.
    pub fn choose_layout(&mut self, vertical: bool) {
        let mut input = InputSettings::from(&*self);
        if is_preset(&input.keys) {
            input.keys = preset_keys(vertical).map(String::from);
            let _ = self.set_input(&input); // only refused if every way of pressing is off; the keys then stay
        }
        self.extra.insert("OverlayDock".into(), Value::from(if vertical { "left" } else { "bottom" }));
        self.extra.insert(crate::overlay::POPUP.into(), Value::from(if vertical { "vertical" } else { "horizontal" }));
        self.overlay_x = (-1).into();
        self.overlay_y = (-1).into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_keep_the_pad_on_and_the_keyboard_off() {
        let s = InputSettings::from(&Settings::default());
        assert!(s.hardware && !s.keyboard && s.exclusive);
        assert_eq!(s.keys, DEFAULT_KEYS.map(String::from));
    }

    #[test]
    fn the_defaults_are_f1_to_f3_and_both_sets_are_valid_shortcuts() {
        assert_eq!(DEFAULT_KEYS, ["F1", "F2", "F3"]);
        assert_eq!(preset_keys(false), DEFAULT_KEYS);
        assert_eq!(preset_keys(true), ["`", "Tab", "CapsLock"]);
        for vertical in [false, true] {
            assert_eq!(check_keys(&preset_keys(vertical).map(String::from)), Ok(()));
        }
    }

    #[test]
    fn choosing_a_layout_sets_the_overlay_the_pop_up_and_the_keys() {
        use crate::overlay::{overlay_appearance, overlay_position, popup_vertical};
        let mut s = Settings { overlay_x: 40.into(), overlay_y: 300.into(), ..Settings::default() };
        s.choose_layout(true);
        assert_eq!(InputSettings::from(&s).keys, VERTICAL_KEYS.map(String::from));
        assert_eq!(overlay_appearance(&s).dock, "left");
        assert!(popup_vertical(&s));
        assert_eq!(overlay_position(&s), None, "back on its edge");
        s.choose_layout(false);
        assert_eq!(InputSettings::from(&s).keys, DEFAULT_KEYS.map(String::from));
        assert_eq!(overlay_appearance(&s).dock, "bottom");
        assert!(!popup_vertical(&s));
    }

    #[test]
    fn choosing_a_layout_keeps_keys_the_person_chose() {
        let mut s = Settings::default();
        let mine = InputSettings { keyboard: true, keys: ["Ctrl+Alt+K".into(), "F2".into(), "F3".into()], ..InputSettings::default() };
        s.set_input(&mine).unwrap();
        s.choose_layout(true);
        assert_eq!(InputSettings::from(&s), mine);
        assert!(crate::overlay::popup_vertical(&s), "the look still follows the answer");
    }

    #[test]
    fn choices_are_saved_and_read_back_from_the_file_text() {
        let mut settings = Settings::default();
        let mine = InputSettings {
            hardware: false,
            keyboard: true,
            keys: ["F13".into(), "Ctrl+Alt+K".into(), String::new()],
            exclusive: false,
        };
        settings.set_input(&mine).unwrap();
        let back: Settings = serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(InputSettings::from(&back), mine);
    }

    #[test]
    fn tab_caps_lock_and_a_lone_modifier_are_accepted() {
        let mut settings = Settings::default();
        let input = InputSettings { keyboard: true, keys: ["Tab".into(), "CapsLock".into(), "Shift".into()], ..InputSettings::default() };
        settings.set_input(&input).unwrap();
        let twice = InputSettings { keys: ["Shift".into(), "shift".into(), String::new()], ..input };
        assert!(settings.set_input(&twice).unwrap_err().contains("same shortcut"));
    }

    #[test]
    fn bad_shortcuts_and_all_ways_off_are_refused_and_change_nothing() {
        let mut settings = Settings::default();
        let before = settings.clone();
        let mut input = InputSettings { keyboard: true, ..InputSettings::default() };
        input.keys[0] = "Ctrl+Alt".into(); // no plain key
        assert!(settings.set_input(&input).unwrap_err().contains("Button 1"));
        input.keys[0] = "Ctrl+Q".into();
        input.keys[1] = "ctrl+q".into(); // the same as button 1
        assert!(settings.set_input(&input).unwrap_err().contains("same shortcut"));
        input.keys[1] = "Q+Ctrl".into(); // the same, written the other way round
        assert!(settings.set_input(&input).unwrap_err().contains("same shortcut"));
        let none = InputSettings { hardware: false, keyboard: false, ..InputSettings::default() };
        assert!(settings.set_input(&none).is_err());
        assert_eq!(settings, before);
    }
}
