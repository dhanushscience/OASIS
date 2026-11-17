//! Starting with Windows, and starting as administrator. Port of `AutostartService` and
//! `Services/Elevation/AdminMode.cs`; the rules (arguments, task XML, when to hand over) are in
//! `oasis_core::startup`.
//!
//! Neither is changed except when the user asks. The C# app re-applies "start with Windows" at
//! every launch; doing that here would silently repoint the C# app's login entry at this build.

use std::fs;
use std::io;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use oasis_core::startup::{
    autostart_command, elevated_task_xml, AUTOSTART_VALUE_NAME, ELEVATED_TASK_NAME,
};
use oasis_win::system;

use crate::paths::OasisPaths;
use crate::security_log::SecurityLog;

/// Same names as the C# app, so only one of the two builds can run at a time.
pub const SINGLE_INSTANCE_MUTEX: &str = r"Local\ProjectOASIS.SingleInstance";
pub const SHOW_WINDOW_EVENT: &str = r"Local\ProjectOASIS.ShowWindow";

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SCHTASKS_TIMEOUT: Duration = Duration::from_secs(30);
/// A "show the window" note older than this is from a start that never happened.
const NOTE_LIFETIME: Duration = Duration::from_secs(60);

// ---- start with Windows -----------------------------------------------------------------------

/// Whether OASIS is registered to start when the user signs in (the Run key).
pub fn autostart_enabled() -> bool {
    system::run_key_value(AUTOSTART_VALUE_NAME).is_some()
}

/// Register or remove the login entry for `exe` (started with `--tray`).
pub fn set_autostart(enabled: bool, exe: &Path) -> bool {
    if enabled {
        system::set_run_key_value(AUTOSTART_VALUE_NAME, Some(&autostart_command(&exe.to_string_lossy())))
    } else {
        system::set_run_key_value(AUTOSTART_VALUE_NAME, None)
    }
}

// ---- administrator mode -----------------------------------------------------------------------

/// What the status line says about administrator mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminStatus {
    Off,
    /// The mode is off but this session is still elevated: restart OASIS.
    OffStillElevated,
    OnAndElevated,
    /// On, the task exists, but this copy is not elevated: restart OASIS.
    OnNotElevated,
    /// On, but the task is missing: turn it off and on again.
    OnNotSetUp,
}

impl AdminStatus {
    pub fn text(self) -> &'static str {
        match self {
            AdminStatus::Off => "OFF",
            AdminStatus::OffStillElevated => "OFF, but this session is still elevated; restart OASIS",
            AdminStatus::OnAndElevated => "ON: running as administrator",
            AdminStatus::OnNotElevated => "ON, but not elevated; restart OASIS",
            AdminStatus::OnNotSetUp => "ON, but not set up; turn it off and on again",
        }
    }
}

/// `exe` is this program: a task made for another build (the C# app's) counts as not set up.
pub fn admin_status(setting_on: bool, exe: &Path) -> AdminStatus {
    match (setting_on, system::is_elevated()) {
        (false, false) => AdminStatus::Off,
        (false, true) => AdminStatus::OffStillElevated,
        (true, true) => AdminStatus::OnAndElevated,
        (true, false) if task_runs(exe) => AdminStatus::OnNotElevated,
        (true, false) => AdminStatus::OnNotSetUp,
    }
}

fn schtasks_path() -> PathBuf {
    let root = std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    root.join("System32").join("schtasks.exe")
}

/// Run `schtasks.exe`; its exit code, or `None` if it could not run or the prompt was declined.
/// `elevate` shows the single UAC prompt (for schtasks, not for OASIS).
fn run_schtasks(arguments: &[&str], elevate: bool) -> Option<i32> {
    if elevate {
        let line = arguments.iter().map(|a| quote_argument(a)).collect::<Vec<_>>().join(" ");
        return system::run_elevated_and_wait(&schtasks_path().to_string_lossy(), &line, SCHTASKS_TIMEOUT)
            .map(|code| code as i32);
    }
    Command::new(schtasks_path())
        .args(arguments)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .and_then(|out| out.status.code())
}

/// Quote an argument for a Windows command line when it has a space.
fn quote_argument(arg: &str) -> String {
    if arg.contains(' ') {
        format!("\"{arg}\"")
    } else {
        arg.to_string()
    }
}

pub fn is_task_installed() -> bool {
    run_schtasks(&["/query", "/tn", ELEVATED_TASK_NAME], false) == Some(0)
}

