//! Calendar meetings: which one is joinable, when to remind, how to say it. Port of the pure parts
//! of `Services/Meetings/MeetingModels.cs` and `MeetingService.cs`. Fetching them from a calendar
//! is `oasis-engine::meetings`.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    /// Seconds since 1970.
    pub start: i64,
    pub end: i64,
    pub join_url: String,
    pub all_day: bool,
    /// `google` or `outlook`.
    pub provider: String,
}

impl Meeting {
    pub fn key(&self) -> String {
        format!("{}:{}", self.provider, self.id)
    }

    pub fn in_progress(&self, now: i64) -> bool {
        now >= self.start && now < self.end
    }

    /// Has a link, and it is no earlier than `window_minutes` before the start and not yet over.
    pub fn joinable(&self, now: i64, window_minutes: i64) -> bool {
        !self.join_url.trim().is_empty() && now >= self.start - window_minutes * 60 && now < self.end
    }
}

/// The meeting a join button should open: the earliest joinable one.
pub fn joinable_now(meetings: &[Meeting], now: i64, window_minutes: i64) -> Option<&Meeting> {
    meetings.iter().filter(|m| m.joinable(now, window_minutes)).min_by_key(|m| m.start)
}

/// The same meeting from two calendars (same start and title) is shown once, preferring the one
/// that has a join link. Sorted by start.
pub fn merge(meetings: Vec<Meeting>) -> Vec<Meeting> {
    let mut best: HashMap<(i64, String), Meeting> = HashMap::new();
    for m in meetings {
        let key = (m.start, m.title.trim().to_lowercase());
        match best.get(&key) {
            Some(old) if !old.join_url.trim().is_empty() || m.join_url.trim().is_empty() => {}
            _ => {
                best.insert(key, m);
            }
        }
    }
    let mut list: Vec<Meeting> = best.into_values().collect();
    list.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| a.title.cmp(&b.title)));
    list
}

/// What has been done about reminders so far.
#[derive(Debug, Default)]
pub struct ReminderState {
    pub reminded: HashSet<String>,
    pub snoozed_until: HashMap<String, i64>,
}

impl ReminderState {
    /// The meeting to remind about now: not already reminded, not snoozed, reminder time reached and
    /// not over. Expired snoozes are forgotten here.
    pub fn due<'a>(&mut self, meetings: &'a [Meeting], now: i64, minutes_before: i64) -> Option<&'a Meeting> {
        let mut sorted: Vec<&Meeting> = meetings.iter().collect();
        sorted.sort_by_key(|m| m.start);
        for m in sorted {
            let key = m.key();
            if self.reminded.contains(&key) {
                continue;
            }
            if let Some(until) = self.snoozed_until.get(&key).copied() {
                if now < until {
                    continue;
                }
                self.snoozed_until.remove(&key);
            }
            if now >= m.start - minutes_before * 60 && now < m.end {
                return Some(m);
            }
        }
        None
    }

    pub fn dismiss(&mut self, meeting: &Meeting) {
        self.reminded.insert(meeting.key());
    }

    pub fn snooze(&mut self, meeting: &Meeting, now: i64, minutes: i64) {
        self.reminded.remove(&meeting.key());
        self.snoozed_until.insert(meeting.key(), now + minutes * 60);
    }

    /// Forget meetings that are no longer on the calendar.
    pub fn prune(&mut self, meetings: &[Meeting]) {
        let live: HashSet<String> = meetings.iter().map(Meeting::key).collect();
        self.reminded.retain(|k| live.contains(k));
        self.snoozed_until.retain(|k, _| live.contains(k));
    }
}

fn civil_month_day(days: i64) -> (i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (m, d)
}

/// `Monday, Nov 8` for a moment on the clock `offset_secs` from UTC.
pub fn day_text(unix: i64, offset_secs: i32) -> String {
    const DAYS: [&str; 7] = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"];
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let days = (unix + i64::from(offset_secs)).div_euclid(86_400);
    let (month, day) = civil_month_day(days);
    format!("{}, {} {day}", DAYS[days.rem_euclid(7) as usize], MONTHS[(month - 1) as usize])
}

