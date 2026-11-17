//! Reading what has the keyboard focus inside a program's accessibility tree, for panel
//! detection. Ports of `Services/Context/UiFocusProbe.cs` (the focus and its ancestors) and the
//! tree scan of `WorkbenchRegionProbe.cs`. Which panel that means is decided in
//! `oasis_core::matching::panel`; this only reads.
//!
//! Electron IDEs build their accessibility tree lazily and deeply, so every walk has a deadline
//! and a size cap: a slow window gives a partial answer, never a stuck pad.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{
    IUIAutomation, IUIAutomationElement, TreeScope_Children, UIA_CONTROLTYPE_ID,
};

use crate::window::{window_process_id, WindowId};

/// One control: `depth` 0 is the focus, 1 its parent, and so on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawNode {
    pub depth: usize,
    pub name: String,
    /// `ControlType.Edit`, `ControlType.Pane`, …
    pub control_type: String,
    pub automation_id: String,
    pub class_name: String,
}

/// A control met while scanning a whole window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RawScanNode {
    pub depth: usize,
    pub name: String,
    pub class_name: String,
    pub automation_id: String,
    pub focus_within: bool,
}

const MAX_ANCESTORS: usize = 16;
const MAX_MEMBERSHIP_DEPTH: usize = 40;
const SIGNATURE_DEPTH: usize = 14;
const MAX_SCAN_NODES: usize = 4000;
/// A walk gives up after this long and returns what it has.
const WALK_BUDGET: Duration = Duration::from_millis(1200);

/// The programmatic name of a control-type id.
pub fn control_type_name(id: i32) -> &'static str {
    const NAMES: [&str; 39] = [
        "Button", "Calendar", "CheckBox", "ComboBox", "Edit", "Hyperlink", "Image", "ListItem", "List", "Menu",
        "MenuBar", "MenuItem", "ProgressBar", "RadioButton", "ScrollBar", "Slider", "Spinner", "StatusBar", "Tab",
        "TabItem", "Text", "ToolBar", "ToolTip", "Tree", "TreeItem", "Custom", "Group", "Thumb", "DataGrid",
        "DataItem", "Document", "SplitButton", "Window", "Pane", "Header", "HeaderItem", "Table", "TitleBar",
        "Separator",
    ];
    match id.checked_sub(50000).and_then(|i| usize::try_from(i).ok()).and_then(|i| NAMES.get(i)) {
        Some(name) => name,
        None => "Unknown",
    }
}

#[allow(unsafe_code)]
fn read(element: &IUIAutomationElement, depth: usize) -> RawNode {
    // SAFETY: property reads on a live element; any failure reads as empty text.
    unsafe {
        RawNode {
            depth,
            name: element.CurrentName().map(|s| s.to_string()).unwrap_or_default(),
            control_type: element
                .CurrentControlType()
                .map(|t: UIA_CONTROLTYPE_ID| format!("ControlType.{}", control_type_name(t.0)))
                .unwrap_or_default(),
            automation_id: element.CurrentAutomationId().map(|s| s.to_string()).unwrap_or_default(),
            class_name: element.CurrentClassName().map(|s| s.to_string()).unwrap_or_default(),
        }
    }
}

#[allow(unsafe_code)]
fn class_of(element: &IUIAutomationElement) -> String {
    // SAFETY: as in `read`.
    unsafe { element.CurrentClassName().map(|s| s.to_string()).unwrap_or_default() }
}

/// The browser-engine shell around a page: a focus there tells nothing about the page.
fn is_shell_chrome(class: &str) -> bool {
    let lower = class.to_lowercase();
    lower.contains("chrome_widgetwin")
        || lower.contains("chrome_renderwidgethosthwnd")
        || ["rootview", "clientview", "nonclientview", "winframeview"].contains(&lower.as_str())
}

/// The focused control and its ancestors, nearest first. Empty if nothing can be read.
#[allow(unsafe_code)]
pub fn focus_chain(ui: &IUIAutomation, window: WindowId) -> Vec<RawNode> {
    let deadline = Instant::now() + WALK_BUDGET;
    // SAFETY: COM calls on this worker thread; the window handle is only a handle.
    unsafe {
        let root = ui.ElementFromHandle(HWND(window.0 as *mut _)).ok();
        let focused = ui.GetFocusedElement().ok();
        if let (Some(focused), Some(root)) = (&focused, &root) {
            if is_under_window(ui, focused, root, window) && !is_shell_chrome(&class_of(focused)) {
                return chain_from(ui, focused);
            }
        }
        if let Some(root) = &root {
            if let Some(found) = find_best_descendant(root, deadline) {
                return chain_from(ui, &found);
            }
        }
        focused.map(|f| chain_from(ui, &f)).unwrap_or_default()
    }
}