/// The program the task starts, read from its definition.
pub fn task_command() -> Option<String> {
    let output = Command::new(schtasks_path())
        .args(["/query", "/tn", ELEVATED_TASK_NAME, "/xml"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    command_from_task_xml(&decode_task_xml(&output.stdout))
}

/// Whether the task starts exactly `exe`. The C# app and this one share the task's name, so a
/// task made by the other build starts the other build: handing over to it would be wrong.
pub fn task_runs(exe: &Path) -> bool {
    task_command().is_some_and(|command| same_program(&command, exe))
}

fn same_program(command: &str, exe: &Path) -> bool {
    command.trim().trim_matches('"').eq_ignore_ascii_case(exe.to_string_lossy().trim())
}

/// `schtasks /xml` prints UTF-16 or the console's code page depending on the Windows version.
fn decode_task_xml(bytes: &[u8]) -> String {
    let utf16 = bytes.starts_with(&[0xFF, 0xFE])
        || bytes.get(1).is_some_and(|b| *b == 0) && bytes.get(3).is_some_and(|b| *b == 0);
    if utf16 {
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units).trim_start_matches('\u{feff}').to_string()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn command_from_task_xml(xml: &str) -> Option<String> {
    let start = xml.find("<Command>")? + "<Command>".len();
    let end = start + xml[start..].find("</Command>")?;
    Some(
        xml[start..end]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&"),
    )
}

/// Register the administrator task for `exe`. From a normal-level OASIS this shows one UAC
/// prompt. False if the user declines or Windows refuses.
pub fn install_admin_task(paths: &OasisPaths, exe: &Path, start_at_sign_in: bool) -> bool {
    let Some(sid) = system::current_user_sid() else { return false };
    let xml = elevated_task_xml(&exe.to_string_lossy(), &sid, start_at_sign_in);
    let file = std::env::temp_dir().join(format!("oasis-admin-task-{}.xml", std::process::id()));
    // Task Scheduler reads this as UTF-16, as its XML declaration says (with a byte-order mark).
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
    if fs::write(&file, bytes).is_err() {
        return false;
    }
    let code = run_schtasks(
        &["/create", "/tn", ELEVATED_TASK_NAME, "/xml", &file.to_string_lossy(), "/f"],
        !system::is_elevated(),
    );
    let _ = fs::remove_file(&file);
    SecurityLog::at(paths.security_log()).write(&format!(
        "Administrator mode: task install exit={} startAtSignIn={start_at_sign_in}",
        code.map_or("declined".to_string(), |c| c.to_string())
    ));
    code == Some(0) && is_task_installed()
}

/// Remove the administrator task (one UAC prompt from a normal-level OASIS).
pub fn remove_admin_task(paths: &OasisPaths) -> bool {
    if !is_task_installed() {
        return true;
    }
    let code = run_schtasks(&["/delete", "/tn", ELEVATED_TASK_NAME, "/f"], !system::is_elevated());
    SecurityLog::at(paths.security_log()).write(&format!(
        "Administrator mode: task removal exit={}",
        code.map_or("declined".to_string(), |c| c.to_string())
    ));
    !is_task_installed()
}

/// Ask Task Scheduler to start the elevated OASIS now. No prompt.
pub fn start_elevated_instance() -> bool {
    run_schtasks(&["/run", "/tn", ELEVATED_TASK_NAME], false) == Some(0)
}

// ---- "show my window" note --------------------------------------------------------------------

const RESTART_NOTE: &str = "restart";

fn note_path(app_data_root: &Path) -> PathBuf {
    app_data_root.join("show-window-on-start")
}

/// Leave a note asking the next OASIS to open its window even though the task starts it in the
/// tray. `restart` also says it replaces a running OASIS.
pub fn request_window_on_next_start(app_data_root: &Path, restart: bool) -> io::Result<()> {
    fs::create_dir_all(app_data_root)?;
    fs::write(note_path(app_data_root), if restart { RESTART_NOTE } else { "window" })
}

/// What a note said.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowRequest {
    pub is_restart: bool,
}

/// Take the note, once. A note older than a minute is dropped as stale.
pub fn consume_window_request(app_data_root: &Path) -> Option<WindowRequest> {
    let path = note_path(app_data_root);
    let modified = fs::metadata(&path).ok()?.modified().ok()?;
    let fresh = SystemTime::now().duration_since(modified).is_ok_and(|age| age < NOTE_LIFETIME);
    let text = fs::read_to_string(&path).unwrap_or_default();
    let _ = fs::remove_file(&path);
    fresh.then(|| WindowRequest { is_restart: text.trim() == RESTART_NOTE })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-startup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_window_note_is_read_once() {
        let dir = scratch("note");
        request_window_on_next_start(&dir, false).unwrap();
        assert_eq!(consume_window_request(&dir), Some(WindowRequest { is_restart: false }));
        assert_eq!(consume_window_request(&dir), None, "gone after one read");
    }

    #[test]
    fn a_restart_note_says_so() {
        let dir = scratch("restart");
        request_window_on_next_start(&dir, true).unwrap();
        assert_eq!(consume_window_request(&dir), Some(WindowRequest { is_restart: true }));
    }

    #[test]
    fn no_note_means_no_request() {
        assert_eq!(consume_window_request(&scratch("none")), None);
    }

    #[test]
    fn the_task_program_is_read_from_its_xml() {
        let xml = r#"<Task><Actions><Exec><Command>C:\Apps\A &amp; B\oasis.exe</Command><Arguments>--tray</Arguments></Exec></Actions></Task>"#;
        assert_eq!(command_from_task_xml(xml).as_deref(), Some(r"C:\Apps\A & B\oasis.exe"));
        assert_eq!(command_from_task_xml("<Task/>"), None);
    }

    #[test]
    fn a_task_made_for_another_build_is_not_this_one() {
        let mine = Path::new(r"C:\Rust\oasis-desktop.exe");
        assert!(same_program(r"c:\rust\OASIS-desktop.exe", mine));
        assert!(same_program(r#""C:\Rust\oasis-desktop.exe""#, mine));
        assert!(!same_program(r"C:\CSharp\OASIS.exe", mine));
    }

    #[test]
    fn the_task_xml_is_decoded_from_utf16_or_text() {
        let text = "<Command>x</Command>";
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode_task_xml(&utf16), text);
        assert_eq!(decode_task_xml(text.as_bytes()), text);
    }

    #[test]
    fn arguments_with_spaces_are_quoted() {
        assert_eq!(quote_argument(r"C:\a b\x.xml"), r#""C:\a b\x.xml""#);
        assert_eq!(quote_argument("/create"), "/create");
    }

    #[test]
    fn the_status_lines_say_what_to_do() {
        assert!(AdminStatus::OnNotSetUp.text().contains("turn it off and on"));
        assert!(AdminStatus::OnNotElevated.text().contains("restart"));
        assert_eq!(AdminStatus::Off.text(), "OFF");
    }
}
