//! "set a timer for 10 minutes", "set an alarm for 7:30 am", "remind me to stretch in 20 minutes",
//! "cancel my timers". Port of the alarm, timer, reminder and cancel parts of
//! `Oasis.Core/SystemControl/SystemCommandPhrase.cs`.
//!
//! The result is a `Reminder` action, `kind|due|label` (kind `alarm`, `timer` or `reminder`, due
//! as seconds since 1970), or `CancelReminders` with `alarm`, `timer`, `reminder` or `all`. The
//! engine keeps them; see `oasis-engine/src/reminders.rs`.
//!
//! Known gap: a time like "7:30" arrives as "7 30" (the router drops punctuation), which is read
//! the same way. "Tomorrow", dates and "1.5 hours" are not understood; the phrase is then left alone.

use crate::speech::numbers;

/// The moment a phrase was said, as the clock on the wall shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalTime {
    /// Seconds since 1970-01-01 UTC.
    pub unix: i64,
    /// Local time minus UTC, in seconds (the hour of the day comes from this).
    pub offset_secs: i32,
}

const MAX_LABEL: usize = 120;
const DAY: i64 = 86_400;

/// The action `(kind, value)` and a short label, or `None` when the phrase is something else.
pub fn plan(normalized: &str, now: LocalTime) -> Option<(&'static str, String, String)> {
    let text = clean(normalized);
    let words: Vec<&str> = text.split(' ').filter(|w| !w.is_empty()).collect();
    let words = words.strip_prefix(&["please"]).unwrap_or(&words);
    if words.is_empty() || words.contains(&"oasis") {
        return None;
    }
    if let Some(what) = cancel(words) {
        let label = if what == "all" { "CANCEL ALL".to_string() } else { format!("CANCEL {}S", what.to_uppercase()) };
        return Some(("CancelReminders", what.to_string(), label));
    }
    let (kind, due, label) = alarm(words, now).or_else(|| reminder(words, now)).or_else(|| timer(words, now))?;
    let value = format!("{kind}|{due}|{}", flat(&label));
    Some(("Reminder", value, describe(kind, due, &label, now)))
}

/// Number words become digits, and "a m" / "p m" become "am" / "pm".
fn clean(normalized: &str) -> String {
    let mut text = numbers::apply(normalized).to_lowercase();
    for (spoken, written) in [(" a m", " am"), (" p m", " pm")] {
        text = text.replace(spoken, written);
    }
    text.replace('%', " ")
}

fn cancel<'a>(words: &[&'a str]) -> Option<&'a str> {
    let (verb, rest) = words.split_first()?;
    let rest = match (*verb, rest) {
        ("turn", ["off", r @ ..]) => r,
        ("cancel" | "delete" | "clear" | "remove" | "dismiss" | "stop", r) => r,
        _ => return None,
    };
    let rest: &[&str] = &rest[rest.iter().take_while(|w| matches!(**w, "all" | "my" | "the" | "every" | "any")).count()..];
    match rest {
        [what] => {
            let what = what.strip_suffix('s').unwrap_or(what);
            match what {
                "alarm" | "timer" | "reminder" => Some(what),
                "notification" => Some("reminder"),
                _ => None,
            }
        }
        _ => None,
    }
}

