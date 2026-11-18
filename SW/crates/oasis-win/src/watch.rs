//! Tells a caller when something in a folder was written, so it does not have to poll the files.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::thread::{self, JoinHandle};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
};
use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent, WaitForMultipleObjects, INFINITE};

/// A saving program often writes several times in a row: wait for this much quiet before telling.
const QUIET: u32 = 150;

/// Calls `on_change` after files in `folder` (not its subfolders) were written, renamed, added or
/// removed. Dropping it stops the thread and waits for it, so nothing outlives the owner.
pub struct FolderWatch {
    stop: usize,
    thread: Option<JoinHandle<()>>,
}

impl FolderWatch {
    /// Fails (the caller should fall back to checking now and then) when the folder cannot be watched.
    #[allow(unsafe_code)]
    pub fn start(folder: &Path, on_change: impl Fn() + Send + 'static) -> io::Result<FolderWatch> {
        let wide: Vec<u16> = folder.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: `wide` is a NUL-terminated path that outlives the call.
        let changes = unsafe {
            FindFirstChangeNotificationW(wide.as_ptr(), 0, FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME)
        };
        if changes == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: no security attributes or name; a manual-reset event that starts unset.
        let stop = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if stop.is_null() {
            let error = io::Error::last_os_error();
            // SAFETY: `changes` was just opened and is not used again.
            unsafe { FindCloseChangeNotification(changes) };
            return Err(error);
        }
        // Handles are plain numbers to the OS; carry them across as such.
        let (changes_id, stop_id) = (changes as usize, stop as usize);
        let thread = thread::Builder::new()
            .name("oasis-folder-watch".into())
            .spawn(move || {
                wait_loop(changes_id as HANDLE, stop_id as HANDLE, &on_change);
                // SAFETY: this thread is the only user of `changes`, and it is done with it.
                unsafe { FindCloseChangeNotification(changes_id as HANDLE) };
            })
            .inspect_err(|_| {
                // SAFETY: neither handle was handed to a thread.
                unsafe {
                    FindCloseChangeNotification(changes);
                    CloseHandle(stop);
                }
            })?;
        Ok(FolderWatch { stop: stop_id, thread: Some(thread) })
    }
}

#[allow(unsafe_code)]
fn wait_loop(changes: HANDLE, stop: HANDLE, on_change: &impl Fn()) {
    let both = [stop, changes];
    loop {
        // SAFETY: both handles stay open until this thread ends (the stop event until `drop` joins it).
        let hit = unsafe { WaitForMultipleObjects(2, both.as_ptr(), 0, INFINITE) };
        if hit != WAIT_OBJECT_0 + 1 {
            return; // stop was set, or the wait failed
        }
        // Keep re-arming while more writes arrive; tell once things go quiet.
        loop {
            // SAFETY: `changes` is a valid notification handle.
            if unsafe { FindNextChangeNotification(changes) } == 0 {
                return;
            }
            // SAFETY: as above.
            match unsafe { WaitForMultipleObjects(2, both.as_ptr(), 0, QUIET) } {
                h if h == WAIT_OBJECT_0 + 1 => continue,
                h if h == WAIT_OBJECT_0 => return,
                _ => break,
            }
        }
        on_change();
    }
}

impl Drop for FolderWatch {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: the event is open until closed below, after the thread has ended.
        unsafe { SetEvent(self.stop as HANDLE) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        // SAFETY: the thread is gone, so nothing waits on the event any more.
        unsafe { CloseHandle(self.stop as HANDLE) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn tells_once_after_a_burst_of_writes_and_stops_on_drop() {
        let dir = std::env::temp_dir().join(format!("oasis-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (tx, rx) = mpsc::channel();
        let watch = FolderWatch::start(&dir, move || {
            let _ = tx.send(());
        })
        .unwrap();
        for i in 0..3 {
            std::fs::write(dir.join("a.json"), format!("{i}")).unwrap();
        }
        rx.recv_timeout(Duration::from_secs(3)).expect("a notification");
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err(), "the burst was told once");
        drop(watch);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_folder_fails() {
        assert!(FolderWatch::start(Path::new("C:\\no\\such\\folder\\oasis"), || {}).is_err());
    }
}
