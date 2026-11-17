//! Reading the text fields of a window through UI Automation. Port of the Win32 half of
//! `ContextTracker.GetBrowserUrl`: find the window's edit controls and read the value of the ones
//! the caller accepts by name. Which one is the address bar is decided in
//! `oasis_core::matching::address_bar`.
//!
//! UI Automation can stall for seconds on a busy or hung browser, and a pad press must never wait
//! for that. All COM work happens on one worker thread; callers wait a limited time and get
//! nothing back if the browser is slow. (Chromium builds its accessibility tree on the first
//! request, so the first read of a browser may come back empty; a later one succeeds.)

use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use windows::core::Interface;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants,
    UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_ValuePatternId,
};

use crate::window::WindowId;

/// An edit field: accessible name and current text.
pub type Field = (String, String);

struct FieldsRequest {
    window: isize,
    accept_name: fn(&str) -> bool,
    reply: Sender<Vec<Field>>,
}

enum Request {
    Fields(FieldsRequest),
    /// Is keyboard focus in a text field of some other program?
    EditableFocus(Sender<bool>),
    /// The whole text of the field that has keyboard focus.
    FocusedText(Sender<Option<String>>),
    /// The focused control and its ancestors in a window (panel detection).
    FocusChain(isize, Sender<Vec<crate::uia_panel::RawNode>>),
    /// A scan of a whole window for telltale controls (panel detection fallback).
    Scan(isize, Sender<Vec<crate::uia_panel::RawScanNode>>),
}

/// A handle to the UI Automation worker thread. Cheap to clone; the thread ends when the last
/// handle is dropped.
#[derive(Clone)]
pub struct FieldReader {
    requests: Sender<Request>,
}

impl FieldReader {
    pub fn new() -> FieldReader {
        let (requests, inbox) = mpsc::channel::<Request>();
        thread::Builder::new()
            .name("oasis-uia".into())
            .spawn(move || {
                let mut automation: Option<IUIAutomation> = None;
                for request in inbox {
                    match request {
                        Request::Fields(request) => {
                            let fields = worker_read(&mut automation, &request).unwrap_or_default();
                            let _ = request.reply.send(fields);
                        }
                        Request::FocusChain(window, reply) => {
                            let chain = ensure_automation(&mut automation)
                                .map(|ui| crate::uia_panel::focus_chain(ui, WindowId(window)))
                                .unwrap_or_default();
                            let _ = reply.send(chain);
                        }
                        Request::Scan(window, reply) => {
                            let nodes = ensure_automation(&mut automation)
                                .map(|ui| crate::uia_panel::scan_window(ui, WindowId(window)))
                                .unwrap_or_default();
                            let _ = reply.send(nodes);
                        }
                        Request::EditableFocus(reply) => {
                            let _ = reply.send(worker_editable_focus(&mut automation).unwrap_or(false));
                        }
                        Request::FocusedText(reply) => {
                            let _ = reply.send(worker_focused_text(&mut automation).ok().flatten());
                        }
                    }
                }
            })
            .expect("a thread can be spawned");
        FieldReader { requests }
    }

    /// The edit fields of `window` whose names `accept_name` accepts, with their text. Empty if
    /// the window has none, is gone, or does not answer within `timeout`.
    pub fn edit_fields(
        &self,
        window: WindowId,
        accept_name: fn(&str) -> bool,
        timeout: Duration,
    ) -> Vec<Field> {
        let (reply, answer) = mpsc::channel();
        let request = Request::Fields(FieldsRequest { window: window.0, accept_name, reply });
        if self.requests.send(request).is_err() {
            return Vec::new();
        }
        answer.recv_timeout(timeout).unwrap_or_default()
    }
}

impl Default for FieldReader {
    fn default() -> Self {
        FieldReader::new()
    }
}

