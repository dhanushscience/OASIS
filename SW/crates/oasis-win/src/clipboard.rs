//! Put text on the Windows clipboard. Port of `TrySetClipboardText` in `Services/ActionExecutor.cs`.
//!
//! Dictation is pasted (clipboard, then Ctrl+V) so the whole text lands at once, however long it
//! is and whatever the app does with typed characters. What OASIS puts there is kept out of
//! Windows clipboard history (Win+V) and cloud clipboard sync, and the user's own text is read
//! first so it can be put back (`docs/VOICE_DECISIONS.md` V8, step 7).

#![allow(unsafe_code)]

use std::fmt;

use windows_sys::Win32::Foundation::{GlobalFree, GetLastError, HANDLE};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

/// The clipboard format for UTF-16 text.
const CF_UNICODETEXT: u32 = 13;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardError {
    /// Another program holds the clipboard open; Windows error code.
    Busy(u32),
    /// Memory for the text could not be allocated.
    NoMemory,
    /// Windows refused the data; error code.
    Refused(u32),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClipboardError::Busy(code) => write!(f, "The clipboard is in use by another program (error {code})."),
            ClipboardError::NoMemory => write!(f, "No memory for the clipboard text."),
            ClipboardError::Refused(code) => write!(f, "Windows would not take the clipboard text (error {code})."),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// The text as the clipboard wants it: UTF-16, line breaks as CR LF, ending in a zero.
pub fn clipboard_units(text: &str) -> Vec<u16> {
    let mut out = Vec::with_capacity(text.len() + 1);
    let mut previous_cr = false;
    for unit in text.encode_utf16() {
        if unit == u16::from(b'\n') && !previous_cr {
            out.push(u16::from(b'\r'));
        }
        previous_cr = unit == u16::from(b'\r');
        out.push(unit);
    }
    out.push(0);
    out
}

/// Formats that tell Windows not to keep this text in clipboard history or sync it to the cloud,
/// and tell clipboard managers that honour it to skip it. Their value is a DWORD 0 (for the last
/// one only its presence counts).
const PRIVATE_FORMATS: [&str; 3] = ["CanIncludeInClipboardHistory", "CanUploadToCloudClipboard", "ExcludeClipboardContentFromMonitorProcessing"];

/// Replace the clipboard with `text`, kept out of clipboard history. Tries a few times: another
/// program may have it open for a moment (clipboard managers do).
pub fn set_text(text: &str) -> Result<(), ClipboardError> {
    let units = clipboard_units(text);
    let mut last = ClipboardError::Busy(0);
    for _ in 0..10 {
        match try_set(&units) {
            Ok(()) => return Ok(()),
            Err(e @ ClipboardError::Busy(_)) => {
                last = e;
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(e) => return Err(e),
        }
    }
    Err(last)
}

fn try_set(units: &[u16]) -> Result<(), ClipboardError> {
    let bytes = units.len() * 2;
    // SAFETY: a null owner window is allowed; the clipboard is closed on every path below.
    if unsafe { OpenClipboard(std::ptr::null_mut()) } == 0 {
        // SAFETY: no preconditions.
        return Err(ClipboardError::Busy(unsafe { GetLastError() }));
    }
    let result = (|| {
        // SAFETY: the clipboard is open and ours.
        if unsafe { EmptyClipboard() } == 0 {
            // SAFETY: no preconditions.
            return Err(ClipboardError::Refused(unsafe { GetLastError() }));
        }
        // SAFETY: plain allocation of `bytes` bytes of movable memory.
        let handle: HANDLE = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes) };
        if handle.is_null() {
            return Err(ClipboardError::NoMemory);
        }
        // SAFETY: `handle` is a fresh movable block of `bytes` bytes; it is locked only while the
        // text is copied in, and `units` holds exactly `bytes` bytes.
        unsafe {
            let target = GlobalLock(handle).cast::<u16>();
            if target.is_null() {
                GlobalFree(handle);
                return Err(ClipboardError::NoMemory);
            }
            std::ptr::copy_nonoverlapping(units.as_ptr(), target, units.len());
            GlobalUnlock(handle);
        }
        // SAFETY: on success the system owns `handle`; on failure it is still ours to free.
        let accepted = unsafe { SetClipboardData(CF_UNICODETEXT, handle) };
        if accepted.is_null() {
            // SAFETY: the system did not take the block, so it is freed here.
            let code = unsafe { GetLastError() };
            unsafe { GlobalFree(handle) };
            return Err(ClipboardError::Refused(code));
        }
        // Best effort: an older Windows without clipboard history ignores these formats.
        for name in PRIVATE_FORMATS {
            set_dword_format(name, 0);
        }
        Ok(())
    })();
    // SAFETY: matches the successful OpenClipboard above.
    unsafe { CloseClipboard() };
    result
}

