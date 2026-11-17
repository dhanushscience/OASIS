//! The on-screen overlay: a small transparent window that stays on top, never takes focus and
//! lets clicks pass through. It shows the three pad indicators and, on a first tap, the button
//! labels (DECISIONS D18). The page is `ui/overlay.html`; the engine's events drive it.
//!
//! "Move overlay" in the tray menu makes the window solid and draggable; choosing it again saves
//! where it was left.

use std::sync::Mutex;

use tauri::menu::MenuItem;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Runtime, WebviewUrl,
    WebviewWindowBuilder,
};

pub const LABEL: &str = "overlay";
/// Stacked down an edge: room for a label beside each bar, and the voice and confirm notes above.
const TALL: (f64, f64) = (380.0, 260.0);
/// In a row: three labels side by side over their bars.
const WIDE: (f64, f64) = (680.0, 190.0);
/// How far a docked overlay stays from the screen's edge (clear of the taskbar at the bottom).
const MARGIN: f64 = 8.0;
const TASKBAR: f64 = 48.0;

/// The window size for a dock.
pub fn size_for(dock: &str) -> (f64, f64) {
    if matches!(dock, "bottom" | "top") {
        WIDE
    } else {
        TALL
    }
}
/// How much of the window must stay on a screen for a saved position to count as reachable.
const REACHABLE: f64 = 40.0;

/// Whether the window is being moved right now.
#[derive(Default)]
pub struct MoveMode(pub Mutex<bool>);

/// One screen in logical (scaled) units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Where the overlay goes: the saved spot if some screen still shows it, otherwise its dock on
/// the main screen (right edge, vertically centred, by default, as the C# app did).
pub fn choose_position(
    saved: Option<(f64, f64)>,
    screens: &[Screen],
    main: Screen,
    dock: &str,
) -> (f64, f64) {
    choose_sized(saved, screens, main, dock, size_for(dock))
}

fn choose_sized(
    saved: Option<(f64, f64)>,
    screens: &[Screen],
    main: Screen,
    dock: &str,
    (w, h): (f64, f64),
) -> (f64, f64) {
    if let Some((x, y)) = saved {
        let reachable = screens.iter().chain(std::iter::once(&main)).any(|s| {
            x + REACHABLE < s.x + s.width
                && x + w - REACHABLE > s.x
                && y + REACHABLE < s.y + s.height
                && y + h - REACHABLE > s.y
        });
        if reachable {
            return (x, y);
        }
    }
    let (mid_x, mid_y) = (
        main.x + (main.width - w) / 2.0,
        main.y + (main.height - h) / 2.0,
    );
    match dock {
        "left" => (main.x, mid_y),
        "bottom" => (mid_x, main.y + main.height - h - TASKBAR - MARGIN),
        "top" => (mid_x, main.y + MARGIN),
        _ => (main.x + main.width - w, mid_y),
    }
}

/// How big the overlay is on a screen shorter than the main one: in proportion, never bigger, so it does not
/// fill a small screen.
pub fn size_factor(target: Screen, main: Screen) -> f64 {
    (target.height / main.height).clamp(0.7, 1.0)
}

/// Where the overlay goes when the screen in use changes: its dock on that screen, or, if it was dragged
/// somewhere, the same place in proportion (a quarter of the way down stays a quarter of the way down, an
/// edge stays an edge), so a small screen does not squeeze it against the bottom. `k` is `size_factor`.
pub fn follow_position(
    saved: Option<(f64, f64)>,
    screens: &[Screen],
    main: Screen,
    target: Screen,
    dock: &str,
    k: f64,
) -> (f64, f64) {
    let (w0, h0) = size_for(dock);
    let (w, h) = (w0 * k, h0 * k);
    let Some((x, y)) = saved else {
        return choose_sized(None, screens, target, dock, (w, h));
    };
    let home = screens
        .iter()
        .find(|s| {
            x + w0 / 2.0 >= s.x
                && x + w0 / 2.0 < s.x + s.width
                && y + h0 / 2.0 >= s.y
                && y + h0 / 2.0 < s.y + s.height
        })
        .copied()
        .unwrap_or(main);
    let frac = |at: f64, from: f64, room: f64| {
        if room > 1.0 {
            ((at - from) / room).clamp(0.0, 1.0)
        } else {
            0.5
        }
    };
    let (fx, fy) = (
        frac(x, home.x, home.width - w0),
        frac(y, home.y, home.height - h0),
    );
    (
        target.x + fx * (target.width - w).max(0.0),
        target.y + fy * (target.height - h).max(0.0),
    )
}

