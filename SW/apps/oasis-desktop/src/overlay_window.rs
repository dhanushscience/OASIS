//! The overlay window as the tray and the page drive it: its look, its place, following the screen in use,
//! and "Move overlay".

use super::*;

/// How the overlay should look, from the settings.
#[tauri::command]
pub fn overlay_look(
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
pub fn overlay_relayout(app: AppHandle, slot: State<'_, EngineSlot>) -> Result<(), String> {
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

/// Keep the overlay in front of other windows, and with more than one screen, on the one the person is
/// working on: the screen the mouse pointer is on. Checked a few times a second; nothing happens with a single screen or while the
/// overlay is being moved by hand.
#[cfg(windows)]
pub(crate) fn follow_the_screen_in_use(app: AppHandle) {
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

/// The tray's "Move overlay" entry, kept so its text can follow the mode.
pub(crate) struct MoveItem(pub(crate) MenuItem<tauri::Wry>);

/// Start or finish moving the overlay; finishing saves where it was left. Returns whether it is
/// being moved now.
pub(crate) fn toggle_overlay_move(app: &AppHandle) -> bool {
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
pub fn move_overlay(app: AppHandle) -> bool {
    toggle_overlay_move(&app)
}

/// Remember where the overlay was left. A spot left of or above the main screen cannot be stored
/// (the settings file only holds positions from 0), so it is stored as the nearest edge.
pub(crate) fn save_overlay_position(app: &AppHandle, x: f64, y: f64) {
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
