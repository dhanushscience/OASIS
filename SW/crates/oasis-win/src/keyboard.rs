//! A listener for every key press and release on the computer (a low-level keyboard hook), so a
//! keyboard shortcut can stand in for a button on the pad.
//!
//! It sees all typing, so it is built to know nothing: it passes each key to one handler, keeps
//! nothing, and exists only while the person has the keyboard input switched on. Keys OASIS itself sends
//! (stamped with `input::OASIS_KEY_TAG`) are never reported, so it cannot trigger itself; keys from other
//! programs, such as a remapping tool, are reported like real ones.
//!
//! Windows removes a hook that is slow, so the handler must return at once.

#![allow(unsafe_code)]

use std::io;
use std::sync::mpsc;
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION,
    KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN,
    WM_SYSKEYUP,
};

/// Called with the virtual-key code and whether the key went down. Returns `true` to keep the key
/// from the program in front.
pub type Handler = Box<dyn FnMut(u32, bool) -> bool + Send>;

/// There is only ever one listener: the hook procedure has no place to keep anything else.
static HANDLER: Mutex<Option<Handler>> = Mutex::new(None);

/// Which of Ctrl, Shift, Alt and Win are down right now, as Windows has it: (ctrl, shift, alt, win).
pub fn modifiers_held() -> (bool, bool, bool, bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT};
    // SAFETY: GetAsyncKeyState only reads the key state for a virtual-key code.
    let down = |vk: u16| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0; // the high bit is "down now"
    (down(VK_CONTROL), down(VK_SHIFT), down(VK_MENU), down(VK_LWIN) || down(VK_RWIN))
}

/// Whether a key or mouse button (a virtual-key code: 0x01 is the left mouse button) is down right now.
pub fn is_down(vk: u16) -> bool {
    // SAFETY: GetAsyncKeyState only reads the key state for a virtual-key code.
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(i32::from(vk)) < 0 }
}

/// A key press or release that looks like it came from another program (no OASIS stamp), for tests of
/// the listener here and in the crates above. Panics if Windows refuses it.
#[doc(hidden)]
pub fn send_foreign_key(vk: u16, release: bool) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP};
    let ki = KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if release { KEYEVENTF_KEYUP } else { 0 }, time: 0, dwExtraInfo: 0 };
    let input = INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } };
    // SAFETY: one valid INPUT and its size, as the API requires.
    let sent = unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
    assert_eq!(sent, 1, "SendInput refused the key");
}

pub struct KeyboardHook {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl KeyboardHook {
    /// Start listening. `Err` if Windows refuses the hook, or one is already running.
    pub fn start(handler: Handler) -> io::Result<KeyboardHook> {
        {
            let mut slot = HANDLER.lock().map_err(|_| io::Error::other("the key listener is unavailable"))?;
            if slot.is_some() {
                return Err(io::Error::other("the key listener is already running"));
            }
            *slot = Some(handler);
        }
        let (ready_tx, ready_rx) = mpsc::channel::<io::Result<u32>>();
        let thread = thread::Builder::new().name("oasis-key-listener".into()).spawn(move || run(ready_tx))?;
        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(KeyboardHook { thread_id, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(io::Error::other("the key listener did not start")),
        }
    }
}

impl Drop for KeyboardHook {
    fn drop(&mut self) {
        // SAFETY: plain Win32 call that asks the listener thread's message loop to end. The thread id
        // is the one that thread reported; if the thread is already gone the call just fails.
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        if let Ok(mut slot) = HANDLER.lock() {
            *slot = None;
        }
    }
}

/// The listener thread: put the hook on, report that it is on, and wait for messages (a hook only
/// works while its thread pumps them) until asked to stop.
fn run(ready: mpsc::Sender<io::Result<u32>>) {
    // SAFETY: GetModuleHandleW(null) is the handle of this program; the hook procedure is a plain
    // `extern "system"` function in it that outlives the hook. The hook is removed below, on the same
    // thread that set it. The message loop only reads the MSG it is given.
    unsafe {
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(procedure), GetModuleHandleW(std::ptr::null()), 0);
        if hook.is_null() {
            let _ = ready.send(Err(io::Error::last_os_error()));
            return;
        }
        let _ = ready.send(Ok(GetCurrentThreadId()));
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {}
        UnhookWindowsHookEx(hook);
    }
}

unsafe extern "system" fn procedure(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION on a WH_KEYBOARD_LL hook, `lparam` points to a KBDLLHOOKSTRUCT that
        // is valid for the duration of this call.
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        if info.dwExtraInfo != crate::input::OASIS_KEY_TAG {
            let message = wparam as u32;
            let down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
            let up = message == WM_KEYUP || message == WM_SYSKEYUP;
            if down || up {
                // try_lock: if the handler is busy (or being replaced) the key simply passes.
                if let Ok(mut slot) = HANDLER.try_lock() {
                    if let Some(handler) = slot.as_mut() {
                        if handler(info.vkCode, down) {
                            return 1;
                        }
                    }
                }
            }
        }
    }
    // SAFETY: passing the same arguments on to the next hook, as the API requires.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{send_key, KeyEvent};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    const F24: u16 = 0x87; // no real keyboard has it, so the test cannot disturb anyone typing

    /// Needs an interactive desktop, so it is run on purpose: `cargo test -p oasis-win keyboard -- --ignored`.
    #[test]
    #[ignore = "needs an interactive Windows desktop"]
    fn the_listener_hears_other_programs_keys_but_never_its_own() {
        let seen = Arc::new(Mutex::new(Vec::<(u32, bool)>::new()));
        let log = Arc::clone(&seen);
        let hook = KeyboardHook::start(Box::new(move |vk, down| {
            log.lock().unwrap().push((vk, down));
            vk == u32::from(F24) // kept from the program in front
        }))
        .expect("the hook starts");
        let wait = |count: usize| {
            let end = Instant::now() + Duration::from_secs(3);
            while seen.lock().unwrap().len() < count && Instant::now() < end {
                thread::sleep(Duration::from_millis(10));
            }
        };

        send_foreign_key(F24, false);
        send_foreign_key(F24, true);
        wait(2);
        assert_eq!(*seen.lock().unwrap(), vec![(u32::from(F24), true), (u32::from(F24), false)]);

        // OASIS's own keys carry its stamp and never reach the handler.
        seen.lock().unwrap().clear();
        send_key(KeyEvent { vk: F24, release: false, scancode_only: false, extended: false }).unwrap();
        send_key(KeyEvent { vk: F24, release: true, scancode_only: false, extended: false }).unwrap();
        thread::sleep(Duration::from_millis(400));
        assert!(seen.lock().unwrap().is_empty(), "OASIS heard itself: {:?}", seen.lock().unwrap());

        drop(hook);
        // and once stopped, a second listener can start again
        drop(KeyboardHook::start(Box::new(|_, _| false)).expect("it can be started again"));
    }
}
