//! The profiles screen: the profiles, their sets of buttons and the buttons, as the page reads and edits them;
//! keys and sounds the button editor needs; press counts.

use super::*;

/// The profiles and their buttons, for the settings screen.
#[tauri::command]
pub fn settings_view(slot: State<'_, EngineSlot>) -> Result<SettingsView, String> {
    with_engine(&slot, |engine| Ok(settings_view::view(&engine.settings())))
}

/// A step as the page sends it.
#[derive(Deserialize)]
pub(crate) struct StepInput {
    kind: String,
    value: String,
    /// null: the plain-list rule; -1: stop; otherwise the step to go to.
    #[serde(default)]
    next: Option<i64>,
    #[serde(default)]
    on_fail: Option<i64>,
    #[serde(default)]
    x: Option<i32>,
    #[serde(default)]
    y: Option<i32>,
}

/// What the page sends to change a button. `steps` is only used for the "several steps" type.
#[derive(Deserialize)]
pub(crate) struct ButtonInput {
    kind: String,
    value: String,
    label: String,
    steps: Vec<StepInput>,
}

/// The window says what its keyboard focus is (0 none, 1 typing in a text box, 2 recording a shortcut),
/// so typing there is never taken for a keyboard button.
#[tauri::command]
pub fn set_typing(mode: u8) {
    oasis_platform::ports::set_window_typing(mode);
}

/// A keyboard button pressed or let go in the OASIS window, which the system-wide key listener is not
/// given while the window is in front. It runs like any other keyboard press.
#[tauri::command]
pub fn press_button(button: u8, down: bool) {
    oasis_platform::ports::press_from_window(button, down);
}

/// Caps Lock chosen as a button and pressed in the OASIS window: Windows has already switched the lock
/// (the window cannot stop that, as the system-wide listener can elsewhere), so switch it back. The key is
/// still held down, and a press of a key that is down is only a repeat, so it is let go first.
#[tauri::command]
pub fn restore_caps_lock() {
    #[cfg(windows)]
    {
        use oasis_win::input::{send_key, KeyEvent};
        for release in [true, false, true] {
            let _ = send_key(KeyEvent {
                vk: 0x14,
                release,
                scancode_only: false,
                extended: false,
            });
        }
    }
}

/// The sounds a step can play, for the picker: id, name and whether it is a "worked" sound.
#[tauri::command]
pub fn sound_list() -> Vec<(&'static str, &'static str, bool)> {
    oasis_core::actions::sounds::SOUNDS.iter().map(|s| (s.id, s.name, s.success)).collect()
}

/// Plays a sound once, so it can be heard before it is chosen.
#[tauri::command]
pub fn play_sound(id: String) {
    #[cfg(windows)]
    if let Some(wav) = oasis_core::actions::sounds::wav_cached(&id) {
        oasis_win::system::play_wav(wav);
    }
    #[cfg(not(windows))]
    let _ = id;
}

/// "Try it": run one saved button's action now.
#[tauri::command]
pub fn try_button(slot: State<'_, EngineSlot>, profile: usize, config: usize, button: usize) -> Result<(), String> {
    with_engine(&slot, |engine| engine.try_button(profile, config, button))
}

/// Change one button and save. The error text is meant for the person editing.
#[tauri::command]
pub fn save_button(
    slot: State<'_, EngineSlot>,
    profile: usize,
    config: usize,
    button: usize,
    edit: ButtonInput,
) -> Result<(), String> {
    with_engine(&slot, |engine| {
        let edit = ButtonEdit {
            kind: edit.kind,
            value: edit.value,
            label: edit.label,
            steps: edit
                .steps
                .into_iter()
                .map(|s| ActionStep {
                    kind: s.kind,
                    value: s.value,
                    on_ok: Next::from_code(s.next),
                    on_fail: Next::from_code(s.on_fail),
                    pos: s.x.zip(s.y),
                })
                .collect(),
        };
        engine
            .change(|s| s.set_button(profile, config, button, edit))
            .map_err(|e| e.to_string())
    })
}

/// Rename a profile or change what it matches, and save.
#[tauri::command]
pub fn save_profile(
    slot: State<'_, EngineSlot>,
    profile: usize,
    name: String,
    context_match: String,
    kind: String,
) -> Result<(), String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| s.update_profile(profile, &name, &context_match, &kind))
            .map_err(|e| e.to_string())
    })
}

