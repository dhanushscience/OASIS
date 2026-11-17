//! The one pad this computer trusts. Port of `Services/BleDeviceTrust.cs`; same file
//! (`trusted_device.json`) and layout, so a pad paired with the C# app stays trusted:
//!
//! ```json
//! { "Address": "189285357480946", "DeviceName": "OASIS_INTERFACE", "TrustedUtc": "…Z" }
//! ```
//!
//! `Address` is the pad id. On Windows and Linux that is the Bluetooth address in decimal, which
//! is exactly what C# wrote. Until a pad connects once, every OASIS pad is acceptable; after
//! that only that pad (`oasis_core::pad_connect::is_trusted`).

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::SystemTime;

use serde_json::{json, Value};

use crate::atomic_file;
use crate::crash_log::utc_timestamp;

pub struct TrustedDevice {
    path: PathBuf,
    id: Option<String>,
}

impl TrustedDevice {
    /// `path` is `OasisPaths::trusted_device()`. A missing or unreadable file means nobody is
    /// trusted yet, as in C#.
    pub fn load(path: PathBuf) -> TrustedDevice {
        let id = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
            .and_then(|doc| doc.get("Address")?.as_str().map(str::to_string))
            .filter(|address| !address.trim().is_empty());
        TrustedDevice { path, id }
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn has_trusted_device(&self) -> bool {
        self.id.is_some()
    }

    pub fn is_trusted(&self, id: &str) -> bool {
        oasis_core::pad_connect::is_trusted(self.id(), id)
    }

    /// Trust `id`. Returns `true` if this changed who is trusted (the caller writes the
    /// security log entry); trusting the same pad again does nothing.
    pub fn trust(&mut self, id: &str, device_name: &str, now: SystemTime) -> io::Result<bool> {
        if self.id.as_deref() == Some(id) {
            return Ok(false);
        }
        let doc = json!({
            "Address": id,
            "DeviceName": device_name,
            "TrustedUtc": iso_utc(now),
        });
        let text = serde_json::to_string_pretty(&doc).expect("a JSON object always serialises");
        atomic_file::write_utf8(&self.path, &text)?;
        self.id = Some(id.to_string());
        Ok(true)
    }
}

/// `2026-07-11T17:27:59.6544716Z`, the shape of .NET's round-trip format.
fn iso_utc(t: SystemTime) -> String {
    let nanos = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let stamp = utc_timestamp(t);
    let stamp = stamp.trim_end_matches('Z');
    let (date, time) = stamp.split_once(' ').unwrap_or((stamp, "00:00:00"));
    format!("{date}T{time}.{:07}Z", nanos / 100)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-trust-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("trusted_device.json")
    }

    #[test]
    fn a_fresh_install_trusts_nobody_and_accepts_any_pad() {
        let t = TrustedDevice::load(temp("fresh"));
        assert!(!t.has_trusted_device());
        assert!(t.is_trusted("anything"));
    }

    #[test]
    fn reads_the_file_the_csharp_app_wrote() {
        let path = temp("csharp");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{
  "Address": "189285357480946",
  "DeviceName": "OASIS_INTERFACE",
  "TrustedUtc": "2026-07-11T17:27:59.6544716Z"
}"#,
        )
        .unwrap();
        let t = TrustedDevice::load(path);
        assert_eq!(t.id(), Some("189285357480946"));
        assert!(t.is_trusted("189285357480946"));
        assert!(!t.is_trusted("189285357480947"));
    }

    #[test]
    fn trusting_writes_the_same_layout_and_is_idempotent() {
        let path = temp("write");
        let mut t = TrustedDevice::load(path.clone());
        let at = SystemTime::UNIX_EPOCH + Duration::new(1_783_000_000, 123_456_700);
        assert!(t.trust("42", "OASIS_INTERFACE", at).unwrap());
        assert!(!t.trust("42", "OASIS_INTERFACE", at).unwrap());

        let doc: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(doc["Address"], "42");
        assert_eq!(doc["DeviceName"], "OASIS_INTERFACE");
        assert_eq!(doc["TrustedUtc"], "2026-07-02T13:46:40.1234567Z");
        assert_eq!(TrustedDevice::load(path).id(), Some("42"));
    }

    #[test]
    fn a_damaged_file_trusts_nobody() {
        let path = temp("damaged");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        for bad in ["not json", r#"{"Address": 5}"#, r#"{"Address": " "}"#, "{}"] {
            fs::write(&path, bad).unwrap();
            assert!(!TrustedDevice::load(path.clone()).has_trusted_device(), "{bad}");
        }
    }
}