type Found = (&'static str, i64, String);

fn alarm(words: &[&str], now: LocalTime) -> Option<Found> {
    let rest = match words {
        ["wake", "me", "up", r @ ..] => r,
        [verb, r @ ..] if matches!(*verb, "set" | "create" | "make" | "add" | "start" | "schedule") => {
            let r = r.strip_prefix(&["me"]).unwrap_or(r);
            let r = if matches!(r.first(), Some(&"a" | &"an" | &"the" | &"my")) { &r[1..] } else { r };
            r.strip_prefix(&["alarm"])?
        }
        _ => return None,
    };
    let (due, label) = when(rest, now)?;
    Some(("alarm", due, label))
}

fn timer(words: &[&str], now: LocalTime) -> Option<Found> {
    let at = words.iter().position(|w| *w == "timer")?;
    let mut before = &words[..at];
    if before.first().is_some_and(|v| matches!(*v, "set" | "start" | "create" | "make" | "begin")) {
        before = &before[1..];
        before = before.strip_prefix(&["me"]).unwrap_or(before);
        if matches!(before.first(), Some(&"a" | &"an" | &"the")) {
            before = &before[1..];
        }
    }
    let mut after = &words[at + 1..];
    if matches!(after.first(), Some(&"for" | &"of")) {
        after = &after[1..];
    }
    let source = if after.is_empty() { before } else { after };
    let (span, used) = duration(source)?;
    if after.is_empty() && used < source.len() {
        return None;
    }
    let seconds = span.clamp(1, DAY);
    Some(("timer", now.unix + seconds, label_of(&source[used..])))
}

fn reminder(words: &[&str], now: LocalTime) -> Option<Found> {
    let rest = match words {
        [verb, r @ ..] if matches!(*verb, "set" | "create" | "add" | "make") => {
            let r = r.strip_prefix(&["me"]).unwrap_or(r);
            let r = if matches!(r.first(), Some(&"a" | &"an" | &"the" | &"my")) { &r[1..] } else { r };
            match r {
                [what, rest @ ..] if matches!(*what, "reminder" | "notification") => rest,
                _ => return None,
            }
        }
        [verb, "me", r @ ..] if matches!(*verb, "remind" | "notify" | "alert" | "ping") => r,
        ["send", "me", a, what, r @ ..] if matches!(*a, "a" | "an") && matches!(*what, "reminder" | "notification") => r,
        _ => return None,
    };
    if rest.is_empty() {
        return None;
    }
    if let Some((due, label)) = when(rest, now) {
        return Some(("reminder", due, if label.is_empty() { "Reminder".into() } else { label }));
    }
    // "remind me to drink water in 20 minutes": the time is the tail, so try each in/after/at/by.
    let body = strip_lead(rest);
    for j in 1..body.len().saturating_sub(1) {
        if !matches!(body[j], "in" | "after" | "at" | "by") {
            continue;
        }
        let start = if body[j - 1] == "tomorrow" { j - 1 } else { j };
        if let Some((due, extra)) = when(&body[start..], now) {
            if extra.is_empty() && start > 0 {
                return Some(("reminder", due, body[..start].join(" ")));
            }
        }
    }
    None
}

/// "in 10 minutes", "for 7 30 am", "at 6 to go for a run": the moment, then an optional label.
fn when(words: &[&str], now: LocalTime) -> Option<(i64, String)> {
    // "tomorrow" may sit before the time or after it; it moves a clock time to the next day.
    let tomorrow = words.contains(&"tomorrow");
    let kept: Vec<&str> = words.iter().copied().filter(|w| *w != "tomorrow").collect();
    let words = &kept[..];
    let lead = match words {
        ["to", "ring", "at", ..] => 3,
        ["to", "go", "off", "at", ..] => 4,
        [w, ..] if matches!(*w, "in" | "after" | "for" | "at" | "by" | "around") => 1,
        _ => 0,
    };
    // "for tomorrow at 7": a second lead word once "tomorrow" is out of the way.
    let after_lead = if lead > 0 && matches!(words.get(lead), Some(&"at" | &"by" | &"around")) { lead + 1 } else { lead };
    if matches!(words.first(), Some(&"in" | &"after" | &"for")) {
        if let Some((span, used)) = duration(&words[lead..]) {
            return Some((now.unix + span, label_of(&words[lead + used..])));
        }
    }
    let (due, used) = clock(&words[after_lead..], now, tomorrow)?;
    Some((due, label_of(&words[after_lead + used..])))
}

/// "10 minutes", "an hour and a half", "1 hour 30 minutes": seconds and how many words were read.
fn duration(words: &[&str]) -> Option<(i64, usize)> {
    let (mut total, mut i) = (0.0f64, 0);
    loop {
        let mut j = i;
        if matches!(words.get(j), Some(&"and" | &"plus")) {
            j += 1;
        }
        let amount = match words.get(j).copied() {
            Some("a" | "an") => 1.0,
            Some("half") => {
                if matches!(words.get(j + 1), Some(&"a" | &"an")) {
                    j += 1;
                }
                0.5
            }
            Some(n) if n.bytes().all(|b| b.is_ascii_digit()) && !n.is_empty() => n.parse::<f64>().ok()?,
            _ => break,
        };
        let unit = match words.get(j + 1).copied() {
            Some("hour" | "hours" | "hr" | "hrs") => 3600.0,
            Some("minute" | "minutes" | "min" | "mins") => 60.0,
            Some("second" | "seconds" | "sec" | "secs") => 1.0,
            _ => break,
        };
        j += 2;
        let mut extra = 0.0;
        if words.get(j..j + 3) == Some(&["and", "a", "half"]) {
            extra = 0.5;
            j += 3;
        }
        total += unit * (amount + extra);
        i = j;
    }
    (total > 0.0).then_some((total as i64, i))
}

/// "noon", "7 30 pm", "730", "6": the next time the clock shows it, and how many words were read.
fn clock(words: &[&str], now: LocalTime, tomorrow: bool) -> Option<(i64, usize)> {
    let digits = |w: &str, len: std::ops::RangeInclusive<usize>| len.contains(&w.len()) && w.bytes().all(|b| b.is_ascii_digit());
    let (hour, minute, mut used, word) = match words.first().copied()? {
        "noon" => (12, 0, 1, true),
        "midnight" => (0, 0, 1, true),
        w if digits(w, 3..=4) => {
            let n: i64 = w.parse().ok()?;
            (n / 100, n % 100, 1, false)
        }
        w if digits(w, 1..=2) => match words.get(1).copied() {
            Some(m) if digits(m, 2..=2) => (w.parse().ok()?, m.parse().ok()?, 2, false),
            _ => (w.parse().ok()?, 0, 1, false),
        },
        _ => return None,
    };
    let meridiem = words.get(used).copied().filter(|w| matches!(*w, "am" | "pm"));
    if meridiem.is_some() {
        used += 1;
    }
    if minute > 59 || hour > 23 || (meridiem.is_some() && (hour == 0 || hour > 12)) {
        return None;
    }
    let hours: Vec<i64> = match meridiem {
        Some("am") => vec![hour % 12],
        Some(_) => vec![hour % 12 + 12],
        None if word || hour == 0 || hour > 12 => vec![hour],
        None => vec![hour % 12, hour % 12 + 12],
    };
    let local = now.unix + i64::from(now.offset_secs);
    let midnight = local.div_euclid(DAY) * DAY - i64::from(now.offset_secs);
    let due = hours
        .into_iter()
        .map(|h| {
            let today = midnight + h * 3600 + minute * 60;
            if tomorrow {
                today + DAY
            } else if today > now.unix {
                today
            } else {
                today + DAY
            }
        })
        .min()?;
    Some((due, used))
}

/// "to call mom" / "called gym" / "about the report" → the words after.
fn label_of(words: &[&str]) -> String {
    let words = strip_lead(words);
    let words = if matches!(words.first(), Some(&"called" | &"named" | &"labeled" | &"labelled" | &"titled")) { &words[1..] } else { words };
    words.join(" ")
}

fn strip_lead<'a, 'b>(words: &'b [&'a str]) -> &'b [&'a str] {
    if matches!(words.first(), Some(&"to" | &"about" | &"that" | &"for" | &"saying")) { &words[1..] } else { words }
}