#[allow(unsafe_code)]
fn worker_read(
    automation: &mut Option<IUIAutomation>,
    request: &FieldsRequest,
) -> windows::core::Result<Vec<Field>> {
    if automation.is_none() {
        // SAFETY: plain COM calls on this worker thread, which owns the apartment for its life.
        // A thread already in another mode (`RPC_E_CHANGED_MODE`) is fine to use as it is.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            *automation = Some(CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?);
        }
    }
    let ui = automation.as_ref().expect("created above");

    // SAFETY: `request.window` is just a handle; UI Automation fails cleanly for a stale one.
    let (root, edits) = unsafe {
        let root = ui.ElementFromHandle(HWND(request.window as *mut _))?;
        let edit = ui.CreatePropertyCondition(
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_EditControlTypeId.0),
        )?;
        let edits = root.FindAll(TreeScope_Descendants, &edit)?;
        (root, edits)
    };
    let _keep_alive = root;

    let mut fields = Vec::new();
    // SAFETY: `edits` is a live element array; indexes stay below its length.
    unsafe {
        for index in 0..edits.Length()? {
            let element = edits.GetElement(index)?;
            let name = element.CurrentName()?.to_string();
            if !(request.accept_name)(&name) {
                continue;
            }
            let Ok(pattern) = element.GetCurrentPattern(UIA_ValuePatternId) else { continue };
            let Ok(value) = pattern.cast::<IUIAutomationValuePattern>() else { continue };
            fields.push((name, value.CurrentValue()?.to_string()));
        }
    }
    Ok(fields)
}


impl FieldReader {
    /// Whether keyboard focus is on, or inside, an editable text field of another program (the
    /// target for dictation). Port of `Services/Voice/EditableFocusProbe.cs`. False if UI
    /// Automation does not answer within `timeout`: a slow or hung program must never delay a
    /// push-to-talk press.
    pub fn editable_focus(&self, timeout: Duration) -> bool {
        let (reply, answer) = mpsc::channel();
        if self.requests.send(Request::EditableFocus(reply)).is_err() {
            return false;
        }
        answer.recv_timeout(timeout).unwrap_or(false)
    }

    /// The whole text of the focused text field of another program, to see what the person changed
    /// by hand. `None` for a password box, a field that cannot be read, or no answer in `timeout`.
    pub fn focused_text(&self, timeout: Duration) -> Option<String> {
        let (reply, answer) = mpsc::channel();
        self.requests.send(Request::FocusedText(reply)).ok()?;
        answer.recv_timeout(timeout).ok().flatten()
    }

    /// The focused control and its ancestors in `window`, nearest first. Empty if it cannot be
    /// read within `timeout`.
    pub fn focus_chain(&self, window: WindowId, timeout: Duration) -> Vec<crate::uia_panel::RawNode> {
        let (reply, answer) = mpsc::channel();
        if self.requests.send(Request::FocusChain(window.0, reply)).is_err() {
            return Vec::new();
        }
        answer.recv_timeout(timeout).unwrap_or_default()
    }

    /// A scan of `window`'s controls (see `uia_panel::scan_window`).
    pub fn scan_window(&self, window: WindowId, timeout: Duration) -> Vec<crate::uia_panel::RawScanNode> {
        let (reply, answer) = mpsc::channel();
        if self.requests.send(Request::Scan(window.0, reply)).is_err() {
            return Vec::new();
        }
        answer.recv_timeout(timeout).unwrap_or_default()
    }
}

/// UI Automation control types that take typed text.
const EDIT: i32 = 50004;
const COMBO_BOX: i32 = 50003;
const DOCUMENT: i32 = 50030;
/// How far up the tree an editable ancestor is looked for.
const MAX_ANCESTOR_DEPTH: usize = 6;

#[allow(unsafe_code)]
fn ensure_automation(automation: &mut Option<IUIAutomation>) -> windows::core::Result<&IUIAutomation> {
    if automation.is_none() {
        // SAFETY: plain COM calls on this worker thread, which owns the apartment for its life.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            *automation = Some(CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?);
        }
    }
    Ok(automation.as_ref().expect("created above"))
}

#[allow(unsafe_code)]
fn worker_editable_focus(automation: &mut Option<IUIAutomation>) -> windows::core::Result<bool> {
    let ui = ensure_automation(automation)?;
    // SAFETY: element calls on live elements from this thread; any failure is a "no".
    unsafe {
        let focused = ui.GetFocusedElement()?;
        if belongs_to_this_program(ui, &focused) {
            return Ok(false);
        }
        if is_editable(&focused) {
            return Ok(true);
        }
        let walker = ui.RawViewWalker()?;
        let mut current = focused;
        for depth in 0..MAX_ANCESTOR_DEPTH {
            let Ok(parent) = walker.GetParentElement(&current) else { break };
            if depth > 0 && is_editable(&parent) {
                return Ok(true);
            }
            current = parent;
        }
    }
    Ok(false)
}

/// Longest field text read; a bigger document is skipped.
const MAX_FIELD_TEXT: i32 = 200_000;

