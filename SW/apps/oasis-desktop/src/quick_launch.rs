//! Quick Launch: the setups, saving and deleting them, the screens, capturing the open windows, running one.

use super::*;

// ---- Quick Launch ----------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy)]
pub(crate) struct RegionDto {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

/// One thing a setup opens, as the page edits it.
#[derive(Serialize, Deserialize)]
pub(crate) struct QuickItem {
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
pub(crate) struct QuickSetup {
    id: String,
    name: String,
    /// What to say besides the name, as typed (commas between several).
    call_name: String,
    start_with_oasis: bool,
    items: Vec<QuickItem>,
}

#[derive(Deserialize)]
pub(crate) struct QuickSetupInput {
    id: String,
    name: String,
    call_name: String,
    start_with_oasis: bool,
    items: Vec<QuickItem>,
}

pub(crate) fn layout_number(layout: oasis_core::placement::Layout) -> i64 {
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
pub fn quick_setups(slot: State<'_, EngineSlot>) -> Result<Vec<QuickSetup>, String> {
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
pub fn save_quick_setup(slot: State<'_, EngineSlot>, setup: QuickSetupInput) -> Result<String, String> {
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
pub fn delete_quick_setup(slot: State<'_, EngineSlot>, id: String) -> Result<(), String> {
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
pub(crate) struct QuickScreen {
    spec: String,
    primary: bool,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
}

#[tauri::command]
pub fn quick_screens() -> Vec<QuickScreen> {
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
pub async fn capture_windows() -> Result<Vec<QuickItem>, String> {
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
pub async fn run_quick_setup(slot: State<'_, EngineSlot>, id: String) -> Result<String, String> {
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
