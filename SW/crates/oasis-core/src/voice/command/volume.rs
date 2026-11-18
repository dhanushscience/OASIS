//! "set volume to 30", "turn the volume up by 10", "lower the volume". Port of the phrases in
//! `Features/SystemControl/VolumeIntentParser.cs`.
//!
//! A level or a change of N% is one `Volume` action (`70`, `+10`, `-10`): the host reads the current
//! volume and sets the new one straight away, with no slide through zero. A bare "volume up" or
//! "volume down" is still one media key.

use crate::actions::{ActionKind, ButtonAction};

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
        None if up || down => Some((key(if up { "VolumeUp" } else { "VolumeDown" }), if up { "VOLUME UP" } else { "VOLUME DOWN" }.into())),
        None => None,
        Some(0) if by => None,
        Some(n) if by && (up || down) => {
            let sign = if up { '+' } else { '-' };
            Some((level(format!("{sign}{n}")), format!("VOLUME {sign}{n}%")))
        }
        Some(_) if by => None,
        Some(n) => Some((level(n.to_string()), format!("VOLUME {n}%"))),
    }
}

pub(super) fn percent(word: &str) -> Option<u32> {
    let digits = word.strip_suffix('%').unwrap_or(word);
    if !digits.is_empty() && digits.len() <= 3 && digits.bytes().all(|b| b.is_ascii_digit()) {
        return digits.parse::<u32>().ok().map(|n| n.min(100));
    }
    NAMED.iter().find(|(name, _)| *name == word).map(|(_, n)| *n)
}

fn key(name: &str) -> ButtonAction {
    ButtonAction { kind: ActionKind::Shortcut.as_str().into(), value: name.into(), steps: vec![] }
}

fn level(value: String) -> ButtonAction {
    ButtonAction { kind: ActionKind::Volume.as_str().into(), value, steps: vec![] }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(text: &str) -> (String, String) {
        let (action, label) = plan(text).unwrap_or_else(|| panic!("{text}"));
        assert_eq!(action.kind, "Volume", "{text}");
        (action.value, label)
    }

    #[test]
    fn a_level_is_one_direct_set() {
        for text in ["set volume to 30", "set the volume to 30 percent", "volume 30%", "make my volume thirty", "increase the volume to 30"] {
            assert_eq!(volume(text), ("30".into(), "VOLUME 30%".into()), "{text}");
        }
        assert_eq!(volume("set volume to 500").0, "100", "capped at 100");
        assert_eq!(volume("set volume to 0").0, "0");
    }

    #[test]
    fn steps_are_relative() {
        assert_eq!(volume("turn the volume up by 10 percent"), ("+10".into(), "VOLUME +10%".into()));
        assert_eq!(volume("lower volume by 6").0, "-6");
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
