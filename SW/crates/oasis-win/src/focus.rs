//! Finding an application's window and bringing it to the front. Port of `TryActivateWindow`
//! and the by-process lookup in `Services/ActionExecutor.cs`.

use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindow, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
    SetForegroundWindow, ShowWindow, GW_OWNER, SW_RESTORE,
};

use crate::window::{foreground_window, process_name, window_process_id, window_title, WindowId};

/// How long to let Windows finish switching before checking it worked (C# waited 60 ms).
const SETTLE: std::time::Duration = std::time::Duration::from_millis(60);

#[allow(unsafe_code)]
unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Vec<isize>` passed by `top_level_windows` below, alive for
    // the whole `EnumWindows` call.
    let found = unsafe { &mut *(lparam as *mut Vec<isize>) };
    // SAFETY: plain calls on the handle Windows just gave us.
    if unsafe { IsWindowVisible(hwnd) != 0 || IsIconic(hwnd) != 0 } {
        found.push(hwnd as isize);
    }
    1 // keep going
}

/// Every visible or minimised top-level window.
#[allow(unsafe_code)]
fn top_level_windows() -> Vec<WindowId> {
    let mut found: Vec<isize> = Vec::new();
    // SAFETY: the callback only touches `found`, which outlives the call.
    unsafe { EnumWindows(Some(collect), &mut found as *mut Vec<isize> as LPARAM) };
    found.into_iter().map(WindowId).collect()
}

#[allow(unsafe_code)]
fn has_owner(window: WindowId) -> bool {
    // SAFETY: a stale handle returns null, it does not fault.
    !unsafe { GetWindow(window.hwnd(), GW_OWNER) }.is_null()
}

#[allow(unsafe_code)]
pub fn is_window(window: WindowId) -> bool {
    // SAFETY: plain call.
    unsafe { IsWindow(window.hwnd()) != 0 }
}

#[allow(unsafe_code)]
fn is_iconic(window: WindowId) -> bool {
    // SAFETY: plain call.
    unsafe { IsIconic(window.hwnd()) != 0 }
}

/// The main windows (visible or minimised, untitled and owned popups excluded) of every process
/// whose name is `process`, without extension, compared ignoring case. Like .NET's
/// `Process.MainWindowHandle`, one per window the user would call "the app".
pub fn main_windows_of(process: &str) -> Vec<WindowId> {
    main_windows_where(|name| name.eq_ignore_ascii_case(process))
}

/// Same, for every process whose name `is_it` accepts. Front to back (the window used last first).
pub fn main_windows_where(is_it: impl Fn(&str) -> bool) -> Vec<WindowId> {
    top_level_windows()
        .into_iter()
        .filter(|&w| !has_owner(w) && !window_title(w).trim().is_empty())
        .filter(|&w| window_process_id(w).and_then(process_name).is_some_and(|name| is_it(&name)))
        .collect()
}

/// Bring `window` to the front, restoring it first if minimised. `true` if it ended up in
/// front.
///
/// Windows refuses to let a background process take focus, so, as in C#, this attaches to the
/// input queues of the current foreground window and the target while asking, and always
/// detaches again: a failed detach can leave later keystrokes going nowhere.
#[allow(unsafe_code)]
pub fn activate(window: WindowId) -> bool {
    if !is_window(window) {
        return false;
    }
    // Only restore a minimised window; showing a maximised one again can demote it.
    if is_iconic(window) {
        // SAFETY: plain call.
        unsafe { ShowWindow(window.hwnd(), SW_RESTORE) };
    }
    let current = foreground_window();
    if current == Some(window) {
        return true;
    }

    // SAFETY: plain calls; a null or stale handle yields thread id 0 and the attach below
    // simply fails.
    let (foreground_thread, target_thread, this_thread) = unsafe {
        (
            current.map_or(0, |w| GetWindowThreadProcessId(w.hwnd(), std::ptr::null_mut())),
            GetWindowThreadProcessId(window.hwnd(), std::ptr::null_mut()),
            GetCurrentThreadId(),
        )
    };
    // SAFETY: attach and detach are paired below whatever happens in between.
    unsafe {
        let attached_fg = AttachThreadInput(this_thread, foreground_thread, 1) != 0;
        let attached_target = AttachThreadInput(this_thread, target_thread, 1) != 0;
        SetForegroundWindow(window.hwnd());
        if attached_target {
            AttachThreadInput(this_thread, target_thread, 0);
        }
        if attached_fg {
            AttachThreadInput(this_thread, foreground_thread, 0);
        }
    }
    // Done as soon as Windows says so, not after a fixed 60 ms (APP_WINDOWS.md W5); the 60 ms is the limit.
    wait_until_front(window, SETTLE)
}

/// Whether `window` becomes the window in front within `limit`; checked every few milliseconds.
// shortcut: a 5 ms poll, not a SetWinEventHook(EVENT_SYSTEM_FOREGROUND) hook; a hook needs its own
// message loop thread, worth it only if this ever shows up in the launch timing.
pub fn wait_until_front(window: WindowId, limit: std::time::Duration) -> bool {
    let started = std::time::Instant::now();
    loop {
        if foreground_window() == Some(window) {
            return true;
        }
        if started.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Wait until the program owning `window` is ready for keys: Windows' "input idle" (its message loop
/// waits for input), at most `limit`. Returns at once for a program that is already running idle.
/// When Windows cannot tell (no access to the process), a quarter of `limit` is waited instead.
#[allow(unsafe_code)]
pub fn wait_input_idle(window: WindowId, limit: std::time::Duration) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForInputIdle, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE};
    const WAIT_FAILED: u32 = 0xFFFF_FFFF;
    let Some(pid) = window_process_id(window) else { return };
    let millis = u32::try_from(limit.as_millis()).unwrap_or(u32::MAX);
    // SAFETY: plain calls; the handle is checked for null and closed once.
    let waited = unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            false
        } else {
            let result = WaitForInputIdle(process, millis);
            CloseHandle(process);
            result != WAIT_FAILED
        }
    };
    if !waited {
        std::thread::sleep(limit / 4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_nobody_runs_has_no_windows() {
        assert!(main_windows_of("oasis-no-such-program").is_empty());
    }

    #[test]
    fn our_own_test_process_has_no_window() {
        assert!(main_windows_of("oasis_win").is_empty());
    }

    #[test]
    fn waiting_for_a_window_ends_at_its_limit_and_idle_returns_for_a_gone_one() {
        let started = std::time::Instant::now();
        assert!(!wait_until_front(WindowId(0x7fff_fff0), std::time::Duration::from_millis(30)));
        assert!(started.elapsed() < std::time::Duration::from_millis(200));
        wait_input_idle(WindowId(0x7fff_fff0), std::time::Duration::from_millis(400)); // no process: returns at once
    }

    #[test]
    fn a_gone_window_cannot_be_activated() {
        assert!(!is_window(WindowId(0x7fff_fff0)));
        assert!(!activate(WindowId(0x7fff_fff0)));
    }

    #[test]
    fn enumeration_returns_only_real_windows() {
        for w in top_level_windows() {
            assert!(is_window(w));
        }
    }
}