fn screens<R: Runtime>(app: &AppHandle<R>) -> (Vec<Screen>, Option<Screen>) {
    let convert = |m: &tauri::Monitor| {
        let scale = m.scale_factor();
        let position = m.position();
        let size = m.size();
        Screen {
            x: f64::from(position.x) / scale,
            y: f64::from(position.y) / scale,
            width: f64::from(size.width) / scale,
            height: f64::from(size.height) / scale,
        }
    };
    let all = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(convert)
        .collect();
    (
        all,
        app.primary_monitor().ok().flatten().as_ref().map(convert),
    )
}

/// Size the window for its dock and put it where it belongs. Left alone while it is being moved.
pub fn place<R: Runtime>(
    app: &AppHandle<R>,
    saved: Option<(f64, f64)>,
    dock: &str,
) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
    if app
        .state::<MoveMode>()
        .0
        .lock()
        .map(|m| *m)
        .unwrap_or(false)
    {
        return Ok(());
    }
    let (w, h) = size_for(dock);
    window.set_size(LogicalSize::new(w, h))?;
    let (all, main) = screens(app);
    if let Some(main) = main.or_else(|| all.first().copied()) {
        let (x, y) = choose_position(saved, &all, main, dock);
        window.set_position(LogicalPosition::new(x, y))?;
    }
    Ok(())
}

/// Put the overlay on the screen whose top-left corner (in pixels) is `origin`, so it is where the
/// person is looking. Returns whether it moved. Left alone while it is being moved by hand.
pub fn follow<R: Runtime>(
    app: &AppHandle<R>,
    saved: Option<(f64, f64)>,
    dock: &str,
    origin: (i32, i32),
) -> tauri::Result<bool> {
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(false);
    };
    if app
        .state::<MoveMode>()
        .0
        .lock()
        .map(|m| *m)
        .unwrap_or(false)
    {
        return Ok(false);
    }
    let monitors = app.available_monitors()?;
    let Some(target) = monitors
        .iter()
        .find(|m| (m.position().x, m.position().y) == origin)
    else {
        return Ok(false);
    };
    let (all, main) = screens(app);
    let Some(main) = main.or_else(|| all.first().copied()) else {
        return Ok(false);
    };
    let scale = target.scale_factor();
    let target = Screen {
        x: f64::from(target.position().x) / scale,
        y: f64::from(target.position().y) / scale,
        width: f64::from(target.size().width) / scale,
        height: f64::from(target.size().height) / scale,
    };
    let k = size_factor(target, main);
    let (x, y) = follow_position(saved, &all, main, target, dock, k);
    let now = window.outer_position()?;
    let s = window.scale_factor()?;
    let (from_x, from_y) = (f64::from(now.x) / s, f64::from(now.y) / s);
    if (from_x - x).abs() < 1.0 && (from_y - y).abs() < 1.0 {
        return Ok(false);
    }
    let _ = app.emit_to(LABEL, "overlay-jump", ());
    let (w, h) = size_for(dock);
    // smaller on a smaller screen: the page is zoomed with the window, so nothing is cut off
    window.set_size(LogicalSize::new(w * k, h * k))?;
    let _ = window.set_zoom(k);
    window.set_position(LogicalPosition::new(x, y))?;
    Ok(true)
}

/// Create the overlay window and put it where it belongs.
pub fn create<R: Runtime>(
    app: &AppHandle<R>,
    saved: Option<(f64, f64)>,
    dock: &str,
) -> tauri::Result<()> {
    let (w, h) = size_for(dock);
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("overlay.html".into()))
        .title("OASIS overlay")
        .inner_size(w, h)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focusable(false)
        .focused(false)
        .build()?;

    window.set_ignore_cursor_events(true)?;
    place(app, saved, dock)
}

