//! Crash-safe file writes. Port of `Oasis.Adapters.Windows/Storage/AtomicJsonFile.cs`.
//!
//! Write to `<path>.tmp`, flush it to disk, then rename over the original. A crash or kill
//! between the flush and the rename leaves the original file intact (plus a leftover `.tmp`).
//! `rename` replaces the destination atomically on Linux and macOS, and on Windows std uses
//! `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, the same guarantee `File.Replace` gave the C# app.
//!
//! Writers to the same path are serialised inside this process, like the C# per-path locks.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

fn gate_for(path: &Path) -> Arc<Mutex<()>> {
    static GATES: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = OnceLock::new();
    let mut map = GATES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    map.entry(path.to_path_buf()).or_default().clone()
}

/// Atomically replace `path` with `contents` (UTF-8). Creates the parent folder.
pub fn write_utf8(path: &Path, contents: &str) -> io::Result<()> {
    write_bytes(path, contents.as_bytes())
}

/// As [`write_utf8`], for binary files (encrypted secrets).
pub fn write_bytes(path: &Path, contents: &[u8]) -> io::Result<()> {
    write_bytes_with_hook(path, contents, || {})
}

/// As [`write_utf8`]; `after_flush` runs between the flush and the rename (used by tests to
/// simulate a crash at the worst moment).
pub fn write_utf8_with_hook(
    path: &Path,
    contents: &str,
    after_flush: impl FnOnce(),
) -> io::Result<()> {
    write_bytes_with_hook(path, contents.as_bytes(), after_flush)
}

fn write_bytes_with_hook(
    path: &Path,
    contents: &[u8],
    after_flush: impl FnOnce(),
) -> io::Result<()> {
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let gate = gate_for(&full);
    let _held = gate.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(dir) = full.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp_name = full.as_os_str().to_owned();
    tmp_name.push(".tmp");
    let tmp = PathBuf::from(tmp_name);
    {
        let mut f = File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
    }
    after_flush();
    fs::rename(&tmp, &full)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-atomic-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn writes_and_replaces() {
        let dir = scratch("replace");
        let p = dir.join("sub").join("profiles.json");
        write_utf8(&p, "{\"v\":1}").unwrap();
        write_utf8(&p, "{\"v\":2}").unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "{\"v\":2}");
        assert!(!dir.join("sub").join("profiles.json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn original_survives_until_the_rename() {
        let dir = scratch("survive");
        let p = dir.join("profiles.json");
        write_utf8(&p, "old").unwrap();
        write_utf8_with_hook(&p, "new", || {
            // "Crash" point: the original is still the old content.
            assert_eq!(fs::read_to_string(&p).unwrap(), "old");
        })
        .unwrap();
        assert_eq!(fs::read_to_string(&p).unwrap(), "new");
        let _ = fs::remove_dir_all(&dir);
    }
}