/// One line without the separators the stored form uses, at most 120 characters.
pub fn flat(text: &str) -> String {
    let one: String = text.chars().map(|c| if matches!(c, '|' | '\r' | '\n' | '\t') { ' ' } else { c }).collect();
    one.trim().chars().take(MAX_LABEL).collect()
}

/// A short line for the screen: "Timer for 10 minutes", "Alarm for 7:30 AM (gym)".
pub fn describe(kind: &str, due: i64, label: &str, now: LocalTime) -> String {
    match kind {
        "timer" => format!("Timer for {}{}", spoken(due - now.unix), if label.is_empty() { String::new() } else { format!(": {label}") }),
        "alarm" => format!("Alarm for {}{}", clock_text(due, now.offset_secs), if label.is_empty() { String::new() } else { format!(" ({label})") }),
        _ => format!("Reminder at {}{}", clock_text(due, now.offset_secs), if label.is_empty() { String::new() } else { format!(": {label}") }),
    }
}

/// "1 hour 30 minutes".
pub fn spoken(seconds: i64) -> String {
    let (h, m, s) = (seconds / 3600, seconds % 3600 / 60, seconds % 60);
    let plural = |n: i64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    let mut parts = Vec::new();
    if h > 0 {
        parts.push(plural(h, "hour"));
    }
    if m > 0 {
        parts.push(plural(m, "minute"));
    }
    if s > 0 || parts.is_empty() {
        parts.push(plural(s, "second"));
    }
    parts.join(" ")
}