/// Start or finish moving the overlay. Finishing returns where it ended up, in logical units, for
/// the caller to save. `item` is the tray entry whose text follows the mode.
pub fn toggle_moving<R: Runtime>(app: &AppHandle<R>, item: &MenuItem<R>) -> Option<(f64, f64)> {
    let window = app.get_webview_window(LABEL)?;
    let mode = app.state::<MoveMode>();
    let mut moving = mode.0.lock().ok()?;
    *moving = !*moving;

    let _ = window.set_ignore_cursor_events(!*moving);
    let _ = window.set_focusable(*moving);
    let _ = app.emit_to(LABEL, "overlay-moving", *moving);
    let _ = item.set_text(if *moving {
        "Done moving overlay"
    } else {
        "Move overlay"
    });

    if *moving {
        let _ = window.show();
        return None;
    }
    let scale = window.scale_factor().ok()?;
    let position = window.outer_position().ok()?;
    Some((f64::from(position.x) / scale, f64::from(position.y) / scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Screen = Screen {
        x: 0.0,
        y: 0.0,
        width: 1536.0,
        height: 864.0,
    };

    #[test]
    fn never_moved_means_right_edge_vertically_centred() {
        let (w, h) = TALL;
        assert_eq!(
            choose_position(None, &[MAIN], MAIN, "right"),
            (1536.0 - w, (864.0 - h) / 2.0)
        );
    }

    #[test]
    fn each_dock_has_its_place() {
        let (w, h) = TALL;
        assert_eq!(
            choose_position(None, &[MAIN], MAIN, "left"),
            (0.0, (864.0 - h) / 2.0)
        );
        let (w2, h2) = WIDE;
        assert_eq!(
            choose_position(None, &[MAIN], MAIN, "bottom"),
            ((1536.0 - w2) / 2.0, 864.0 - h2 - TASKBAR - MARGIN)
        );
        assert_eq!(
            choose_position(None, &[MAIN], MAIN, "top"),
            ((1536.0 - w2) / 2.0, MARGIN)
        );
        assert_eq!(size_for("top"), WIDE);
        assert_eq!(size_for("left"), (w, h));
    }

    #[test]
    fn a_saved_spot_on_a_screen_is_kept() {
        assert_eq!(
            choose_position(Some((0.0, 717.0)), &[MAIN], MAIN, "right"),
            (0.0, 717.0)
        );
        assert_eq!(
            choose_position(Some((1200.0, 300.0)), &[MAIN], MAIN, "bottom"),
            (1200.0, 300.0)
        );
    }

    #[test]
    fn a_spot_on_a_screen_that_is_gone_falls_back_to_the_dock() {
        let gone = choose_position(Some((3000.0, 100.0)), &[MAIN], MAIN, "right");
        let (w, h) = TALL;
        assert_eq!(gone, (1536.0 - w, (864.0 - h) / 2.0));
        assert_eq!(
            choose_position(Some((100.0, 5000.0)), &[MAIN], MAIN, "right"),
            gone
        );
    }

    #[test]
    fn it_follows_to_another_screen_keeping_its_place_in_proportion() {
        let second = Screen {
            x: 1536.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        let (w, h) = TALL;
        // docked on the right of the main screen -> docked on the right of the second
        assert_eq!(
            follow_position(None, &[MAIN, second], MAIN, second, "right", 1.0),
            (1536.0 + 1920.0 - w, (1080.0 - h) / 2.0)
        );
        // dragged to the top left corner -> the top left corner of the other screen
        assert_eq!(
            follow_position(
                Some((0.0, 0.0)),
                &[MAIN, second],
                MAIN,
                second,
                "right",
                1.0
            ),
            (1536.0, 0.0)
        );
        // dragged to the bottom right -> the bottom right, not squeezed by a smaller screen's height
        let small = Screen {
            x: -1366.0,
            y: 0.0,
            width: 1366.0,
            height: 768.0,
        };
        let k = size_factor(small, MAIN);
        assert!(k < 1.0 && k >= 0.7);
        let (x, y) = follow_position(
            Some((1536.0 - w, 864.0 - h)),
            &[MAIN, small],
            MAIN,
            small,
            "right",
            k,
        );
        assert!((x - (0.0 - w * k)).abs() < 1e-6 && (y - (768.0 - h * k)).abs() < 1e-6);
    }

    #[test]
    fn a_taller_screen_does_not_make_it_bigger() {
        let tall = Screen {
            x: 0.0,
            y: 0.0,
            width: 2560.0,
            height: 1440.0,
        };
        assert_eq!(size_factor(tall, MAIN), 1.0);
    }

    #[test]
    fn a_spot_on_a_second_screen_is_kept() {
        let second = Screen {
            x: 1536.0,
            y: 0.0,
            width: 1920.0,
            height: 1080.0,
        };
        assert_eq!(
            choose_position(Some((2000.0, 400.0)), &[MAIN, second], MAIN, "right"),
            (2000.0, 400.0)
        );
    }
}
