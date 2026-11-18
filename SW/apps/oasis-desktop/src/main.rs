//! The OASIS desktop app: a tray icon and a status window around `oasis-engine`.
//!
//! The engine runs on its own threads and reports through events; this file starts it, forwards
//! its events to the page, and owns the tray and window. Closing the window hides it (OASIS keeps
//! running in the tray); Quit in the tray menu stops it.
//!
//! It starts in preview mode: presses are shown but nothing runs until "Run actions" is switched
//! on. That is deliberate while the Rust build is still replacing the C# app.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
mod fixes;
mod fixes_core;
mod hud;
mod overlay;
mod settings_view;
mod system;
mod models;
mod onboarding;
mod restart;
mod update;

use std::sync::Mutex;

use oasis_core::actions::{ActionKind, ActionStep, Next};
use oasis_engine::{EngineHandle, EngineStatus};
use oasis_settings::edit::{ButtonEdit, SetEdit};
use serde::{Deserialize, Serialize};
use settings_view::SettingsView;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

/// The running engine, or why there is none.
#[derive(Default)]
struct EngineSlot {
    handle: Mutex<Option<EngineHandle>>,
    error: Mutex<Option<String>>,
}

#[derive(Serialize)]
struct StatusReply {
    engine: Option<EngineStatus>,
    /// Why the engine could not start (no profiles, no Bluetooth, …).
    error: Option<String>,
}

#[tauri::command]
fn status(slot: State<'_, EngineSlot>) -> StatusReply {
    StatusReply {
        engine: slot
            .handle
            .lock()
            .ok()
            .and_then(|h| h.as_ref().map(EngineHandle::status)),
        error: slot.error.lock().ok().and_then(|e| e.clone()),
    }
}

/// Run `f` on the engine, or say why there is none.
fn with_engine<R>(
    slot: &EngineSlot,
    f: impl FnOnce(&EngineHandle) -> Result<R, String>,
) -> Result<R, String> {
    let guard = slot
        .handle
        .lock()
        .map_err(|_| "OASIS is busy; try again.".to_string())?;
    match guard.as_ref() {
        Some(engine) => f(engine),
        None => Err("OASIS is not running, so settings cannot be changed.".to_string()),
    }
}

/// The profiles and their buttons, for the settings screen.
#[tauri::command]
fn settings_view(slot: State<'_, EngineSlot>) -> Result<SettingsView, String> {
    with_engine(&slot, |engine| Ok(settings_view::view(&engine.settings())))
}

/// A step as the page sends it.
#[derive(Deserialize)]
struct StepInput {
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
struct ButtonInput {
    kind: String,
    value: String,
    label: String,
    steps: Vec<StepInput>,
}

/// The window says what its keyboard focus is (0 none, 1 typing in a text box, 2 recording a shortcut),
/// so typing there is never taken for a keyboard button.
#[tauri::command]
fn set_typing(mode: u8) {
    oasis_platform::ports::set_window_typing(mode);
}

/// A keyboard button pressed or let go in the OASIS window, which the system-wide key listener is not
/// given while the window is in front. It runs like any other keyboard press.
#[tauri::command]
fn press_button(button: u8, down: bool) {
    oasis_platform::ports::press_from_window(button, down);
}

/// Caps Lock chosen as a button and pressed in the OASIS window: Windows has already switched the lock
/// (the window cannot stop that, as the system-wide listener can elsewhere), so switch it back. The key is
/// still held down, and a press of a key that is down is only a repeat, so it is let go first.
#[tauri::command]
fn restore_caps_lock() {
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
fn sound_list() -> Vec<(&'static str, &'static str, bool)> {
    oasis_core::actions::sounds::SOUNDS.iter().map(|s| (s.id, s.name, s.success)).collect()
}

/// Plays a sound once, so it can be heard before it is chosen.
#[tauri::command]
fn play_sound(id: String) {
    #[cfg(windows)]
    if let Some(wav) = oasis_core::actions::sounds::wav_cached(&id) {
        oasis_win::system::play_wav(wav);
    }
    #[cfg(not(windows))]
    let _ = id;
}

/// "Try it": run one saved button's action now.
#[tauri::command]
fn try_button(slot: State<'_, EngineSlot>, profile: usize, config: usize, button: usize) -> Result<(), String> {
    with_engine(&slot, |engine| engine.try_button(profile, config, button))
}

/// Change one button and save. The error text is meant for the person editing.
#[tauri::command]
fn save_button(
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
fn save_profile(
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
fn save_input(
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
fn save_profile_look(
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

/// Give a profile a picture icon (a small PNG from the page), or remove it (`None`). The picture is
/// kept in the data folder's `profile_icons/`; the one it replaces is deleted if nothing else uses it.
#[tauri::command]
fn set_profile_icon(
    slot: State<'_, EngineSlot>,
    profile: usize,
    png: Option<Vec<u8>>,
) -> Result<(), String> {
    with_engine(&slot, |engine| {
        let root = engine.paths().app_data_root.clone();
        let old = engine
            .settings()
            .profiles
            .get(profile)
            .map(|p| p.icon_path.clone())
            .ok_or("No such profile.")?;
        let new_path = match png {
            None => String::new(),
            Some(bytes) => {
                if bytes.len() > 512 * 1024 || !bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
                    return Err("That is not a small PNG picture.".into());
                }
                let folder = engine.paths().profile_icons_folder();
                std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
                let id = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_millis());
                std::fs::write(folder.join(format!("{id}.png")), bytes)
                    .map_err(|e| e.to_string())?;
                format!("profile_icons/{id}.png")
            }
        };
        engine
            .change(|s| s.set_profile_icon_path(profile, &new_path))
            .map_err(|e| e.to_string())?;
        let still_used = engine
            .settings()
            .profiles
            .iter()
            .any(|p| p.icon_path == old);
        if !still_used {
            oasis_platform::profile_icons::delete_managed(&root, &old);
        }
        Ok(())
    })
}

/// The picture icons profiles have, as data addresses the page can show, by profile number.
#[tauri::command]
fn profile_icons(slot: State<'_, EngineSlot>) -> std::collections::BTreeMap<usize, String> {
    use base64::Engine as _;
    with_engine(&slot, |engine| {
        let root = engine.paths().app_data_root.clone();
        Ok(engine
            .settings()
            .profiles
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let file = oasis_platform::profile_icons::resolve_managed(&root, &p.icon_path)?;
                let bytes = std::fs::read(file).ok()?;
                Some((
                    i,
                    format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(bytes)
                    ),
                ))
            })
            .collect())
    })
    .unwrap_or_default()
}

/// A program's own icon as a 64-pixel PNG. `target` is a file path, or the name of a program that is
/// open or installed.
#[tauri::command]
async fn app_icon(target: String) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            use base64::Engine as _;
            use std::os::windows::process::CommandExt;
            let target = target.split(',').next().unwrap_or("").trim().to_string();
            if target.is_empty() {
                return Err("Say which app first.".to_string());
            }
            // A file path as it is; else an open program of that name; else the installed list.
            let path = if std::path::Path::new(&target).is_file() {
                target.clone()
            } else {
                // Names are compared as letters and digits only, so "apple music" finds AppleMusic.exe.
                let squash = |s: &str| s.trim_end_matches(".exe").chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
                let want = squash(&target);
                let exact = |n: &str| squash(n) == want;
                let loose = |n: &str| want.len() >= 3 && (squash(n).contains(&want) || (squash(n).len() >= 3 && want.contains(&squash(n))));
                let open = running_programs();
                let installed = oasis_platform::windows::app_discovery::windows_apps().installed_apps();
                open.iter().find(|r| exact(&r.name)).map(|r| r.path.clone())
                    .or_else(|| installed.iter().find(|(n, _)| exact(n)).map(|(_, p)| p.clone()))
                    .or_else(|| open.iter().find(|r| loose(&r.name)).map(|r| r.path.clone()))
                    .or_else(|| installed.iter().find(|(n, _)| loose(n)).map(|(_, p)| p.clone()))
                    .ok_or("OASIS cannot find that program to read its icon.")?
            };
            const SCRIPT: &str = "Add-Type -AssemblyName System.Drawing; $i = [System.Drawing.Icon]::ExtractAssociatedIcon($env:OASIS_ICON_FILE); if (-not $i) { exit 1 }; $b = New-Object System.Drawing.Bitmap 64, 64; $g = [System.Drawing.Graphics]::FromImage($b); $g.InterpolationMode = 'HighQualityBicubic'; $g.DrawImage($i.ToBitmap(), 0, 0, 64, 64); $m = New-Object System.IO.MemoryStream; $b.Save($m, [System.Drawing.Imaging.ImageFormat]::Png); [Convert]::ToBase64String($m.ToArray())";
            let out = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
                .env("OASIS_ICON_FILE", &path)
                .creation_flags(0x0800_0000)
                .output()
                .map_err(|e| e.to_string())?;
            let text = String::from_utf8_lossy(&out.stdout);
            base64::engine::general_purpose::STANDARD
                .decode(text.trim())
                .map_err(|_| "That program has no icon OASIS can read.".to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        Err("App icons are only read on Windows.".into())
    }
}

