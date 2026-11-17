//! The meeting settings (`Meetings` in `profiles.json`): reminders, and how early a meeting can be
//! joined. Read by the engine; edited by the Settings tab through `fields.rs`.

use serde_json::Value;

use crate::model::Settings;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeetingSettings {
    pub reminders_enabled: bool,
    pub reminder_minutes_before: i64,
    pub join_window_minutes_before: i64,
    /// The Google client id the C# app may have saved here; blank means look elsewhere.
    pub google_client_id: String,
}

impl MeetingSettings {
    /// `Meetings` as saved, with the C# default for anything missing or out of range.
    pub fn from(settings: &Settings) -> MeetingSettings {
        let section = settings.extra.get("Meetings").filter(|v| v.is_object());
        let number = |key: &str, default: i64, max: i64| {
            section.and_then(|s| s.get(key)).and_then(Value::as_i64).filter(|n| (1..=max).contains(n)).unwrap_or(default)
        };
        MeetingSettings {
            reminders_enabled: section.and_then(|s| s.get("RemindersEnabled")).and_then(Value::as_bool).unwrap_or(true),
            reminder_minutes_before: number("ReminderMinutesBefore", 5, 60),
            join_window_minutes_before: number("JoinWindowMinutesBefore", 15, 120),
            google_client_id: section.and_then(|s| s.get("GoogleClientId")).and_then(Value::as_str).unwrap_or("").trim().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn settings(json: Value) -> Settings {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn defaults_and_saved_values() {
        let d = MeetingSettings::from(&settings(json!({})));
        assert!(d.reminders_enabled && d.reminder_minutes_before == 5 && d.join_window_minutes_before == 15 && d.google_client_id.is_empty());
        let s = MeetingSettings::from(&settings(json!({ "Meetings": { "RemindersEnabled": false, "ReminderMinutesBefore": 10, "JoinWindowMinutesBefore": 30, "GoogleClientId": " id " } })));
        assert!(!s.reminders_enabled && s.reminder_minutes_before == 10 && s.join_window_minutes_before == 30 && s.google_client_id == "id");
        let bad = MeetingSettings::from(&settings(json!({ "Meetings": { "ReminderMinutesBefore": 0, "JoinWindowMinutesBefore": 9999 } })));
        assert_eq!((bad.reminder_minutes_before, bad.join_window_minutes_before), (5, 15));
    }
}