/// "7:30 AM" for a moment, on the clock at `offset_secs` from UTC.
pub fn clock_text(unix: i64, offset_secs: i32) -> String {
    let minutes = (unix + i64::from(offset_secs)).rem_euclid(DAY) / 60;
    let (h, m) = (minutes / 60, minutes % 60);
    format!("{}:{m:02} {}", if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-11-07 10:00:00 UTC, on a clock 2 hours ahead (so it is 12:00 there).
    const NOW: LocalTime = LocalTime { unix: 1_794_045_600, offset_secs: 7200 };

    fn run(text: &str) -> Option<(&'static str, String, String)> {
        plan(&crate::voice::command::text::normalize(text), NOW)
    }

    fn at(hour: i64, minute: i64, days: i64) -> i64 {
        let local_midnight = (NOW.unix + 7200).div_euclid(DAY) * DAY - 7200;
        local_midnight + days * DAY + hour * 3600 + minute * 60
    }

    #[test]
    fn timers() {
        let (kind, value, label) = run("set a timer for 10 minutes").unwrap();
        assert_eq!(kind, "Reminder");
        assert_eq!(value, format!("timer|{}|", NOW.unix + 600));
        assert_eq!(label, "Timer for 10 minutes");
        assert_eq!(run("set a 5 minute timer").unwrap().1, format!("timer|{}|", NOW.unix + 300));
        assert_eq!(run("timer for an hour and a half").unwrap().1, format!("timer|{}|", NOW.unix + 5400));
        assert_eq!(run("start a timer for twenty five minutes").unwrap().1, format!("timer|{}|", NOW.unix + 1500));
        assert_eq!(run("timer for 1 hour 30 minutes").unwrap().1, format!("timer|{}|", NOW.unix + 5400));
        assert_eq!(run("set a timer for half an hour").unwrap().1, format!("timer|{}|", NOW.unix + 1800));
        assert_eq!(run("set a timer for 10 minutes called pasta").unwrap().1, format!("timer|{}|pasta", NOW.unix + 600));
    }

    #[test]
    fn alarms_pick_the_next_time_the_clock_shows() {
        assert_eq!(run("set an alarm for 7 30 am").unwrap().1, format!("alarm|{}|", at(7, 30, 1)), "already past today");
        assert_eq!(run("set an alarm for 5 pm").unwrap().1, format!("alarm|{}|", at(17, 0, 0)));
        assert_eq!(run("wake me up at 6 am").unwrap().1, format!("alarm|{}|", at(6, 0, 1)));
        assert_eq!(run("set an alarm for noon").unwrap().1, format!("alarm|{}|", at(12, 0, 1)), "12:00 is not after 12:00");
        assert_eq!(run("set an alarm for 6").unwrap().1, format!("alarm|{}|", at(18, 0, 0)), "6 pm comes before 6 am tomorrow");
        assert_eq!(run("set an alarm for 25"), None);
        assert_eq!(run("set an alarm for 13 pm"), None);
        assert_eq!(run("set an alarm in 2 hours").unwrap().1, format!("alarm|{}|", NOW.unix + 7200));
        assert_eq!(run("set an alarm for 5 pm").unwrap().2, "Alarm for 5:00 PM");
    }

    #[test]
    fn tomorrow_moves_a_clock_time_to_the_next_day() {
        assert_eq!(run("set an alarm for 5 pm tomorrow").unwrap().1, format!("alarm|{}|", at(17, 0, 1)));
        assert_eq!(run("set an alarm for tomorrow at 7 am").unwrap().1, format!("alarm|{}|", at(7, 0, 1)));
        assert_eq!(run("remind me tomorrow at 9 am to call mom").unwrap().1, format!("reminder|{}|call mom", at(9, 0, 1)));
        assert_eq!(run("remind me to call mom tomorrow at 9 am").unwrap().1, format!("reminder|{}|call mom", at(9, 0, 1)));
    }

    #[test]
    fn reminders_take_their_words_either_way_round() {
        let (_, value, label) = run("remind me in 20 minutes to drink water").unwrap();
        assert_eq!(value, format!("reminder|{}|drink water", NOW.unix + 1200));
        assert_eq!(label, "Reminder at 12:20 PM: drink water");
        assert_eq!(run("remind me to call mom at 5 pm").unwrap().1, format!("reminder|{}|call mom", at(17, 0, 0)));
        assert_eq!(run("set a reminder for 3 pm to send the report").unwrap().1, format!("reminder|{}|send the report", at(15, 0, 0)));
        assert_eq!(run("remind me in 5 minutes").unwrap().1, format!("reminder|{}|Reminder", NOW.unix + 300));
        assert_eq!(run("remind me to stretch"), None, "no time given");
    }

    #[test]
    fn cancelling() {
        assert_eq!(run("cancel my timers").unwrap(), ("CancelReminders", "timer".into(), "CANCEL TIMERS".into()));
        assert_eq!(run("delete all alarms").unwrap().1, "alarm");
        assert_eq!(run("clear the notification").unwrap().1, "reminder");
        assert_eq!(run("cancel the meeting"), None);
    }

    #[test]
    fn ordinary_sentences_are_left_alone() {
        for text in ["what is a timer", "set the table", "the alarm clock is loud", "set an alarm", "timer", "set a timer for pasta", "alarm for oasis at 5"] {
            assert!(run(text).is_none(), "{text}");
        }
    }

    #[test]
    fn labels_cannot_carry_the_separator() {
        assert_eq!(flat("a|b\nc"), "a b c");
        assert_eq!(flat(&"x".repeat(300)).len(), 120);
    }

    #[test]
    fn clock_text_is_twelve_hour() {
        assert_eq!(clock_text(at(0, 5, 0), 7200), "12:05 AM");
        assert_eq!(clock_text(at(12, 0, 0), 7200), "12:00 PM");
        assert_eq!(clock_text(at(23, 59, 0), 7200), "11:59 PM");
    }
}