/// A website's logo as a 64-pixel PNG: the icon its page names, else `/favicon.ico`, fetched from the
/// site itself (no third party is asked).
#[tauri::command]
async fn site_icon(host: String) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || {
            use base64::Engine as _;
            use std::os::windows::process::CommandExt;
            // A pasted address is cut down to its host; a bare name ("instagram") is taken as .com.
            let host = host.trim().to_lowercase();
            let host = host.split("://").last().unwrap_or("").split(['/', '?', '#', ':']).next().unwrap_or("").to_string();
            let host = if !host.is_empty() && !host.contains('.') { format!("{host}.com") } else { host };
            let plain = !host.is_empty()
                && host.len() <= 253
                && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
            if !plain {
                return Err("That is not a website address.".to_string());
            }
            // The script is passed encoded, so its quotes and new lines reach PowerShell untouched.
            let script: Vec<u8> = include_str!("site_icon.ps1").encode_utf16().flat_map(u16::to_le_bytes).collect();
            let out = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &base64::engine::general_purpose::STANDARD.encode(script)])
                .env("OASIS_SITE", &host)
                .creation_flags(0x0800_0000)
                .output()
                .map_err(|e| e.to_string())?;
            base64::engine::general_purpose::STANDARD
                .decode(String::from_utf8_lossy(&out.stdout).trim())
                .map_err(|_| "That site has no icon OASIS can read.".to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = host;
        Err("Site icons are only read on Windows.".into())
    }
}