#[allow(unsafe_code)]
fn worker_focused_text(automation: &mut Option<IUIAutomation>) -> windows::core::Result<Option<String>> {
    use windows::Win32::UI::Accessibility::{IUIAutomationTextPattern, UIA_TextPatternId};
    let ui = ensure_automation(automation)?;
    // SAFETY: element calls on live elements from this thread; any failure is "unreadable".
    unsafe {
        let focused = ui.GetFocusedElement()?;
        if belongs_to_this_program(ui, &focused) || focused.CurrentIsPassword().map(|b| b.as_bool()).unwrap_or(true) {
            return Ok(None);
        }
        // The focused control, or the nearest editable one above it (a caret inside a document).
        let walker = ui.RawViewWalker()?;
        let mut current = focused;
        for _ in 0..MAX_ANCESTOR_DEPTH {
            if let Ok(pattern) = current.GetCurrentPattern(UIA_TextPatternId) {
                if let Ok(text) = pattern.cast::<IUIAutomationTextPattern>() {
                    let range = text.DocumentRange()?;
                    let all = range.GetText(MAX_FIELD_TEXT + 1)?.to_string();
                    return Ok((all.len() <= MAX_FIELD_TEXT as usize).then_some(all));
                }
            }
            if let Ok(pattern) = current.GetCurrentPattern(UIA_ValuePatternId) {
                if let Ok(value) = pattern.cast::<IUIAutomationValuePattern>() {
                    return Ok(Some(value.CurrentValue()?.to_string()));
                }
            }
            let Ok(parent) = walker.GetParentElement(&current) else { break };
            current = parent;
        }
    }
    Ok(None)
}

/// Whether the element is a text input. Whole browser render hosts are never counted: only true
/// edit controls, documents that can take keys, and well-known editor classes.
#[allow(unsafe_code)]
unsafe fn is_editable(element: &windows::Win32::UI::Accessibility::IUIAutomationElement) -> bool {
    // SAFETY: the caller passes a live element; failures read as "not editable".
    unsafe {
        let can_type = element.CurrentIsKeyboardFocusable().map(|b| b.as_bool()).unwrap_or(false)
            || element.CurrentHasKeyboardFocus().map(|b| b.as_bool()).unwrap_or(false);
        if !can_type {
            return false;
        }
        let control = element.CurrentControlType().map(|t| t.0).unwrap_or(0);
        if matches!(control, EDIT | COMBO_BOX | DOCUMENT) {
            return true;
        }
        let class = element.CurrentClassName().map(|c| c.to_string().to_lowercase()).unwrap_or_default();
        class.contains("richedit") || class.contains("textbox") || class == "edit" || class.contains("scintilla") || class.contains("notepad")
    }
}

/// The focused element lives in this very program (its own window), so dictation would type
/// into OASIS itself.
#[allow(unsafe_code)]
unsafe fn belongs_to_this_program(
    ui: &IUIAutomation,
    element: &windows::Win32::UI::Accessibility::IUIAutomationElement,
) -> bool {
    // SAFETY: live element and walker from this thread; a failure reads as "not ours".
    unsafe {
        let Ok(walker) = ui.RawViewWalker() else { return false };
        let mut current = element.clone();
        for _ in 0..24 {
            if let Ok(handle) = current.CurrentNativeWindowHandle() {
                if !handle.0.is_null() {
                    return crate::window::window_process_id(WindowId(handle.0 as isize)) == Some(std::process::id());
                }
            }
            let Ok(parent) = walker.GetParentElement(&current) else { return false };
            current = parent;
        }
        false
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_that_does_not_exist_gives_no_fields_and_does_not_hang() {
        let reader = FieldReader::new();
        let started = std::time::Instant::now();
        let fields = reader.edit_fields(WindowId(0x7fff_fff0), |_| true, Duration::from_secs(3));
        assert!(fields.is_empty());
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn asking_about_the_focus_never_crashes_and_never_hangs() {
        let reader = FieldReader::new();
        let started = std::time::Instant::now();
        let _ = reader.editable_focus(Duration::from_secs(3));
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn the_reader_can_be_used_from_several_threads() {
        let reader = FieldReader::new();
        let handles: Vec<_> = (0..3)
            .map(|_| {
                let reader = reader.clone();
                thread::spawn(move || {
                    reader.edit_fields(WindowId(0x7fff_fff0), |_| true, Duration::from_secs(3))
                })
            })
            .collect();
        for handle in handles {
            assert!(handle.join().unwrap().is_empty());
        }
    }
}
