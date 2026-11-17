//! Open a URL, file, folder or program the way Explorer would. Port of the launch half of
//! `UserLevelProcess` / `ActionExecutor`; the "start at normal rights while running as
//! administrator" part comes with elevation support (step W6).

use std::ptr;

use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `ShellExecute` "open" on `target`, with optional command-line `args`. `Err` carries the
/// code Windows returned (31 = no program is associated, 2 = not found, 5 = access denied).
#[allow(unsafe_code)]
pub fn open(target: &str, args: Option<&str>) -> Result<(), u32> {
    let verb = wide("open");
    let target = wide(target);
    let args = args.map(wide);
    // SAFETY: every pointer is either null or a NUL-terminated UTF-16 buffer that outlives the
    // call.
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            args.as_ref().map_or(ptr::null(), |a| a.as_ptr()),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean success; below are error codes.
    let code = result as usize;
    if code > 32 {
        Ok(())
    } else {
        Err(code as u32)
    }
}

/// The file a Start Menu shortcut (`.lnk`) starts, or `None` if it cannot be read.
#[allow(unsafe_code)]
pub fn shortcut_target(lnk: &str) -> Option<String> {
    use windows::core::{Interface, HSTRING};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, STGM_READ,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    let mut buf = [0u16; 1024];
    // SAFETY: COM is initialised for this thread first (an "already initialised" result is fine);
    // the interfaces are owned and released by their wrappers; `buf` outlives `GetPath`, which
    // writes at most `buf.len()` units, and a null find-data pointer is allowed.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        link.cast::<IPersistFile>().ok()?.Load(&HSTRING::from(lnk), STGM_READ).ok()?;
        link.GetPath(&mut buf, ptr::null_mut(), 0).ok()?;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len])).filter(|s| !s.is_empty())
}

/// The program Windows opens web links with (the default browser), as a full path; `None` when
/// none is set.
#[allow(unsafe_code)]
pub fn default_browser() -> Option<String> {
    use windows_sys::Win32::UI::Shell::{AssocQueryStringW, ASSOCF_NONE, ASSOCSTR_EXECUTABLE};
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (scheme, verb) = (wide("https"), wide("open"));
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: both strings are NUL-terminated and live across the call; `buf` is writable for
    // `len` UTF-16 units, which the call reads and updates.
    let hr = unsafe { AssocQueryStringW(ASSOCF_NONE, ASSOCSTR_EXECUTABLE, scheme.as_ptr(), verb.as_ptr(), buf.as_mut_ptr(), &mut len) };
    if hr != 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let path = String::from_utf16_lossy(&buf[..end]);
    (!path.trim().is_empty()).then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_program_is_reported_not_ignored() {
        assert!(open("C:\\definitely\\not\\here\\oasis-test.exe", None).is_err());
    }

    #[test]
    fn strings_are_nul_terminated_utf16() {
        assert_eq!(wide("hé"), [0x68, 0xE9, 0]);
    }

    #[test]
    fn a_shortcut_target_is_read() {
        let dir = std::env::temp_dir();
        let lnk = dir.join(format!("oasis-lnk-test-{}.lnk", std::process::id()));
        let target = std::env::var("ComSpec").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".into());
        let made = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath='{}'; $s.Save()",
                lnk.display(),
                target
            ))
            .status()
            .unwrap();
        assert!(made.success());
        let read = shortcut_target(&lnk.to_string_lossy());
        let _ = std::fs::remove_file(&lnk);
        assert_eq!(read.map(|s| s.to_lowercase()), Some(target.to_lowercase()));
        assert_eq!(shortcut_target(r"C:\no\such\file.lnk"), None);
    }

    #[test]
    fn the_default_browser_is_a_program_that_exists() {
        // On a PC without a browser set this is None; where one is set it must be a real file.
        if let Some(path) = default_browser() {
            assert!(std::path::Path::new(&path).is_file(), "{path}");
            assert!(path.to_lowercase().ends_with(".exe"), "{path}");
        }
    }
}

/// A Yes/No box in front of every window; `true` when Yes is pressed. Blocks until answered.
#[allow(unsafe_code)]
pub fn ask_yes_no(title: &str, text: &str) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONQUESTION, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO};
    let (title, text) = (wide(title), wide(text));
    // SAFETY: both are NUL-terminated UTF-16 buffers that outlive the call; a null owner is allowed.
    let answer = unsafe { MessageBoxW(ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_YESNO | MB_ICONQUESTION | MB_TOPMOST | MB_SETFOREGROUND) };
    answer == IDYES
}