/// What the page sends to create or change a set of buttons.
#[derive(Deserialize)]
struct SetInput {
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
fn set_multi_config(
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
fn add_config(
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
fn update_config(
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
fn delete_config(slot: State<'_, EngineSlot>, profile: usize, config: usize) -> Result<(), String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| s.delete_config(profile, config))
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
}

/// Add a profile and return its index. `target` is what it applies to (blank = everything).
#[tauri::command]
fn add_profile(
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
fn delete_profile(slot: State<'_, EngineSlot>, profile: usize) -> Result<(), String> {
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
fn move_profile(slot: State<'_, EngineSlot>, from: usize, to: usize) -> Result<(), String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| s.move_profile(from, to))
            .map_err(|e| e.to_string())
    })
}

/// How the overlay should look, from the settings.
#[tauri::command]
fn overlay_look(
    slot: State<'_, EngineSlot>,
) -> Result<oasis_settings::overlay::OverlayAppearance, String> {
    with_engine(&slot, |engine| {
        Ok(oasis_settings::overlay::overlay_appearance(
            &engine.settings(),
        ))
    })
}

/// Size the overlay for its arrangement and put it on its dock (or where it was dragged). The
/// overlay page asks for this when it loads and whenever the settings change.
#[tauri::command]
fn overlay_relayout(app: AppHandle, slot: State<'_, EngineSlot>) -> Result<(), String> {
    let (saved, dock, popup_vertical) = with_engine(&slot, |engine| {
        let settings = engine.settings();
        Ok((
            oasis_settings::overlay::overlay_position(&settings),
            oasis_settings::overlay::overlay_appearance(&settings).dock,
            oasis_settings::overlay::popup_vertical(&settings),
        ))
    })?;
    // The voice pop-up follows the same settings; it is told here, outside any engine callback.
    hud::set_side(match (popup_vertical, dock.as_str()) {
        (false, _) => hud::Side::Bottom,
        (true, "right") => hud::Side::Right,
        (true, _) => hud::Side::Left,
    });
    overlay::place(&app, saved, &dock).map_err(|e| e.to_string())
}

/// The HUD's answer to a word fixed by hand: save it to the dictionary, or not.
#[tauri::command]
fn learn_answer(
    app: AppHandle,
    slot: State<'_, EngineSlot>,
    save: bool,
    words: Vec<(String, String)>,
) -> Result<(), String> {
    hud::end_learn(&app);
    if !save {
        return Ok(());
    }
    with_engine(&slot, |engine| {
        engine
            .change(|s| {
                words.iter().try_for_each(|(heard, written)| {
                    oasis_settings::fields::add_glossary_word(s, heard, written)
                })
            })
            .map_err(|e| e.to_string())
    })
}

/// Keep the overlay in front of other windows, and with more than one screen, on the one the person is
/// working on: the screen the mouse pointer is on. Checked a few times a second; nothing happens with a single screen or while the
/// overlay is being moved by hand.
#[cfg(windows)]
fn follow_the_screen_in_use(app: AppHandle) {
    std::thread::spawn(move || {
        let (mut front, mut tick) = (0i64, 0u32);
        let mut monitors: Vec<oasis_win::desktop::Monitor> = Vec::new();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            // Keep the overlay in front: when another window comes forward, and every 2 s.
            tick = tick.wrapping_add(1);
            let now = oasis_win::desktop::foreground();
            if now != front || tick % 20 == 0 {
                front = now;
                if let Some(hwnd) = app.get_webview_window(overlay::LABEL).and_then(|w| w.hwnd().ok()) {
                    oasis_win::desktop::raise_topmost(hwnd.0 as i64);
                }
            }
            let Some(at) = oasis_win::desktop::cursor_monitor() else {
                continue;
            };
            // The screens change only when one is plugged in, moved or resized: listed once a second,
            // and at once when the pointer is on one not in the list.
            if tick % 10 == 0 || !monitors.iter().any(|m| m.handle == at) {
                monitors = oasis_win::desktop::monitors();
            }
            if monitors.len() < 2 {
                continue;
            }
            let Some(screen) = monitors.iter().find(|m| m.handle == at) else {
                continue;
            };
            // Compare with where the overlay really is, not with where it was last put: a layout or settings
            // change puts it back on the main screen without telling this loop.
            let on = app
                .get_webview_window(overlay::LABEL)
                .and_then(|w| w.hwnd().ok())
                .and_then(|h| oasis_win::desktop::read(h.0 as i64))
                .map(|p| p.monitor);
            if on == Some(at) {
                continue;
            }
            let slot = app.state::<EngineSlot>();
            let Ok(guard) = slot.handle.lock() else {
                continue;
            };
            let Some(engine) = guard.as_ref() else {
                continue;
            };
            let settings = engine.settings();
            let (saved, dock) = (
                oasis_settings::overlay::overlay_position(&settings),
                oasis_settings::overlay::overlay_appearance(&settings).dock,
            );
            drop(guard);
            // A move held off by "Move overlay" is simply retried on the next pass.
            let _ = overlay::follow(&app, saved, &dock, (screen.bounds.left, screen.bounds.top));
        }
    });
}

/// Open the folder holding the browser extension, for "Load unpacked" in the browser.
#[tauri::command]
fn open_extension_folder(slot: State<'_, EngineSlot>) -> Result<(), String> {
    with_engine(&slot, |engine| {
        let folder = oasis_platform::extension_install::published_folder(engine.paths());
        if !folder.join("manifest.json").is_file() {
            return Err(
                "The extension has not been copied yet. Start OASIS with the browser bridge switched on, and keep the extensions folder next to the program.".to_string(),
            );
        }
        #[cfg(windows)]
        {
            oasis_win::shell::open(&folder.to_string_lossy(), None)
                .map_err(|code| format!("Windows could not open the folder (code {code})."))
        }
        #[cfg(not(windows))]
        {
            Err(format!("The extension is in {}", folder.display()))
        }
    })
}

/// The kept dictations, newest first, for the Insights screen.
#[tauri::command]
fn history_list(slot: State<'_, EngineSlot>) -> Result<Vec<oasis_engine::history::Entry>, String> {
    with_engine(&slot, |engine| Ok(oasis_engine::history::list(&engine.paths().dictation_history_folder())))
}

/// One kept recording as base64 WAV, for the page to play.
#[tauri::command]
fn history_audio(slot: State<'_, EngineSlot>, id: String) -> Result<String, String> {
    use base64::Engine as _;
    with_engine(&slot, |engine| {
        let wav = oasis_engine::history::audio(&engine.paths().dictation_history_folder(), &id)?;
        Ok(base64::engine::general_purpose::STANDARD.encode(wav))
    })
}

/// Forget one kept dictation (recording and text).
#[tauri::command]
fn history_delete(slot: State<'_, EngineSlot>, id: String) -> Result<(), String> {
    with_engine(&slot, |engine| oasis_engine::history::delete(&engine.paths().dictation_history_folder(), &id))
}

/// Forget every kept dictation (recordings and texts).
#[tauri::command]
fn history_delete_all(slot: State<'_, EngineSlot>) -> Result<(), String> {
    with_engine(&slot, |engine| oasis_engine::history::delete_all(&engine.paths().dictation_history_folder()))
}

/// Build the training set (audio + `metadata.jsonl`) inside the history folder and show it.
/// Returns how many clips it holds.
#[tauri::command]
fn history_export_dataset(slot: State<'_, EngineSlot>) -> Result<usize, String> {
    with_engine(&slot, |engine| {
        let (dest, n) = oasis_engine::history::export_dataset(&engine.paths().dictation_history_folder())?;
        #[cfg(windows)]
        let _ = oasis_win::shell::open(&dest.to_string_lossy(), None);
        let _ = &dest;
        Ok(n)
    })
}

/// Show the folder of kept dictations, to share it or clear it out.
#[tauri::command]
fn open_history_folder(slot: State<'_, EngineSlot>) -> Result<(), String> {
    with_engine(&slot, |engine| {
        let folder = engine.paths().dictation_history_folder();
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            oasis_win::shell::open(&folder.to_string_lossy(), None)
                .map_err(|code| format!("Windows could not open the folder (code {code})."))
        }
        #[cfg(not(windows))]
        {
            Err(format!("The history is in {}", folder.display()))
        }
    })
}

/// Every simple setting, grouped in sections, with its current value, for the Settings tab.
#[derive(Serialize)]
struct FieldsReply {
    sections: Vec<oasis_settings::fields::SectionView>,
    /// Things worth knowing about the current choices (two roles on one button, …).
    warnings: Vec<String>,
}

fn field_context() -> oasis_settings::fields::FieldContext {
    oasis_settings::fields::FieldContext {
        microphones: oasis_voice::device_names(),
        browsers: installed_browsers(),
    }
}

#[cfg(windows)]
fn installed_browsers() -> Vec<(String, String)> {
    use oasis_platform::windows::actions::{installed_browsers, BROWSERS};
    installed_browsers()
        .into_iter()
        .filter_map(|name| BROWSERS.iter().find(|(n, _)| *n == name).map(|(n, shown)| (n.to_string(), shown.to_string())))
        .collect()
}

#[cfg(not(windows))]
fn installed_browsers() -> Vec<(String, String)> {
    Vec::new()
}

/// The Groq API key lives in the Windows-protected secret store, never in `profiles.json`.
#[cfg(windows)]
fn groq_store() -> Result<oasis_platform::windows::dpapi::DpapiSecretStore, String> {
    let paths = oasis_engine::defaults::detect_paths().ok_or("No data folder was found.")?;
    Ok(oasis_platform::windows::dpapi::DpapiSecretStore::new(
        paths.secrets_folder(),
    ))
}

/// Whether a Groq key is stored. The key itself is never sent to the window.
#[tauri::command]
fn groq_key_status() -> bool {
    #[cfg(windows)]
    {
        groq_store()
            .ok()
            .is_some_and(|s| oasis_engine::defaults::windows::groq_key(&s).is_some())
    }
    #[cfg(not(windows))]
    false
}

/// Store the key (a blank one removes it). The window asks the engine to reload speech afterwards.
#[tauri::command]
fn set_groq_key(slot: State<'_, EngineSlot>, key: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use oasis_platform::ports::SecretStore;
        let store = groq_store()?;
        let key = key.trim();
        if key.is_empty() {
            store
                .clear(oasis_platform::secrets::GROQ_API_KEY)
                .map_err(|e| e.to_string())?;
        } else {
            store
                .set(oasis_platform::secrets::GROQ_API_KEY, key)
                .map_err(|e| e.to_string())?;
        }
        // The speech engine may have been waiting for a key: let it look again.
        with_engine(&slot, |engine| engine.reload_settings())
    }
    #[cfg(not(windows))]
    {
        let _ = (slot, key);
        Err("Groq is only wired up on Windows.".into())
    }
}

/// Try the stored key with a quarter second of silence. Returns what to tell the user.
#[tauri::command]
async fn test_groq_key(slot: State<'_, EngineSlot>) -> Result<String, String> {
    #[cfg(windows)]
    {
        let model = with_engine(&slot, |engine| {
            Ok(oasis_settings::voice::VoiceSettings::from(&engine.settings()).groq_model)
        })?;
        let key = oasis_engine::defaults::windows::groq_key(&groq_store()?)
            .ok_or("No Groq key is stored yet.")?;
        Ok(match oasis_engine::groq::test_key(&key, &model) {
            None => "The key works.".to_string(),
            Some(reason) => reason,
        })
    }
    #[cfg(not(windows))]
    {
        let _ = slot;
        Err("Groq is only wired up on Windows.".into())
    }
}

/// Spotify's saved sign-in (shared with the C# app) and where it is set up.
#[cfg(windows)]
fn spotify_auth() -> Result<std::sync::Arc<oasis_engine::spotify::Auth>, String> {
    let paths = oasis_engine::defaults::detect_paths().ok_or("No data folder was found.")?;
    Ok(std::sync::Arc::clone(
        &oasis_engine::defaults::windows::spotify_hub(&paths).auth,
    ))
}

#[derive(Serialize)]
struct InstalledApp {
    name: String,
    /// A Start Menu shortcut, a program, or a Store app id.
    path: String,
}

/// Installed apps for the action field. Cached; the first call can take a few seconds.
#[tauri::command]
async fn installed_apps() -> Vec<InstalledApp> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let apps = oasis_platform::windows::app_discovery::windows_apps().installed_apps();
            apps.into_iter()
                .map(|(name, path)| InstalledApp { name, path })
                .collect()
        })
        .await
        .unwrap_or_default()
    }
    #[cfg(not(windows))]
    Vec::new()
}

