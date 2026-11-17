//! Rotating crash log. Port of `Platform/CrashLog.cs`.
//!
//! **Writes must never fail loudly.** A logger that panics inside a panic hook or error handler
//! turns a recoverable fault into a hard kill and loses the only record of what happened. So
//! every function here swallows its own I/O errors — the one place in OASIS where that is the
//! right thing to do.
//!
//! The file rotates to `crash.log.old` past 512 KiB. Timestamps are UTC (the C# log used local
//! time; UTC needs no time-zone database and is unambiguous across OSes).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 512 * 1024;

static GATE: Mutex<()> = Mutex::new(());

pub struct CrashLog {
    path: PathBuf,
}

impl CrashLog {
    pub fn at(path: impl Into<PathBuf>) -> CrashLog {
        CrashLog { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&self, source: &str, message: &str) {
        let _held = GATE.lock().unwrap_or_else(|e| e.into_inner());
        let _ = self.write_inner(source, message);
    }

    /// Route Rust panics into this log, then run the previous hook (which prints to stderr).
    pub fn install_panic_hook(self) {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            // The place and message, and the calls that led there: usually enough to fix a crash.
            self.write("panic", &format!("{info}\n{}", std::backtrace::Backtrace::force_capture()));
            previous(info);
        }));
    }

    fn write_inner(&self, source: &str, message: &str) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        self.rotate_if_needed()?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let nl = if cfg!(windows) { "\r\n" } else { "\n" };
        write!(
            f,
            "===== {} · {source} ====={nl}{message}{nl}{nl}",
            utc_timestamp(SystemTime::now())
        )
    }

    fn rotate_if_needed(&self) -> std::io::Result<()> {
        let Ok(meta) = fs::metadata(&self.path) else {
            return Ok(());
        };
        if meta.len() <= MAX_BYTES {
            return Ok(());
        }
        let mut backup = self.path.as_os_str().to_owned();
        backup.push(".old");
        let _ = fs::remove_file(&backup);
        fs::rename(&self.path, backup)
    }
}

/// `YYYY-MM-DD HH:MM:SSZ` without a date library (Howard Hinnant's civil-from-days).
pub fn utc_timestamp(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn timestamp() {
        assert_eq!(utc_timestamp(UNIX_EPOCH), "1970-01-01 00:00:00Z");
        assert_eq!(
            utc_timestamp(UNIX_EPOCH + Duration::from_secs(1_783_000_000)),
            "2026-07-02 13:46:40Z"
        );
    }

    #[test]
    fn writes_rotates_and_never_panics() {
        let dir = std::env::temp_dir().join(format!("oasis-crash-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let log = CrashLog::at(dir.join("crash.log"));
        log.write("test", "hello");
        assert!(fs::read_to_string(log.path())
            .unwrap()
            .contains("· test ====="));
        log.write("big", &"x".repeat(600 * 1024));
        log.write("after", "rotated");
        assert!(dir.join("crash.log.old").exists());
        // A path that cannot be created must not panic.
        CrashLog::at("/proc/definitely/not/writable/crash.log").write("x", "y");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod panic_hook_tests {
    use super::*;

    #[test]
    fn a_panic_is_written_with_its_place_and_calls() {
        let dir = std::env::temp_dir().join("oasis-panic-hook-test");
        let _ = fs::remove_dir_all(&dir);
        let log = CrashLog::at(dir.join("crash.log"));
        let path = log.path().to_path_buf();
        // The hook is process-wide: run the panic on its own thread and put the old hook back after.
        let previous = std::panic::take_hook();
        log.install_panic_hook();
        let _ = std::thread::spawn(|| panic!("a test panic")).join();
        let _ = std::panic::take_hook();
        std::panic::set_hook(previous);
        let text = fs::read_to_string(path).expect("the panic was written");
        assert!(text.contains("panic") && text.contains("a test panic") && text.contains("crash_log.rs"), "{text}");
    }
}
