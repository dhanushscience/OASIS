//! `ForegroundApp` for Windows: the real window source behind the portable tracker.

use std::time::Duration;

use oasis_core::matching::address_bar::{is_address_bar_name, pick_address};
use oasis_core::matching::panel::{UiNode, UiSnapshot, WorkbenchNode};
use oasis_win::uia::FieldReader;
use oasis_win::window::{self, WindowId};

use crate::foreground::{ForegroundTracker, WindowSnapshot, WindowSource};

/// How long a pad press may wait for a browser to answer. A slow browser costs one missed
/// address, never a stuck pad.
const ADDRESS_TIMEOUT: Duration = Duration::from_millis(1500);
/// How long a press may wait for the focused control of an IDE. A slow IDE costs the panel (the
/// main set of buttons is used), never a stuck pad.
const PANEL_TIMEOUT: Duration = Duration::from_millis(900);
const SCAN_TIMEOUT: Duration = Duration::from_millis(1400);

#[derive(Default)]
pub struct Win32Windows {
    /// Started on the first browser read, so an app with no website profiles never starts it.
    address_reader: Option<FieldReader>,
}

impl WindowSource for Win32Windows {
    fn foreground(&mut self) -> Option<WindowSnapshot> {
        let id = window::foreground_window()?;
        let pid = window::window_process_id(id);
        Some(WindowSnapshot {
            id: id.0 as u64,
            title: window::window_title(id),
            // Process exited or access denied: an empty name, as C# did.
            process: pid.and_then(window::process_name).unwrap_or_default(),
            pid,
        })
    }

    /// The browser's address bar, read through UI Automation. Empty when it cannot be read: the
    /// browser is slow, is still building its accessibility tree (the first read of a Chromium
    /// browser often comes back empty), or the bar is hidden (fullscreen video).
    fn browser_url(&mut self, window_id: u64) -> String {
        let reader = self.address_reader.get_or_insert_with(FieldReader::new);
        let fields = reader.edit_fields(WindowId(window_id as isize), is_address_bar_name, ADDRESS_TIMEOUT);
        pick_address(fields.iter().map(|(name, value)| (name.as_str(), value.as_str())))
            .unwrap_or_default()
    }

    /// The focused control and its ancestors, read through UI Automation.
    fn ui_snapshot(&mut self, window_id: u64) -> UiSnapshot {
        let chain = self.reader().focus_chain(WindowId(window_id as isize), PANEL_TIMEOUT);
        let nodes: Vec<UiNode> = chain
            .into_iter()
            .map(|n| UiNode {
                depth: n.depth,
                name: n.name,
                control_type: n.control_type,
                automation_id: n.automation_id,
                class_name: n.class_name,
            })
            .collect();
        UiSnapshot { focus: nodes.first().cloned().unwrap_or_default(), ancestors: nodes }
    }

    /// A scan of the window's controls, for telling IDE panels apart by name and class.
    fn workbench_nodes(&mut self, window_id: u64) -> Vec<WorkbenchNode> {
        self.reader()
            .scan_window(WindowId(window_id as isize), SCAN_TIMEOUT)
            .into_iter()
            .map(|n| WorkbenchNode {
                depth: n.depth,
                name: n.name,
                class_name: n.class_name,
                automation_id: n.automation_id,
                has_focus_within: n.focus_within,
            })
            .collect()
    }
}

impl Win32Windows {
    fn reader(&mut self) -> &FieldReader {
        self.address_reader.get_or_insert_with(FieldReader::new)
    }
}

pub fn foreground_app() -> ForegroundTracker<Win32Windows> {
    ForegroundTracker::new(Win32Windows::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::ForegroundApp;

    #[test]
    fn the_real_foreground_window_has_a_process_when_there_is_one() {
        // Headless sessions have no foreground window; then the context is simply empty.
        let info = foreground_app().current().unwrap();
        if info.pid.is_some() {
            assert!(!info.process.is_empty());
            assert_eq!(info.process, info.process.to_lowercase());
        }
    }

    #[test]
    fn a_missing_window_has_no_address() {
        assert_eq!(Win32Windows::default().browser_url(0x7fff_fff0), "");
    }
}