#[derive(Serialize)]
struct RunningApp {
    name: String,
    /// The program's file, which the Launch action can start again.
    path: String,
    title: String,
}

/// Programs with a window open right now (one PowerShell call).
#[cfg(windows)]
fn running_programs() -> Vec<RunningApp> {
    use std::os::windows::process::CommandExt;
    const SCRIPT: &str = "Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowTitle } | ForEach-Object { [pscustomobject]@{ n = $_.ProcessName; p = $_.Path; t = $_.MainWindowTitle } } | ConvertTo-Json -Compress";
    let Ok(out) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .creation_flags(0x0800_0000) // no console window
        .output()
    else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return Vec::new();
    };
    // One result comes back as an object, several as an array.
    let rows = match json {
        serde_json::Value::Array(rows) => rows,
        one @ serde_json::Value::Object(_) => vec![one],
        _ => Vec::new(),
    };
    let text = |row: &serde_json::Value, key: &str| {
        row.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut apps = Vec::new();
    for row in &rows {
        let (name, path, title) = (text(row, "n"), text(row, "p"), text(row, "t"));
        // Skip what cannot be launched again, this app itself, and the Store-app frame.
        if path.is_empty()
            || name.eq_ignore_ascii_case("oasis-desktop")
            || name.eq_ignore_ascii_case("ApplicationFrameHost")
        {
            continue;
        }
        if seen.insert(path.to_lowercase()) {
            apps.push(RunningApp { name, path, title });
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Programs with a window open right now, for the action field. Asked for when the box is clicked
/// rather than kept.
#[tauri::command]
async fn running_apps() -> Vec<RunningApp> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(running_programs)
            .await
            .unwrap_or_default()
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// What was in front when the user clicked a section, for "pick the section" on a new set.
#[derive(Serialize)]
struct Captured {
    panel_id: String,
    panel_label: String,
    /// What to put in "window title or address contains": the site for a browser, else the title.
    context: String,
    /// The program clicked, as its process name ("code", "chrome").
    process: String,
}

/// What the overlay says while a section is being picked: seconds `left` to click, or what was `done` (caught).
#[derive(Serialize, Clone)]
struct PickPrompt {
    left: Option<u64>,
    done: Option<String>,
}

/// "Pick the section": OASIS gets out of the way (minimised), the overlay asks for a click and counts down,
/// and the first click in another program is read: which program and which part of it has the focus. Then
/// OASIS comes back. Esc, or no click in time, gives up.
#[tauri::command]
async fn capture_focus(app: AppHandle) -> Result<Captured, String> {
    #[cfg(windows)]
    {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.minimize();
        }
        let waiting = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || wait_for_pick(&waiting))
            .await
            .map_err(|e| e.to_string())?;
        let said = result.as_ref().ok().map(|c| {
            let what = if !c.panel_label.is_empty() {
                c.panel_label.clone()
            } else if !c.context.is_empty() {
                c.context.clone()
            } else {
                c.process.clone()
            };
            PickPrompt {
                left: None,
                done: Some(format!("Caught: {what}")),
            }
        });
        let _ = app.emit_to(overlay::LABEL, "pick-prompt", said);
        show_window(&app);
        result
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("Picking a section is only available on Windows.".into())
    }
}

/// How long the overlay waits for the click.
#[cfg(windows)]
const PICK_SECONDS: u64 = 20;

/// Waits for a click in a program other than OASIS and reads what it gave the focus to.
#[cfg(windows)]
fn wait_for_pick(app: &AppHandle) -> Result<Captured, String> {
    use oasis_win::keyboard::is_down;
    const LEFT_BUTTON: u16 = 0x01;
    const ESC: u16 = 0x1B;
    let start = std::time::Instant::now();
    let mut was_down = is_down(LEFT_BUTTON);
    let mut said = u64::MAX;
    loop {
        let left = PICK_SECONDS.saturating_sub(start.elapsed().as_secs());
        if left != said {
            said = left;
            let _ = app.emit_to(
                overlay::LABEL,
                "pick-prompt",
                Some(PickPrompt {
                    left: Some(left),
                    done: None,
                }),
            );
        }
        if left == 0 {
            return Err(format!(
                "No click in {PICK_SECONDS} seconds. Press Pick the section to try again."
            ));
        }
        if is_down(ESC) {
            return Err("Picking was cancelled.".into());
        }
        let down = is_down(LEFT_BUTTON);
        if was_down && !down {
            // the click has finished; give the program a moment to move the focus where it was clicked
            std::thread::sleep(std::time::Duration::from_millis(250));
            if let Ok(found) = read_focus() {
                return Ok(found);
            } // a click on OASIS itself (its taskbar button, say) does not count: keep waiting
        }
        was_down = down;
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

/// The program and panel that have the focus right now.
#[cfg(windows)]
fn read_focus() -> Result<Captured, String> {
    let mut tracker = oasis_platform::windows::foreground::foreground_app();
    tracker.set_panel_profiles_exist(true);
    let info = tracker.current_with_fresh_url();
    if info.process.eq_ignore_ascii_case("oasis-desktop") || info.process.is_empty() {
        return Err("OASIS itself was in front. Click the other program's section.".to_string());
    }
    let known = info.panel_id != "unknown" && !info.panel_id.is_empty();
    let site = info
        .browser_url
        .as_deref()
        .and_then(|u| u.split("://").nth(1))
        .and_then(|r| r.split('/').next())
        .unwrap_or("");
    // A panel is the condition by itself; a title would change with every file.
    let context = if known {
        String::new()
    } else if !site.is_empty() {
        site.to_string()
    } else {
        info.window_title.clone()
    };
    Ok(Captured {
        panel_id: if known { info.panel_id } else { String::new() },
        panel_label: info.panel_label,
        context,
        process: info.process,
    })
}

// ---- Quick Launch ----------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy)]
struct RegionDto {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

/// One thing a setup opens, as the page edits it.
#[derive(Serialize, Deserialize)]
struct QuickItem {
    /// `app`, `url` or `folder`.
    kind: String,
    target: String,
    open_with: String,
    /// `""` (where it opens), `main`, `other:N`, `left`, `right` or `screen:N`.
    screen: String,
    /// 0 as opened, 1 maximized, 2 left half, 3 right half.
    layout: i64,
    region: Option<RegionDto>,
    open_another: bool,
}

#[derive(Serialize)]
struct QuickSetup {
    id: String,
    name: String,
    /// What to say besides the name, as typed (commas between several).
    call_name: String,
    start_with_oasis: bool,
    items: Vec<QuickItem>,
}

#[derive(Deserialize)]
struct QuickSetupInput {
    id: String,
    name: String,
    call_name: String,
    start_with_oasis: bool,
    items: Vec<QuickItem>,
}

fn layout_number(layout: oasis_core::placement::Layout) -> i64 {
    use oasis_core::placement::Layout;
    match layout {
        Layout::AsIs => 0,
        Layout::Maximized => 1,
        Layout::LeftHalf => 2,
        Layout::RightHalf => 3,
    }
}

/// Every setup in `profiles.json` with its items.
#[tauri::command]
fn quick_setups(slot: State<'_, EngineSlot>) -> Result<Vec<QuickSetup>, String> {
    use oasis_settings::quick_launch::{setups, ItemKind};
    with_engine(&slot, |engine| {
        Ok(setups(&engine.settings())
            .into_iter()
            .map(|s| QuickSetup {
                id: s.id,
                name: s.name,
                call_name: s.voice_phrases.join(", "),
                start_with_oasis: s.start_with_oasis,
                items: s
                    .items
                    .into_iter()
                    .map(|i| QuickItem {
                        kind: match i.kind {
                            ItemKind::App => "app",
                            ItemKind::Url => "url",
                            ItemKind::Folder => "folder",
                        }
                        .into(),
                        target: i.target,
                        open_with: i.open_with,
                        screen: i.screen,
                        layout: layout_number(i.layout),
                        region: i.region.map(|r| RegionDto {
                            x: r.x,
                            y: r.y,
                            w: r.w,
                            h: r.h,
                        }),
                        open_another: i.open_another,
                    })
                    .collect(),
            })
            .collect())
    })
}

/// Create or change a setup; returns its id.
#[tauri::command]
fn save_quick_setup(slot: State<'_, EngineSlot>, setup: QuickSetupInput) -> Result<String, String> {
    use oasis_settings::quick_launch::{ItemEdit, SetupEdit};
    let edit = SetupEdit {
        id: setup.id,
        name: setup.name,
        call_name: setup.call_name,
        start_with_oasis: setup.start_with_oasis,
        items: setup
            .items
            .into_iter()
            .map(|i| ItemEdit {
                kind: i.kind,
                target: i.target,
                open_with: i.open_with,
                screen: i.screen,
                layout: i.layout,
                region: i.region.and_then(|r| {
                    oasis_core::placement::Region::new(
                        r.x.into(),
                        r.y.into(),
                        r.w.into(),
                        r.h.into(),
                    )
                }),
                open_another: i.open_another,
            })
            .collect(),
    };
    with_engine(&slot, |engine| {
        engine
            .change(|s| {
                s.save_setup(edit)
                    .map_err(oasis_settings::edit::EditError::Invalid)
            })
            .map_err(|e| e.to_string())
    })
}

#[tauri::command]
fn delete_quick_setup(slot: State<'_, EngineSlot>, id: String) -> Result<(), String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| {
                s.delete_setup(&id)
                    .map_err(oasis_settings::edit::EditError::Invalid)
            })
            .map_err(|e| e.to_string())
    })
}

