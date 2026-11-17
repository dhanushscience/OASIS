//! Port of `Platform/SecurityLog.cs`: a best-effort audit trail of things that were blocked or
//! trusted (a refused URL scheme, a blocked script, a pad being trusted). Never throws: a
//! failing log must not stop the action it records.
//!
//! One line per entry, `<UTC time> - <message>`. The file rotates to `security.log.old` past
//! 256 KiB. Times are UTC (C# wrote local time), as in the crash log.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::crash_log::utc_timestamp;

const MAX_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone)]
pub struct SecurityLog {
    path: PathBuf,
}

impl SecurityLog {
    /// `path` is `OasisPaths::security_log()`.
    pub fn at(path: impl Into<PathBuf>) -> SecurityLog {
        SecurityLog { path: path.into() }
    }

    pub fn write(&self, message: &str) {
        let _ = self.write_inner(message);
    }

    fn write_inner(&self, message: &str) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        self.rotate_if_needed()?;
        let mut file = OpenOptions::new().create(true).append(true).open(&self.path)?;
        writeln!(file, "{} - {}", utc_timestamp(SystemTime::now()), message)
    }

    fn rotate_if_needed(&self) -> std::io::Result<()> {
        match fs::metadata(&self.path) {
            Ok(meta) if meta.len() > MAX_BYTES => {
                let mut backup = self.path.as_os_str().to_owned();
                backup.push(".old");
                let backup = PathBuf::from(backup);
                let _ = fs::remove_file(&backup);
                fs::rename(&self.path, backup)
            }
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-seclog-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("security.log")
    }

    #[test]
    fn appends_one_line_per_entry() {
        let path = temp("append");
        let log = SecurityLog::at(&path);
        log.write("Blocked OpenUrl scheme: javascript:x");
        log.write("BLE device trusted: ABC");
        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with("Z - Blocked OpenUrl scheme: javascript:x"));
    }

    #[test]
    fn rotates_past_the_size_limit() {
        let path = temp("rotate");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![b'x'; (MAX_BYTES + 1) as usize]).unwrap();
        SecurityLog::at(&path).write("fresh");
        assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 1);
        assert!(path.with_file_name("security.log.old").exists());
    }

    #[test]
    fn never_panics_when_the_file_cannot_be_written() {
        // A folder where the file should be.
        let path = temp("blocked");
        fs::create_dir_all(&path).unwrap();
        SecurityLog::at(&path).write("ignored");
    }
}