/// Which ways of pressing the three buttons are on, and the keys for the keyboard way. Refused (nothing
/// changed) if the keys make no sense or every way would be off.
#[tauri::command]
pub fn save_input(
    slot: State<'_, EngineSlot>,
    hardware: bool,
    keyboard: bool,
    keys: Vec<String>,
    exclusive: bool,
) -> Result<(), String> {
    let keys: [String; 3] = keys
        .try_into()
        .map_err(|_| "Three shortcuts are needed, one per button.".to_string())?;
    let input = oasis_settings::input::InputSettings {
        hardware,
        keyboard,
        keys,
        exclusive,
    };
    with_engine(&slot, |engine| {
        engine
            .change(|s| {
                s.set_input(&input)
                    .map_err(oasis_settings::edit::EditError::Invalid)
            })
            .map_err(|e| e.to_string())
    })
}

/// How a profile shows its labels, and its emoji icon, and save.
#[tauri::command]
pub fn save_profile_look(
    slot: State<'_, EngineSlot>,
    profile: usize,
    label_mode: String,
    icon_emoji: String,
) -> Result<(), String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| s.set_profile_look(profile, &label_mode, &icon_emoji))
            .map_err(|e| e.to_string())
    })
}

/// What the page sends to create or change a set of buttons.
#[derive(Deserialize)]
pub(crate) struct SetInput {
    name: String,
    context_match: String,
    when: String,
    panel_id: String,
    label_mode: String,
}

impl From<SetInput> for SetEdit {
    fn from(input: SetInput) -> SetEdit {
        SetEdit {
            name: input.name,
            context_match: input.context_match,
            when: input.when,
            panel_id: input.panel_id,
            label_mode: input.label_mode,
        }
    }
}

/// Turn a profile's extra sets of buttons on or off (they are kept either way).
#[tauri::command]
pub fn set_multi_config(
    slot: State<'_, EngineSlot>,
    profile: usize,
    enabled: bool,
) -> Result<(), String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| s.set_multi_config(profile, enabled))
            .map_err(|e| e.to_string())
    })
}

/// Add a set of buttons to a profile and return its index.
#[tauri::command]
pub fn add_config(
    slot: State<'_, EngineSlot>,
    profile: usize,
    edit: SetInput,
) -> Result<usize, String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| s.add_config(profile, edit.into()))
            .map_err(|e| e.to_string())
    })
}

/// Change a set's name, condition and panel.
#[tauri::command]
pub fn update_config(
    slot: State<'_, EngineSlot>,
    profile: usize,
    config: usize,
    edit: SetInput,
) -> Result<(), String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| s.update_config(profile, config, edit.into()))
            .map_err(|e| e.to_string())
    })
}

/// Delete a set of buttons (never the main one, never the last).
#[tauri::command]
pub fn delete_config(slot: State<'_, EngineSlot>, profile: usize, config: usize) -> Result<(), String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| s.delete_config(profile, config))
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Add a profile and return its index. `target` is what it applies to (blank = everything).
#[tauri::command]
pub fn add_profile(
    slot: State<'_, EngineSlot>,
    name: String,
    target: String,
    kind: String,
) -> Result<usize, String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| s.add_profile(&name, &target, &kind))
            .map_err(|e| e.to_string())
    })
}

/// Delete a profile (at least one must remain). Its icon file goes too, unless another profile
/// still uses it.
#[tauri::command]
pub fn delete_profile(slot: State<'_, EngineSlot>, profile: usize) -> Result<(), String> {
    with_engine(&slot, |engine| {
        let removed = engine
            .change(|s| s.delete_profile(profile))
            .map_err(|e| e.to_string())?;
        let still_used = engine
            .settings()
            .profiles
            .iter()
            .any(|p| p.icon_path == removed.icon_path);
        if !still_used {
            oasis_platform::profile_icons::delete_managed(
                &engine.paths().app_data_root,
                &removed.icon_path,
            );
        }
        Ok(())
    })
}

/// Move a profile to a new position in the list.
#[tauri::command]
pub fn move_profile(slot: State<'_, EngineSlot>, from: usize, to: usize) -> Result<(), String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| s.move_profile(from, to))
            .map_err(|e| e.to_string())
    })
}

/// Presses per profile name, for ordering profiles by use.
#[tauri::command]
pub fn profile_use_counts(slot: State<'_, EngineSlot>) -> std::collections::BTreeMap<String, u64> {
    slot.handle
        .lock()
        .ok()
        .and_then(|h| h.as_ref().map(EngineHandle::profile_use_counts))
        .unwrap_or_default()
}