/// A connected screen as the page draws it: where it sits among the others, its size, and the spec
/// an item stores to land on it.
#[derive(Serialize)]
struct QuickScreen {
    spec: String,
    primary: bool,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

#[tauri::command]
fn quick_screens() -> Vec<QuickScreen> {
    #[cfg(windows)]
    {
        use oasis_core::placement::{screen_spec, Desktop};
        let screens = oasis_engine::defaults::windows::WindowsDesktop.screens();
        let mut out: Vec<QuickScreen> = (0..screens.len())
            .map(|i| {
                let b = screens[i].bounds;
                QuickScreen {
                    spec: screen_spec(&screens, i),
                    primary: screens[i].primary,
                    left: b.left,
                    top: b.top,
                    width: b.width(),
                    height: b.height(),
                }
            })
            .collect();
        out.sort_by_key(|s| (s.left, s.top));
        out
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// Every window open right now as an item, each on its screen and in its place, for "Capture".
#[tauri::command]
async fn capture_windows() -> Result<Vec<QuickItem>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            Ok(oasis_engine::defaults::windows::capture_items()
                .into_iter()
                .map(|i| QuickItem {
                    kind: i.kind,
                    target: i.target,
                    open_with: i.open_with,
                    screen: i.screen,
                    layout: i.layout,
                    region: i.region.map(|r| RegionDto { x: r.x, y: r.y, w: r.w, h: r.h }),
                    open_another: i.open_another,
                })
                .collect())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    Err("Capture is only available on Windows.".into())
}

/// Open a setup now; returns the one-line result ("Project Mode: 3 of 3 ready.").
#[tauri::command]
async fn run_quick_setup(slot: State<'_, EngineSlot>, id: String) -> Result<String, String> {
    #[cfg(windows)]
    {
        let setup = with_engine(&slot, |engine| {
            oasis_settings::quick_launch::find(&engine.settings(), &id)
                .ok_or_else(|| "That setup is no longer there.".to_string())
        })?;
        tauri::async_runtime::spawn_blocking(move || {
            let (setup, skipped) = oasis_settings::quick_launch::without_blocked(&setup);
            let parts = oasis_engine::defaults::windows::quick_launch_parts();
            let results = oasis_engine::quick_launch::Runner::new(
                &parts,
                oasis_engine::quick_launch::Timing::REAL,
            )
            .run(&setup);
            let mut text = oasis_engine::quick_launch::summary(&setup.name, &results);
            if !skipped.is_empty() {
                text.push_str(&format!(" {}.", skipped.join("; ")));
            }
            Ok(text)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = (slot, id);
        Err("Quick Launch runs only on Windows.".into())
    }
}

/// Presses per profile name, for ordering profiles by use.
#[tauri::command]
fn profile_use_counts(slot: State<'_, EngineSlot>) -> std::collections::BTreeMap<String, u64> {
    slot.handle
        .lock()
        .ok()
        .and_then(|h| h.as_ref().map(EngineHandle::profile_use_counts))
        .unwrap_or_default()
}

/// One line for the settings screen: set up, connected or not.
#[tauri::command]
async fn spotify_status() -> String {
    #[cfg(windows)]
    {
        // Really tries the sign-in (it may need the network), so it runs off the window thread.
        tauri::async_runtime::spawn_blocking(|| spotify_auth().map_or_else(|e| e, |a| a.verify()))
            .await
            .unwrap_or_else(|e| e.to_string())
    }
    #[cfg(not(windows))]
    "Spotify is only wired up on Windows.".to_string()
}

/// Sign in through the browser. Waits up to two minutes for you to finish there.
#[tauri::command]
async fn spotify_connect() -> Result<String, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(|| {
            let auth = spotify_auth()?;
            auth.connect(&|url| oasis_win::shell::open(url, None).is_ok())?;
            Ok(auth.status())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    Err("Spotify is only wired up on Windows.".into())
}

#[tauri::command]
fn spotify_disconnect() -> Result<String, String> {
    #[cfg(windows)]
    {
        let auth = spotify_auth()?;
        auth.disconnect()?;
        Ok(auth.status())
    }
    #[cfg(not(windows))]
    Err("Spotify is only wired up on Windows.".into())
}

/// The calendar's state for the settings screen: one line, the error if the last refresh failed,
/// and the next few meetings.
#[derive(serde::Serialize)]
struct MeetingsReply {
    status: String,
    connected: bool,
    error: String,
    upcoming: Vec<MeetingLine>,
}

#[derive(serde::Serialize)]
struct MeetingLine {
    title: String,
    when: String,
    can_join: bool,
}

#[tauri::command]
fn meetings_status(slot: State<'_, EngineSlot>) -> Result<MeetingsReply, String> {
    with_engine(&slot, |engine| {
        let Some(meetings) = engine.meetings() else {
            return Ok(MeetingsReply {
                status: "Meetings are not available in this build.".into(),
                connected: false,
                error: String::new(),
                upcoming: vec![],
            });
        };
        #[cfg(windows)]
        let offset = oasis_win::system::utc_offset_seconds();
        #[cfg(not(windows))]
        let offset = 0;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        Ok(MeetingsReply {
            status: meetings.calendar().status(),
            connected: meetings.calendar().connected(),
            error: meetings.last_error(),
            upcoming: meetings
                .upcoming_lines(now, offset, 6)
                .into_iter()
                .map(|(title, when, can_join)| MeetingLine {
                    title,
                    when,
                    can_join,
                })
                .collect(),
        })
    })
}

/// Sign in to Google in the browser (waits up to five minutes), then load the meetings.
#[tauri::command]
async fn meetings_connect(slot: State<'_, EngineSlot>) -> Result<String, String> {
    let meetings = with_engine(&slot, |engine| {
        engine
            .meetings()
            .ok_or_else(|| "Meetings are not available in this build.".to_string())
    })?;
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        let open = |url: &str| oasis_win::shell::open(url, None).is_ok();
        #[cfg(not(windows))]
        let open = |_: &str| false;
        meetings.calendar().connect(&open)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        meetings.refresh(now);
        Ok(meetings.calendar().status())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn meetings_disconnect(slot: State<'_, EngineSlot>) -> Result<String, String> {
    with_engine(&slot, |engine| {
        let meetings = engine
            .meetings()
            .ok_or("Meetings are not available in this build.")?;
        meetings.calendar().disconnect()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        meetings.refresh(now);
        Ok(meetings.calendar().status())
    })
}

/// A button on the meeting reminder: `join`, `snooze` or `dismiss`.
#[tauri::command]
fn meeting_reminder_action(slot: State<'_, EngineSlot>, action: String) -> Result<(), String> {
    with_engine(&slot, |engine| engine.answer_meeting_reminder(&action))
}

#[tauri::command]
fn settings_fields(slot: State<'_, EngineSlot>) -> Result<FieldsReply, String> {
    with_engine(&slot, |engine| {
        let settings = engine.settings();
        Ok(FieldsReply {
            sections: oasis_settings::fields::sections(&settings, &field_context()),
            warnings: oasis_settings::fields::warnings(&settings),
        })
    })
}

/// Change one simple setting and save. The error text is meant for the person editing.
#[tauri::command]
fn set_setting(
    slot: State<'_, EngineSlot>,
    id: String,
    value: serde_json::Value,
) -> Result<FieldsReply, String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| oasis_settings::fields::set_field(s, &id, value, &field_context()))
            .map_err(|e| e.to_string())?;
        let settings = engine.settings();
        Ok(FieldsReply {
            sections: oasis_settings::fields::sections(&settings, &field_context()),
            warnings: oasis_settings::fields::warnings(&settings),
        })
    })
}

/// Put the overlay's colours and sizes back to the defaults.
#[tauri::command]
fn reset_overlay_look_setting(slot: State<'_, EngineSlot>) -> Result<FieldsReply, String> {
    with_engine(&slot, |engine| {
        engine
            .change(|s| {
                oasis_settings::fields::reset_overlay_look(s);
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        let settings = engine.settings();
        Ok(FieldsReply {
            sections: oasis_settings::fields::sections(&settings, &field_context()),
            warnings: oasis_settings::fields::warnings(&settings),
        })
    })
}

/// Read `profiles.json` again (after the C# app changed it, say).
#[tauri::command]
fn reload_settings(slot: State<'_, EngineSlot>) -> Result<(), String> {
    with_engine(&slot, |engine| engine.reload_settings())
}

/// The first time the Rust app runs it has no profiles of its own (it never reads the C# app's): start it
/// with one profile for everything, which the person then builds on.
///
/// Returns whether it made the file. A new file gets its defaults here, once, and never again:
/// keyboard keys on (so the buttons work without the OASIS interface), button 1 named for voice (it
/// is the talk key by default) and button 3 showing the desktop. Button 2 is left empty; the
/// first-run guide borrows it for its demo and hands it back empty.
#[cfg(windows)]
fn ensure_profiles(paths: &oasis_platform::paths::OasisPaths) -> bool {
    if paths.profiles().exists() {
        return false;
    }
    let Some(dir) = paths.profiles().parent().map(std::path::Path::to_path_buf) else {
        return false;
    };
    let mut settings = oasis_settings::Settings::default();
    if std::fs::create_dir_all(&dir).is_err() || settings.add_profile("Default", "", "").is_err() {
        return false;
    }
    let button = |kind: &str, value: &str, label: &str| ButtonEdit {
        kind: kind.into(),
        value: value.into(),
        label: label.into(),
        steps: Vec::new(),
    };
    // Each is tried on its own: a default that is refused must not cost the profile itself.
    let _ = settings.set_button(0, 0, 0, button(ActionKind::Shortcut.as_str(), "", "Voice"));
    let _ = settings.set_button(0, 0, 2, button(ActionKind::Shortcut.as_str(), "Win+D", "Desktop"));
    let _ = settings.set_input(&oasis_settings::input::InputSettings {
        keyboard: true,
        ..Default::default()
    });
    // Horizontal until the first-run guide asks: F1 to F3, the overlay in a row at the bottom.
    settings.choose_layout(false);
    oasis_platform::profile_store::ProfileStore::new(&dir)
        .save(&settings)
        .is_ok()
}

#[cfg(windows)]
fn start_engine(app: &AppHandle, hold_keys: bool) -> Result<EngineHandle, String> {
    use oasis_engine::{defaults, start};

    let mut config =
        defaults::windows::config(true).ok_or("No home folder found on this computer.")?;
    // The first-run guide is about to open: the keyboard buttons wait for it.
    config.hold_keys_at_start = hold_keys;
    // "Start with OASIS": after sign-in Windows is still busy, so wait longer; a restart OASIS made
    // itself has the desk arranged already.
    let args = oasis_core::startup::LaunchArgs::parse(std::env::args().skip(1));
    config.startup_setup_after =
        (!args.restart).then(|| std::time::Duration::from_secs(if args.tray { 12 } else { 3 }));
    let parts = defaults::windows::parts(&config.paths).map_err(|e| e.to_string())?;
    let events = app.clone();
    start(config, parts, move |event| {
        // An alarm should be seen: bring the window forward.
        if matches!(
            event,
            oasis_engine::EngineEvent::Reminder { .. }
                | oasis_engine::EngineEvent::MeetingReminder { .. }
        ) {
            show_window(&events);
        }
        if let oasis_engine::EngineEvent::Transcript { text, typed: true } = &event {
            fixes::watch(events.clone(), text.clone());
        }
        onboarding::on_engine_event(&events, &event);
        hud::on_engine_event(&events, &event);
        // 20 times a second while recording, for the pill's bars only: an event name of its own, so
        // the other windows (whose `engine-event` listeners ask for the status each time) are not woken.
        if matches!(event, oasis_engine::EngineEvent::VoiceLevel { .. }) {
            let _ = events.emit("voice-level", &event);
            return;
        }
        let _ = events.emit("engine-event", &event);
    })
    .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
fn start_engine(_: &AppHandle, _: bool) -> Result<EngineHandle, String> {
    Err("The desktop app is only built for Windows so far.".into())
}

/// The window's contents grow with the window: on a big screen everything is drawn larger instead of
/// the same controls sitting in empty space. 1 up to a 1600 x 900 window (in screen
/// points), at most 1.5. Page zoom, so the page lays itself out as usual in fewer, larger pixels.
fn fit_zoom(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
        return;
    };
    if size.width == 0 || size.height == 0 {
        return; // minimised
    }
    let (w, h) = (
        f64::from(size.width) / scale,
        f64::from(size.height) / scale,
    );
    let _ = window.set_zoom((w / 1600.0).min(h / 900.0).clamp(1.0, 1.5));
}

/// Bring OASIS forward: the welcome window while it is open (a second launch, the tray, a reminder),
/// otherwise the main window.
fn show_window(app: &AppHandle) {
    if app.get_webview_window(onboarding::WELCOME).is_some() || onboarding::welcome_due(app) {
        onboarding::show_welcome(app);
    } else {
        show_main(app);
    }
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The tray's "Move overlay" entry, kept so its text can follow the mode.
struct MoveItem(MenuItem<tauri::Wry>);

/// Start or finish moving the overlay; finishing saves where it was left. Returns whether it is
/// being moved now.
fn toggle_overlay_move(app: &AppHandle) -> bool {
    let item = app.state::<MoveItem>();
    if let Some((x, y)) = overlay::toggle_moving(app, &item.0) {
        save_overlay_position(app, x, y);
    }
    app.state::<overlay::MoveMode>()
        .0
        .lock()
        .map(|m| *m)
        .unwrap_or(false)
}

/// The same as the tray's "Move overlay", for a button in the window.
#[tauri::command]
fn move_overlay(app: AppHandle) -> bool {
    toggle_overlay_move(&app)
}

/// Remember where the overlay was left. A spot left of or above the main screen cannot be stored
/// (the settings file only holds positions from 0), so it is stored as the nearest edge.
fn save_overlay_position(app: &AppHandle, x: f64, y: f64) {
    let slot = app.state::<EngineSlot>();
    let saved = with_engine(&slot, |engine| {
        engine
            .change(|s| {
                s.set_overlay_position(x.max(0.0).round() as i32, y.max(0.0).round() as i32)
            })
            .map_err(|e| e.to_string())
    });
    if let Err(why) = saved {
        let _ = app.emit(
            "engine-event",
            &oasis_engine::EngineEvent::Note {
                text: format!("The overlay position was not saved: {why}"),
            },
        );
    }
}

/// Stop the engine so the pad is released cleanly.
fn stop_engine(app: &AppHandle) {
    if let Some(mut engine) = app
        .state::<EngineSlot>()
        .handle
        .lock()
        .ok()
        .and_then(|mut h| h.take())
    {
        engine.stop();
    }
}

/// Stop the engine first, then leave.
fn quit(app: &AppHandle) {
    stop_engine(app);
    app.exit(0);
}

/// Keeps the single-copy lock for as long as the app runs.
struct StartupHold(#[allow(dead_code)] Mutex<system::Startup>);

fn main() {
    // A Rust panic is written to crash.log (with the calls that led to it); without this it leaves no trace.
    if let Some(paths) = oasis_platform::paths::OasisPaths::detect() {
        oasis_platform::crash_log::CrashLog::at(paths.crash_log()).install_panic_hook();
    }
    let startup = match system::begin() {
        system::Begin::Run(startup) => startup,
        system::Begin::Exit => return,
    };
    let startup_cell = Mutex::new(Some(startup));
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(EngineSlot::default())
        .manage(update::UpdateState::default())
        .manage(models::ModelsState::default())
        .manage(restart::RestartState::default())
        .invoke_handler(tauri::generate_handler![
            status,
            settings_view,
            save_button,
            set_typing,
            press_button,
            restore_caps_lock,
            sound_list,
            play_sound,
            save_profile,
            save_profile_look,
            save_input,
            add_profile,
            delete_profile,
            move_profile,
            set_multi_config,
            add_config,
            update_config,
            delete_config,
            overlay_look,
            overlay_relayout,
            learn_answer,
            move_overlay,
            open_extension_folder,
            history_list,
            history_audio,
            history_delete,
            history_delete_all,
            history_export_dataset,
            open_history_folder,
            settings_fields,
            groq_key_status,
            spotify_status,
            spotify_connect,
            spotify_disconnect,
            meetings_status,
            meetings_connect,
            meetings_disconnect,
            meeting_reminder_action,
            set_groq_key,
            test_groq_key,
            set_setting,
            reset_overlay_look_setting,
            system::system_status,
            system::set_start_with_windows,
            system::set_run_as_administrator,
            system::restart_oasis,
            reload_settings,
            installed_apps,
            running_apps,
            capture_focus,
            quick_setups,
            save_quick_setup,
            delete_quick_setup,
            quick_screens,
            capture_windows,
            run_quick_setup,
            set_profile_icon,
            profile_icons,
            app_icon,
            site_icon,
            try_button,
            profile_use_counts,
            update::update_status,
            restart::restart_pending,
            restart::engine_busy,
            models::models_status,
            models::models_choose,
            models::models_pause,
            models::models_resume,
            models::models_download_anyway,
            models::models_check_again,
            models::models_remove,
            update::check_for_update,
            update::install_update,
            onboarding::onboarding_state,
            onboarding::onboarding_event,
            onboarding::onboarding_prepare_press,
            onboarding::onboarding_try_press,
            onboarding::onboarding_check_mic,
            onboarding::onboarding_layout,
            onboarding::onboarding_open_mic_settings,
            onboarding::onboarding_test_groq,
            onboarding::onboarding_open_groq_keys
        ])
        .manage(overlay::MoveMode::default())
        .manage(onboarding::OnboardingState::default())
        .setup(move |app| {
            // Before any window is shown: is this a first run, and so the welcome window's turn?
            #[cfg(windows)]
            let created = oasis_engine::defaults::detect_paths().is_some_and(|p| ensure_profiles(&p));
            #[cfg(not(windows))]
            let created = false;
            let guide = onboarding::begin(app.handle(), created);
            if let Some(mut startup) = startup_cell.lock().ok().and_then(|mut s| s.take()) {
                let handle = app.handle().clone();
                system::watch_show_requests(&mut startup, move || show_window(&handle));
                // A start that shows nothing (the tray, "start with Windows") opens the welcome the
                // first time OASIS is asked for: `show_window` does that.
                if startup.show_window {
                    show_window(app.handle());
                }
                app.manage(StartupHold(Mutex::new(startup)));
                fit_zoom(app.handle());
            }
            let slot = app.state::<EngineSlot>();
            match start_engine(app.handle(), guide.hold) {
                Ok(engine) => *slot.handle.lock().expect("fresh lock") = Some(engine),
                Err(why) => *slot.error.lock().expect("fresh lock") = Some(why),
            }
            onboarding::engine_ready(app.handle());
            if let Err(why) = onboarding::create_panel(app.handle()) {
                eprintln!("The guide's caption panel could not be made: {why}");
            }

            let show = MenuItem::with_id(app, "show", "Show OASIS", true, None::<&str>)?;
            // The menu opens just above the pointer, so its last item is the easiest to hit by accident:
            // a separator keeps Quit apart and the label says what it stops.
            let quit_item = MenuItem::with_id(app, "quit", "Quit OASIS (pad and voice stop)", true, None::<&str>)?;
            let move_item =
                MenuItem::with_id(app, "move_overlay", "Move overlay", true, None::<&str>)?;
            let gap = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&show, &move_item, &gap, &quit_item])?;
            app.manage(MoveItem(move_item.clone()));

            let saved = slot.handle.lock().ok().and_then(|h| {
                h.as_ref()
                    .and_then(|e| oasis_settings::overlay::overlay_position(&e.settings()))
            });
            let dock = slot
                .handle
                .lock()
                .ok()
                .and_then(|h| {
                    h.as_ref()
                        .map(|e| oasis_settings::overlay::overlay_appearance(&e.settings()).dock)
                })
                .unwrap_or_else(|| "right".to_string());
            overlay::create(app.handle(), saved, &dock)?;
            hud::create(app.handle())?;
            #[cfg(windows)]
            follow_the_screen_in_use(app.handle().clone());
            let mut tray = TrayIconBuilder::new()
                .tooltip("OASIS")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => show_window(app),
                    "move_overlay" => {
                        toggle_overlay_move(app);
                    }
                    "quit" => quit(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_window(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            update::start(app.handle());
            models::start(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // The welcome window really closes, and the application takes its place.
            WindowEvent::CloseRequested { .. } if window.label() == onboarding::WELCOME => {
                onboarding::send(window.app_handle(), oasis_core::onboarding::Event::PopupClosed);
                show_main(window.app_handle());
            }
            // Back from the browser: the guide goes on.
            WindowEvent::Focused(true) if window.label() == "main" => {
                onboarding::send(window.app_handle(), oasis_core::onboarding::Event::MainFocused);
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                fit_zoom(window.app_handle())
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("the OASIS app could not start");
}
