//! The two system switches in `profiles.json`: `StartWithWindows` (default on, as in C#) and
//! `RunAsAdministrator` (default off).

use std::path::Path;

use serde_json::Value;

use crate::model::Settings;

const START_WITH_WINDOWS: &str = "StartWithWindows";
const RUN_AS_ADMINISTRATOR: &str = "RunAsAdministrator";

fn flag(settings: &Settings, key: &str, default: bool) -> bool {
    settings.extra.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub fn start_with_windows(settings: &Settings) -> bool {
    flag(settings, START_WITH_WINDOWS, true)
}

pub fn run_as_administrator(settings: &Settings) -> bool {
    flag(settings, RUN_AS_ADMINISTRATOR, false)
}

impl Settings {
    pub fn set_start_with_windows(&mut self, on: bool) {
        self.extra.insert(START_WITH_WINDOWS.into(), Value::Bool(on));
    }

    pub fn set_run_as_administrator(&mut self, on: bool) {
        self.extra.insert(RUN_AS_ADMINISTRATOR.into(), Value::Bool(on));
    }
}

/// Reads `RunAsAdministrator` straight from the file. The hand-over to the elevated copy happens
/// before settings are loaded, and must not create, migrate or verify anything. Any problem reads
/// as "off".
pub fn read_run_as_administrator(profiles_json: &Path) -> bool {
    std::fs::read_to_string(profiles_json)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|root| root.get(RUN_AS_ADMINISTRATOR).and_then(Value::as_bool))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_match_csharp() {
        let s = Settings::default();
        assert!(start_with_windows(&s));
        assert!(!run_as_administrator(&s));
    }

    #[test]
    fn the_switches_are_set_and_kept_in_the_file_text() {
        let mut s = Settings::default();
        s.set_start_with_windows(false);
        s.set_run_as_administrator(true);
        assert!(!start_with_windows(&s) && run_as_administrator(&s));
        let text = serde_json::to_string(&s).unwrap();
        assert!(text.contains("\"StartWithWindows\":false") && text.contains("\"RunAsAdministrator\":true"));
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert!(!start_with_windows(&back) && run_as_administrator(&back));
    }

    fn file(name: &str, text: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("oasis-sys-{name}-{}.json", std::process::id()));
        std::fs::write(&path, text).unwrap();
        path
    }

    #[test]
    fn the_raw_read_trusts_nothing_and_defaults_to_off() {
        assert!(read_run_as_administrator(&file("on", r#"{"RunAsAdministrator": true}"#)));
        assert!(!read_run_as_administrator(&file("off", r#"{"RunAsAdministrator": false}"#)));
        assert!(!read_run_as_administrator(&file("absent", r#"{"Profiles": []}"#)));
        assert!(!read_run_as_administrator(&file("junk", "not json")));
        assert!(!read_run_as_administrator(&file("string", r#"{"RunAsAdministrator": "true"}"#)));
        assert!(!read_run_as_administrator(Path::new(r"C:\no\such\file.json")));
    }
}