#[allow(unsafe_code)]
unsafe fn chain_from(ui: &IUIAutomation, start: &IUIAutomationElement) -> Vec<RawNode> {
    // SAFETY: live elements and a walker from this thread; a failed step ends the chain.
    unsafe {
        let mut chain = Vec::new();
        let Ok(walker) = ui.RawViewWalker() else { return vec![read(start, 0)] };
        let mut current = start.clone();
        for depth in 0..MAX_ANCESTORS {
            chain.push(read(&current, depth));
            match walker.GetParentElement(&current) {
                Ok(parent) => current = parent,
                Err(_) => break,
            }
        }
        chain
    }
}

/// Whether `element` lives under the window's own tree. Compared by element identity, not by
/// window handle: Electron focus nodes have no handle for 20 levels.
#[allow(unsafe_code)]
unsafe fn is_under_window(ui: &IUIAutomation, element: &IUIAutomationElement, root: &IUIAutomationElement, window: WindowId) -> bool {
    // SAFETY: live elements and walker; failures read as "not under".
    unsafe {
        let Ok(walker) = ui.RawViewWalker() else { return false };
        let target_pid = window_process_id(window);
        let mut current = element.clone();
        for _ in 0..MAX_MEMBERSHIP_DEPTH {
            if ui.CompareElements(&current, root).map(|b| b.as_bool()).unwrap_or(false) {
                return true;
            }
            if let Ok(handle) = current.CurrentNativeWindowHandle() {
                if !handle.0.is_null() {
                    if handle.0 as isize == window.0 {
                        return true;
                    }
                    if target_pid.is_some() && window_process_id(WindowId(handle.0 as isize)) == target_pid {
                        return true;
                    }
                }
            }
            match walker.GetParentElement(&current) {
                Ok(parent) => current = parent,
                Err(_) => return false,
            }
        }
        false
    }
}

#[allow(unsafe_code)]
fn children(ui_root: &IUIAutomationElement, condition: &windows::Win32::UI::Accessibility::IUIAutomationCondition) -> Vec<IUIAutomationElement> {
    // SAFETY: a live element; failures give no children.
    unsafe {
        let Ok(found) = ui_root.FindAll(TreeScope_Children, condition) else { return Vec::new() };
        let count = found.Length().unwrap_or(0);
        (0..count).filter_map(|i| found.GetElement(i).ok()).collect()
    }
}

#[allow(unsafe_code)]
fn has_keyboard_focus(element: &IUIAutomationElement) -> bool {
    // SAFETY: as in `read`.
    unsafe { element.CurrentHasKeyboardFocus().map(|b| b.as_bool()).unwrap_or(false) }
}

fn find_best_descendant(root: &IUIAutomationElement, deadline: Instant) -> Option<IUIAutomationElement> {
    let condition = true_condition()?;
    find_focused(root, &condition, deadline).or_else(|| find_signature(root, &condition, deadline))
}

#[allow(unsafe_code)]
fn true_condition() -> Option<windows::Win32::UI::Accessibility::IUIAutomationCondition> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::UI::Accessibility::CUIAutomation;
    // SAFETY: plain COM creation on a thread already initialised for COM.
    unsafe {
        let ui: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        ui.CreateTrueCondition().ok()
    }
}

fn find_focused(
    element: &IUIAutomationElement,
    condition: &windows::Win32::UI::Accessibility::IUIAutomationCondition,
    deadline: Instant,
) -> Option<IUIAutomationElement> {
    if Instant::now() > deadline {
        return None;
    }
    if has_keyboard_focus(element) {
        return Some(element.clone());
    }
    children(element, condition).iter().find_map(|child| find_focused(child, condition, deadline))
}

/// Class and name signatures only some IDE panels carry (the terminal's hidden text box, the agent
/// panel). Used when nothing reports the focus.
fn signature_priority(class: &str, name: &str) -> i32 {
    let (class, name) = (class.to_lowercase(), name.to_lowercase());
    if class.contains("xterm-helper-textarea") {
        40
    } else if class.contains("antigravity-agent-side-panel") {
        39
    } else if name == "message input" {
        38
    } else if name.contains("agent conversation") {
        37
    } else if class.contains("aislash-editor-input") {
        36
    } else if class.contains("xterm-helper") || class.contains("terminal xterm") {
        34
    } else if class.contains("composer-messages") || class.contains("composer-input") {
        33
    } else {
        0
    }
}

