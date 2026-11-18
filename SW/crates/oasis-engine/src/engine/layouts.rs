//! Spoken layouts (docs/APP_WINDOWS.md W9): save and restore the open windows as a Quick Launch setup.

use super::*;

// ---- spoken layouts (docs/APP_WINDOWS.md W9) -----------------------------------------------------

/// The Quick Launch setup a layout is kept as: "work" is "work layout"; nothing said is "my layout".
pub(super) fn layout_name(spoken: &str) -> String {
    let name = spoken.trim().to_lowercase();
    let name = if name.is_empty() { "my layout".to_string() } else if name.ends_with("layout") { name } else { format!("{name} layout") };
    name.chars().take(40).collect()
}

/// "save this layout [as work]" records every open app window, its screen and its place, as a Quick
/// Launch setup. "restore [work] layout" runs that setup: apps not running are started, then placed.
/// `value` is `save|name` or `restore|name`.
pub(super) fn run_layout(shared: &Shared, value: &str, restore: &mut dyn FnMut(&str) -> bool) -> Result<(), String> {
    let (verb, spoken) = value.split_once('|').unwrap_or((value, ""));
    let name = layout_name(spoken);
    match verb {
        "save" => {
            #[cfg(windows)]
            let mut items = crate::defaults::windows::capture_items();
            #[cfg(not(windows))]
            let mut items: Vec<oasis_settings::quick_launch::ItemEdit> = Vec::new();
            if items.is_empty() {
                return Err("There are no windows to save.".into());
            }
            items.truncate(40);
            let count = items.len();
            let id = oasis_settings::quick_launch::find(&shared.settings(), &name).map(|s| s.id).unwrap_or_default();
            let edit = oasis_settings::quick_launch::SetupEdit { id, name: name.clone(), call_name: String::new(), start_with_oasis: false, items };
            shared.change(|s| s.save_setup(edit).map_err(EditError::Invalid)).map_err(|e| e.to_string())?;
            shared.emit(EngineEvent::Note { text: format!("Saved {count} windows as “{name}”. Say “restore {name}” to put them back.") });
            Ok(())
        }
        "restore" => {
            let found = oasis_settings::quick_launch::find(&shared.settings(), &name);
            let Some(setup) = found else {
                return Err(format!("There is no saved layout called “{name}”. Say “save my layout” first."));
            };
            if restore(&setup.id) { Ok(()) } else { Err(format!("Could not restore “{name}”.")) }
        }
        other => Err(format!("I do not know the layout command '{other}'.")),
    }
}

#[cfg(test)]
mod layout_tests {
    use super::layout_name;

    #[test]
    fn a_layout_is_kept_under_its_spoken_name() {
        assert_eq!(layout_name(""), "my layout");
        assert_eq!(layout_name("Work"), "work layout");
        assert_eq!(layout_name("work layout"), "work layout");
        assert_eq!(layout_name(&"x".repeat(60)).chars().count(), 40);
    }
}
