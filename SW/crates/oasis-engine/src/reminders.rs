//! Alarms, timers and reminders set by voice. Port of `Features/SystemControl/ReminderScheduler.cs`.
//!
//! They live in OASIS rather than the Windows Clock app, which has no way to create one from
//! outside. Each is a due time and a label, saved to `reminders.txt` so they survive a restart, and
//! checked against the wall clock (so a sleep or clock change cannot make one late for long). One
//! that came due while OASIS was closed fires on the next start, marked as missed.
//!
//! The saved form is one line per item, `kind<TAB>due<TAB>label`, so no JSON library is needed.

use std::path::PathBuf;

use oasis_core::voice::command::reminders::flat;

/// Older than this when it fires counts as missed.
const LATE_AFTER_SECS: i64 = 120;
/// More than this many waiting at once is refused, so a loop of phrases cannot fill the file.
const MAX_ITEMS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// `alarm`, `timer` or `reminder`.
    pub kind: String,
    pub due: i64,
    pub label: String,
}

impl Item {
    /// What to show and say when it fires: a title and a body.
    pub fn message(&self, now: i64) -> (String, String) {
        let label = &self.label;
        let (title, mut body) = match self.kind.as_str() {
            "alarm" => ("OASIS — Alarm", if label.is_empty() { "Your alarm is going off.".to_string() } else { label.clone() }),
            "timer" => ("OASIS — Timer", if label.is_empty() { "Your timer is done.".to_string() } else { format!("Timer done: {label}") }),
            _ => ("OASIS — Reminder", if label.is_empty() { "Reminder".to_string() } else { label.clone() }),
        };
        if now - self.due > LATE_AFTER_SECS {
            body = format!("Missed: {body}");
        }
        (title.to_string(), body)
    }

    /// How many chimes: a reminder one, the others four.
    pub fn chimes(&self) -> u32 {
        if self.kind == "reminder" { 1 } else { 4 }
    }
}

pub struct Reminders {
    items: Vec<Item>,
    path: PathBuf,
}

impl Reminders {
    /// Loads what was saved; a missing or damaged file is an empty list.
    pub fn load(path: PathBuf) -> Reminders {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let items = text.lines().filter_map(parse_line).take(MAX_ITEMS).collect();
        Reminders { items, path }
    }

    /// `false` when there are already too many.
    pub fn add(&mut self, kind: &str, due: i64, label: &str) -> bool {
        if self.items.len() >= MAX_ITEMS {
            return false;
        }
        self.items.push(Item { kind: kind.to_string(), due, label: flat(label) });
        self.save();
        true
    }

    /// Removes `alarm`, `timer` or `reminder` (anything else: all of them); returns how many.
    pub fn cancel(&mut self, what: &str) -> usize {
        let before = self.items.len();
        self.items.retain(|i| matches!(what, "alarm" | "timer" | "reminder") && i.kind != what);
        let removed = before - self.items.len();
        if removed > 0 {
            self.save();
        }
        removed
    }

    /// Takes out and returns everything due at `now`.
    pub fn take_due(&mut self, now: i64) -> Vec<Item> {
        let (due, waiting): (Vec<Item>, Vec<Item>) = self.items.drain(..).partition(|i| i.due <= now);
        self.items = waiting;
        if !due.is_empty() {
            self.save();
        }
        due
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn save(&self) {
        let text: String = self.items.iter().map(|i| format!("{}\t{}\t{}\n", i.kind, i.due, i.label)).collect();
        // A failed save only means the list is lost on a restart; the timers still run now.
        let _ = std::fs::write(&self.path, text);
    }
}

fn parse_line(line: &str) -> Option<Item> {
    let mut parts = line.splitn(3, '\t');
    let kind = parts.next().filter(|k| matches!(*k, "alarm" | "timer" | "reminder"))?;
    let due = parts.next()?.parse::<i64>().ok().filter(|d| *d > 0)?;
    Some(Item { kind: kind.to_string(), due, label: flat(parts.next().unwrap_or("")) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oasis-reminders-{name}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("reminders.txt");
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn items_survive_a_restart_and_fire_once() {
        let path = file("restart");
        let mut r = Reminders::load(path.clone());
        assert!(r.add("timer", 1000, "pasta") && r.add("alarm", 5000, ""));
        let mut again = Reminders::load(path);
        assert_eq!(again.len(), 2);
        let due = again.take_due(1500);
        assert_eq!(due, vec![Item { kind: "timer".into(), due: 1000, label: "pasta".into() }]);
        assert!(again.take_due(1500).is_empty(), "fires only once");
        assert_eq!(again.len(), 1);
    }

    #[test]
    fn cancel_by_kind_or_all() {
        let mut r = Reminders::load(file("cancel"));
        r.add("timer", 10, "");
        r.add("timer", 20, "");
        r.add("alarm", 30, "");
        assert_eq!(r.cancel("timer"), 2);
        assert_eq!(r.cancel("reminder"), 0);
        assert_eq!(r.cancel("all"), 1);
        assert!(r.is_empty());
    }

    #[test]
    fn a_late_one_says_missed() {
        let item = Item { kind: "timer".into(), due: 1000, label: "pasta".into() };
        assert_eq!(item.message(1010), ("OASIS — Timer".into(), "Timer done: pasta".into()));
        assert_eq!(item.message(5000).1, "Missed: Timer done: pasta");
    }

    #[test]
    fn a_damaged_file_loses_only_its_bad_lines() {
        let path = file("damaged");
        std::fs::write(&path, "timer\t100\tok\nnonsense\ntimer\tabc\tx\nrocket\t5\tx\nalarm\t200\t\n").unwrap();
        assert_eq!(Reminders::load(path).len(), 2);
    }

    #[test]
    fn there_is_a_limit() {
        let mut r = Reminders::load(file("limit"));
        for i in 0..MAX_ITEMS {
            assert!(r.add("timer", i as i64 + 1, ""));
        }
        assert!(!r.add("timer", 1, ""));
    }
}