fn find_signature(
    root: &IUIAutomationElement,
    condition: &windows::Win32::UI::Accessibility::IUIAutomationCondition,
    deadline: Instant,
) -> Option<IUIAutomationElement> {
    struct Hit {
        element: IUIAutomationElement,
        priority: i32,
        depth: usize,
        focus: bool,
    }
    fn collect(
        element: &IUIAutomationElement,
        condition: &windows::Win32::UI::Accessibility::IUIAutomationCondition,
        depth: usize,
        deadline: Instant,
        hits: &mut Vec<Hit>,
    ) {
        if depth > SIGNATURE_DEPTH || Instant::now() > deadline {
            return;
        }
        let node = read(element, depth);
        let priority = signature_priority(&node.class_name, &node.name);
        if priority > 0 {
            hits.push(Hit { element: element.clone(), priority, depth, focus: has_keyboard_focus(element) });
        }
        for child in children(element, condition) {
            collect(&child, condition, depth + 1, deadline, hits);
        }
    }
    let mut hits = Vec::new();
    collect(root, condition, 0, deadline, &mut hits);
    hits.into_iter().max_by_key(|h| (h.focus, h.priority, h.depth)).map(|h| h.element)
}

/// Every control in the window (down to a depth and a count), with whether the keyboard focus is
/// at or just below it. For telling IDE panels apart by their names and classes.
pub fn scan_window(ui: &IUIAutomation, window: WindowId) -> Vec<RawScanNode> {
    let deadline = Instant::now() + WALK_BUDGET;
    #[allow(unsafe_code)]
    // SAFETY: a handle is only a handle; failure gives an empty scan.
    let Some(root) = (unsafe { ui.ElementFromHandle(HWND(window.0 as *mut _)).ok() }) else { return Vec::new() };
    let Some(condition) = true_condition() else { return Vec::new() };
    let mut out = Vec::new();
    scan(&root, &condition, 0, deadline, &mut out);
    out
}

fn scan(
    element: &IUIAutomationElement,
    condition: &windows::Win32::UI::Accessibility::IUIAutomationCondition,
    depth: usize,
    deadline: Instant,
    out: &mut Vec<RawScanNode>,
) {
    if depth > SIGNATURE_DEPTH || out.len() >= MAX_SCAN_NODES || Instant::now() > deadline {
        return;
    }
    let node = read(element, depth);
    out.push(RawScanNode {
        depth,
        name: node.name,
        class_name: node.class_name,
        automation_id: node.automation_id,
        focus_within: focus_within(element, condition, 0, deadline),
    });
    for child in children(element, condition) {
        scan(&child, condition, depth + 1, deadline, out);
    }
}

fn focus_within(
    element: &IUIAutomationElement,
    condition: &windows::Win32::UI::Accessibility::IUIAutomationCondition,
    depth: usize,
    deadline: Instant,
) -> bool {
    if depth > 6 || Instant::now() > deadline {
        return false;
    }
    has_keyboard_focus(element)
        || children(element, condition).iter().any(|child| focus_within(child, condition, depth + 1, deadline))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_type_ids_have_their_programmatic_names() {
        assert_eq!(control_type_name(50004), "Edit");
        assert_eq!(control_type_name(50030), "Document");
        assert_eq!(control_type_name(50033), "Pane");
        assert_eq!(control_type_name(50024), "TreeItem");
        assert_eq!(control_type_name(1), "Unknown");
        assert_eq!(control_type_name(-5), "Unknown");
    }

    #[test]
    fn the_shell_around_a_page_is_recognised() {
        for class in ["Chrome_WidgetWin_1", "Chrome_RenderWidgetHostHWND", "RootView", "ClientView", "WinFrameView"] {
            assert!(is_shell_chrome(class), "{class}");
        }
        assert!(!is_shell_chrome("xterm-helper-textarea"));
        assert!(!is_shell_chrome(""));
    }

    #[test]
    fn signatures_rank_the_terminal_text_box_first() {
        assert!(signature_priority("xterm-helper-textarea", "") > signature_priority("composer-input", ""));
        assert_eq!(signature_priority("monaco-editor", "x"), 0);
        assert_eq!(signature_priority("", "Message input"), 38);
    }
}
