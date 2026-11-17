//! "set volume to 30", "turn the volume up by 10", "lower the volume". Port of the phrases in
//! `Features/SystemControl/VolumeIntentParser.cs`.
//!
//! shortcut: no Core Audio. Windows moves the volume 2% per media key, so a level is reached by
//! 50 presses down and then N/2 up (an Automation of Shortcut steps). It shows the volume pop-up
//! and is not exact if the machine uses another step; upgrade to the endpoint volume API then.

use crate::actions::{ActionStep, ButtonAction};

const STEP_PERCENT: u32 = 2;
const VERBS: [&str; 12] = ["set", "make", "change", "increase", "decrease", "raise", "lower", "turn", "up", "down", "to", "windows"];
const NAMED: [(&str, u32); 13] = [
    ("zero", 0), ("ten", 10), ("twenty", 20), ("thirty", 30), ("forty", 40), ("fifty", 50),
    ("sixty", 60), ("seventy", 70), ("eighty", 80), ("ninety", 90), ("hundred", 100), ("max", 100), ("half", 50),
];

/// The action and a short label for a volume phrase, or `None` when the phrase is something else.
pub fn plan(normalized: &str) -> Option<(ButtonAction, String)> {
    let words: Vec<&str> = normalized.split(' ').collect();
    let at = words.iter().position(|w| *w == "volume")?;
    let (before, after) = (&words[..at], &words[at + 1..]);
    if !before.iter().all(|w| VERBS.contains(w) || matches!(*w, "the" | "my")) {
        return None;
    }
    let up = before.iter().any(|w| matches!(*w, "increase" | "raise" | "up")) || after.first() == Some(&"up");
    let down = before.iter().any(|w| matches!(*w, "decrease" | "lower" | "down")) || after.first() == Some(&"down");
    if up && down {
        return None;
    }

    let tail: Vec<&str> = after.iter().copied().filter(|w| !matches!(*w, "up" | "down")).collect();
    let by = tail.first() == Some(&"by");
    let value = match tail.iter().copied().skip_while(|w| matches!(*w, "to" | "by")).collect::<Vec<_>>().as_slice() {
        [] => None,
        [n] => Some(percent(n)?),
        [n, "percent"] => Some(percent(n)?),
        _ => return None,
    };

    match value {
        None if up || down => Some((presses(if up { "VolumeUp" } else { "VolumeDown" }, 1), if up { "VOLUME UP" } else { "VOLUME DOWN" }.into())),
        None => None,
        Some(0) if by => None,
        Some(n) if by && (up || down) => {
            let times = n.div_ceil(STEP_PERCENT);
            Some((presses(if up { "VolumeUp" } else { "VolumeDown" }, times), format!("VOLUME {}{n}%", if up { '+' } else { '-' })))
        }
        Some(_) if by => None,
        Some(n) => Some((set_to(n), format!("VOLUME {n}%"))),
    }
}

pub(super) fn percent(word: &str) -> Option<u32> {
    let digits = word.strip_suffix('%').unwrap_or(word);
    if !digits.is_empty() && digits.len() <= 3 && digits.bytes().all(|b| b.is_ascii_digit()) {
        return digits.parse::<u32>().ok().map(|n| n.min(100));
    }
    NAMED.iter().find(|(name, _)| *name == word).map(|(_, n)| *n)
}

fn key(name: &str) -> ActionStep {
    ActionStep { kind: "Shortcut".into(), value: name.into(), ..Default::default() }
}

fn presses(name: &str, times: u32) -> ButtonAction {
    if times == 1 {
        return ButtonAction { kind: "Shortcut".into(), value: name.into(), steps: vec![] };
    }
    ButtonAction { kind: "Automation".into(), value: String::new(), steps: (0..times).map(|_| key(name)).collect() }
}

fn set_to(percent: u32) -> ButtonAction {
    let mut steps: Vec<ActionStep> = (0..100 / STEP_PERCENT).map(|_| key("VolumeDown")).collect();
    steps.extend((0..percent.div_ceil(STEP_PERCENT).min(100 / STEP_PERCENT)).map(|_| key("VolumeUp")));
    ButtonAction { kind: "Automation".into(), value: String::new(), steps }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ups_downs(a: &ButtonAction) -> (usize, usize) {
        let n = |k: &str| a.steps.iter().filter(|s| s.value == k).count();
        (n("VolumeUp"), n("VolumeDown"))
    }

    #[test]
    fn a_level_is_all_the_way_down_then_up_to_it() {
        for text in ["set volume to 30", "set the volume to 30 percent", "volume 30%", "make my volume thirty", "increase the volume to 30"] {
            let (action, label) = plan(text).unwrap_or_else(|| panic!("{text}"));
            assert_eq!(ups_downs(&action), (15, 50), "{text}");
            assert_eq!(label, "VOLUME 30%");
        }
        assert_eq!(ups_downs(&plan("set volume to 100").unwrap().0), (50, 50));
        assert_eq!(ups_downs(&plan("set volume to 500").unwrap().0), (50, 50), "capped at 100");
        assert_eq!(ups_downs(&plan("set volume to 0").unwrap().0), (0, 50));
    }

    #[test]
    fn steps_are_relative() {
        let (a, label) = plan("turn the volume up by 10 percent").unwrap();
        assert_eq!(ups_downs(&a), (5, 0));
        assert_eq!(label, "VOLUME +10%");
        assert_eq!(ups_downs(&plan("lower volume by 6").unwrap().0), (0, 3));
        let (single, _) = plan("turn down the volume").unwrap();
        assert_eq!((single.kind.as_str(), single.value.as_str()), ("Shortcut", "VolumeDown"));
        assert_eq!(plan("volume up").unwrap().0.value, "VolumeUp");
    }

    #[test]
    fn other_sentences_are_left_alone() {
        for text in ["open volume mixer", "what is the volume of a cube", "set volume", "volume by 10", "raise lower volume", "set volume to 10 20", "lower volume by 0", "mute"] {
            assert!(plan(text).is_none(), "{text}");
        }
    }
}