/// `Monday, Nov 8 · 3:00 PM – 4:00 PM`, or `… · All day`.
pub fn range_text(meeting: &Meeting, offset_secs: i32) -> String {
    use crate::voice::command::reminders::clock_text;
    let day = day_text(meeting.start, offset_secs);
    if meeting.all_day {
        return format!("{day} · All day");
    }
    format!("{day} · {} – {}", clock_text(meeting.start, offset_secs), clock_text(meeting.end, offset_secs))
}

/// `Standup starts in 5 minutes.`
pub fn reminder_text(meeting: &Meeting, now: i64) -> String {
    let minutes = (meeting.start - now + 59).div_euclid(60);
    match minutes {
        m if m <= 0 => format!("{} is starting now.", meeting.title),
        1 => format!("{} starts in 1 minute.", meeting.title),
        m => format!("{} starts in {m} minutes.", meeting.title),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meeting(id: &str, title: &str, start: i64, minutes: i64, url: &str) -> Meeting {
        Meeting { id: id.into(), title: title.into(), start, end: start + minutes * 60, join_url: url.into(), all_day: false, provider: "google".into() }
    }

    #[test]
    fn a_meeting_is_joinable_from_the_window_before_it_until_it_ends() {
        let m = meeting("a", "Standup", 10_000, 30, "https://meet.google.com/x");
        assert!(!m.joinable(10_000 - 16 * 60, 15));
        assert!(m.joinable(10_000 - 15 * 60, 15));
        assert!(m.joinable(10_000 + 29 * 60, 15));
        assert!(!m.joinable(10_000 + 30 * 60, 15), "over");
        assert!(!meeting("b", "No link", 10_000, 30, " ").joinable(10_000, 15));
        assert!(m.in_progress(10_000) && !m.in_progress(9_999));
    }

    #[test]
    fn the_earliest_joinable_one_is_picked() {
        let list = [meeting("late", "B", 20_000, 60, "u"), meeting("early", "A", 10_000, 120, "u"), meeting("nolink", "C", 9_000, 120, "")];
        assert_eq!(joinable_now(&list, 10_100, 15).unwrap().id, "early");
        assert!(joinable_now(&list, 1_000, 15).is_none());
    }

    #[test]
    fn duplicates_across_calendars_keep_the_one_with_a_link() {
        let mut outlook = meeting("o", "Standup ", 10_000, 30, "");
        outlook.provider = "outlook".into();
        let merged = merge(vec![outlook, meeting("g", "standup", 10_000, 30, "https://x"), meeting("z", "Later", 5_000, 10, "")]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].id, "z", "sorted by start");
        assert_eq!(merged[1].id, "g");
    }

    #[test]
    fn reminders_come_once_wait_when_snoozed_and_forget_removed_meetings() {
        let list = vec![meeting("a", "A", 10_000, 30, "u")];
        let mut state = ReminderState::default();
        assert!(state.due(&list, 10_000 - 6 * 60, 5).is_none(), "too early");
        assert!(state.due(&list, 10_000 - 5 * 60, 5).is_some());
        state.snooze(&list[0], 9_800, 5);
        assert!(state.due(&list, 9_900, 5).is_none(), "snoozed");
        assert!(state.due(&list, 10_100, 5).is_some(), "snooze over");
        state.dismiss(&list[0]);
        assert!(state.due(&list, 10_100, 5).is_none(), "dismissed");
        assert!(state.due(&list, 10_000 + 31 * 60, 5).is_none());
        state.prune(&[]);
        assert!(state.reminded.is_empty() && state.snoozed_until.is_empty());
    }

    #[test]
    fn words_for_the_screen() {
        let m = meeting("a", "Standup", 1_794_045_600, 60, "u"); // 2026-11-07 10:00 UTC
        assert_eq!(day_text(m.start, 0), "Saturday, Nov 7");
        assert_eq!(range_text(&m, 0), "Saturday, Nov 7 · 10:00 AM – 11:00 AM");
        assert_eq!(range_text(&m, 19_800), "Saturday, Nov 7 · 3:30 PM – 4:30 PM");
        assert_eq!(range_text(&Meeting { all_day: true, ..m.clone() }, 0), "Saturday, Nov 7 · All day");
        assert_eq!(reminder_text(&m, m.start - 5 * 60), "Standup starts in 5 minutes.");
        assert_eq!(reminder_text(&m, m.start - 30), "Standup starts in 1 minute.");
        assert_eq!(reminder_text(&m, m.start + 5), "Standup is starting now.");
    }
}