/// Put a registered format holding one DWORD on the open clipboard.
fn set_dword_format(name: &str, value: u32) {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is a zero-terminated UTF-16 string.
    let format = unsafe { RegisterClipboardFormatW(wide.as_ptr()) };
    if format == 0 {
        return;
    }
    // SAFETY: a fresh movable block of 4 bytes, locked only while the value is written; the system
    // owns it once SetClipboardData accepts it, else it is freed here.
    unsafe {
        let handle: HANDLE = GlobalAlloc(GMEM_MOVEABLE, 4);
        if handle.is_null() {
            return;
        }
        let target = GlobalLock(handle).cast::<u32>();
        if target.is_null() {
            GlobalFree(handle);
            return;
        }
        target.write_unaligned(value);
        GlobalUnlock(handle);
        if SetClipboardData(format, handle).is_null() {
            GlobalFree(handle);
        }
    }
}

/// A number that changes every time anything is put on the clipboard, by any program.
pub fn sequence() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetClipboardSequenceNumber() }
}

/// The text on the clipboard now, if it holds text. `None` when it holds something else (a
/// picture, files) or nothing, or another program keeps it busy.
pub fn get_text() -> Option<String> {
    // SAFETY: no preconditions.
    if unsafe { IsClipboardFormatAvailable(CF_UNICODETEXT) } == 0 {
        return None;
    }
    for _ in 0..10 {
        // SAFETY: a null owner window is allowed; closed below on every path.
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            // SAFETY: the clipboard is open; the block is only read while locked, up to its zero
            // terminator, which CF_UNICODETEXT data always has.
            let text = unsafe {
                let handle = GetClipboardData(CF_UNICODETEXT);
                let source = if handle.is_null() { std::ptr::null() } else { GlobalLock(handle).cast::<u16>().cast_const() };
                let text = (!source.is_null()).then(|| {
                    let mut len = 0;
                    while *source.add(len) != 0 {
                        len += 1;
                    }
                    String::from_utf16_lossy(std::slice::from_raw_parts(source, len))
                });
                if !source.is_null() {
                    GlobalUnlock(handle);
                }
                text
            };
            // SAFETY: matches the successful OpenClipboard above.
            unsafe { CloseClipboard() };
            return text;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uses the real clipboard, so it runs only when asked: `cargo test -p oasis-win -- --ignored`.
    /// What was on it is put back.
    #[test]
    #[ignore = "uses the real clipboard"]
    fn text_goes_on_reads_back_and_the_sequence_moves() {
        let before = get_text();
        let start = sequence();
        set_text("oasis clipboard test\nline two").unwrap();
        assert_eq!(get_text().as_deref(), Some("oasis clipboard test\r\nline two"));
        assert_ne!(sequence(), start);
        if let Some(text) = before {
            set_text(&text).unwrap();
            assert_eq!(get_text(), Some(text));
        }
    }

    #[test]
    fn line_breaks_become_crlf_once() {
        let text = |s: &str| String::from_utf16_lossy(&clipboard_units(s));
        assert_eq!(text("a\nb"), "a\r\nb\0");
        assert_eq!(text("a\r\nb"), "a\r\nb\0");
        assert_eq!(text(""), "\0");
        assert_eq!(text("é\n"), "é\r\n\0");
    }
}
