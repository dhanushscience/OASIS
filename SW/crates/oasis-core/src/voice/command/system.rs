//! "lock the computer", "switch to dark mode". Port of the lock and theme parts of
//! `Oasis.Core/SystemControl/SystemCommandPhrase.cs`. Every phrase must be the whole utterance,
//! so "lock the computer and open chrome" is not read as a lock.

use crate::actions::ActionKind;

/// The action `(kind, value)` and a label, or `None` when the phrase is something else.
pub fn plan(normalized: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let text = normalized.strip_prefix("please ").unwrap_or(normalized);
    if let Some(rest) = text.strip_prefix("lock ") {
        let rest = rest.strip_prefix("down ").unwrap_or(rest);
        let rest = ["the ", "my "].into_iter().find_map(|p| rest.strip_prefix(p)).unwrap_or(rest);
        if matches!(rest, "screen" | "computer" | "pc" | "laptop" | "workstation") {
            return Some((ActionKind::LockPc.as_str(), "lock", "LOCK PC"));
        }
        return None;
    }
    if let Some(mode) = text.strip_prefix("turn off ").or_else(|| text.strip_prefix("disable ")).or_else(|| text.strip_prefix("switch off ")) {
        return match mode {
            "dark mode" | "dark theme" => Some((ActionKind::Theme.as_str(), "light", "LIGHT MODE")),
            "light mode" | "light theme" => Some((ActionKind::Theme.as_str(), "dark", "DARK MODE")),
            _ => None,
        };
    }
    let mode = match text.strip_suffix(" on").unwrap_or(text) {
        "dark mode" => "dark",
        "light mode" => "light",
        t => {
            let t = ["switch", "change", "go", "turn", "enable", "use", "activate", "set"]
                .into_iter()
                .find_map(|v| t.strip_prefix(v))?
                .trim_start();
            let t = ["it ", "on ", "to ", "into "].into_iter().fold(t, |t, p| t.strip_prefix(p).unwrap_or(t));
            let t = t.strip_prefix("the ").unwrap_or(t);
            match t {
                "dark mode" | "dark theme" => "dark",
                "light mode" | "light theme" => "light",
                _ => return None,
            }
        }
    };
    Some((ActionKind::Theme.as_str(), mode, if mode == "dark" { "DARK MODE" } else { "LIGHT MODE" }))
}

/// "turn off wifi", "bluetooth on", "enable airplane mode": `wifi|off` and a label. Whole
/// utterance only. Turning Wi-Fi or Bluetooth off, or airplane mode on, is risky; see `safety`.
pub fn radio(normalized: &str) -> Option<(String, String)> {
    let text = normalized
        .replace("wi fi", "wifi")
        .replace("blue tooth", "bluetooth")
        .replace("air plane", "airplane")
        .replace("flight mode", "airplane mode");
    let text = text.strip_prefix("please ").unwrap_or(&text);
    let mut words = text.split(' ').peekable();
    let mut state = None;
    match words.peek().copied()? {
        verb @ ("enable" | "activate" | "disable" | "deactivate") => {
            state = Some(if verb.starts_with('d') { "off" } else { "on" });
            words.next();
        }
        "turn" | "switch" | "put" => {
            words.next();
        }
        _ => {}
    }
    if state.is_none() {
        if let Some(s @ ("on" | "off")) = words.peek().copied() {
            state = Some(s);
            words.next();
        }
    }
    if matches!(words.peek().copied(), Some("the" | "my")) {
        words.next();
    }
    let rest: Vec<&str> = words.collect();
    let (device, trailing) = match rest.as_slice() {
        ["wifi", t @ ..] => ("wifi", t),
        ["bluetooth", t @ ..] => ("bluetooth", t),
        ["airplane", "mode", t @ ..] => ("airplane", t),
        _ => return None,
    };
    let state = match (state, trailing) {
        (Some(s), []) => s,
        (None, ["on"]) => "on",
        (None, ["off"]) => "off",
        _ => return None,
    };
    let name = match device {
        "wifi" => "WI-FI",
        "bluetooth" => "BLUETOOTH",
        _ => "AIRPLANE MODE",
    };
    Some((format!("{device}|{state}"), format!("{name} {}", state.to_uppercase())))
}

const BRIGHTNESS_WORDS: [&str; 17] = [
    "set", "make", "change", "turn", "put", "adjust", "increase", "raise", "decrease", "lower", "reduce", "boost", "up", "down",
    "to", "at", "screen",
];

/// "set brightness to 70", "turn the brightness up by 20", "max brightness": the level as `70`,
/// or a change as `+10` / `-10`, with a label. Every phrase must be the whole utterance.
pub fn brightness(normalized: &str) -> Option<(String, String)> {
    let text = normalized.strip_prefix("please ").unwrap_or(normalized);
    let words: Vec<&str> = text.split(' ').collect();
    let at = words.iter().position(|w| *w == "brightness")?;
    let (before, after) = (&words[..at], &words[at + 1..]);
    let extreme = |names: &[&str]| before.last().is_some_and(|w| names.contains(w));
    let before: Vec<&str> = before.iter().copied().filter(|w| !matches!(*w, "the" | "my" | "display" | "monitor")).collect();

    let level = if extreme(&["max", "maximum", "full"]) {
        Some(100)
    } else if extreme(&["min", "minimum", "lowest"]) {
        Some(10)
    } else {
        None
    };
    if let Some(n) = level {
        return (after.is_empty() && before.len() == 1).then(|| (n.to_string(), format!("BRIGHTNESS {n}%")));
    }
    if !before.iter().all(|w| BRIGHTNESS_WORDS.contains(w)) {
        return None;
    }
    let up = before.iter().any(|w| matches!(*w, "increase" | "raise" | "boost" | "up")) || after.first() == Some(&"up");
    let down = before.iter().any(|w| matches!(*w, "decrease" | "lower" | "reduce" | "down")) || after.first() == Some(&"down");
    if up && down {
        return None;
    }
    let tail: Vec<&str> = after.iter().copied().filter(|w| !matches!(*w, "up" | "down")).collect();
    let by = tail.first() == Some(&"by");
    let value = match tail.iter().copied().skip_while(|w| matches!(*w, "to" | "at" | "by")).collect::<Vec<_>>().as_slice() {
        [] => None,
        [n] => Some(super::volume::percent(n)?),
        [n, "percent"] => Some(super::volume::percent(n)?),
        _ => return None,
    };
    let sign = if up { '+' } else { '-' };
    match value {
        None if up || down => Some((format!("{sign}10"), format!("BRIGHTNESS {sign}10%"))),
        Some(0) if by => None,
        Some(n) if by && (up || down) => Some((format!("{sign}{n}"), format!("BRIGHTNESS {sign}{n}%"))),
        Some(n) if !by => Some((n.to_string(), format!("BRIGHTNESS {n}%"))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_phrases() {
        let r = |t: &str| radio(t).map(|p| p.0);
        assert_eq!(r("turn off wifi").as_deref(), Some("wifi|off"));
        assert_eq!(r("turn off wi fi").as_deref(), Some("wifi|off"));
        assert_eq!(r("wifi on").as_deref(), Some("wifi|on"));
        assert_eq!(r("switch the bluetooth on").as_deref(), Some("bluetooth|on"));
        assert_eq!(r("disable my bluetooth").as_deref(), Some("bluetooth|off"));
        assert_eq!(r("enable airplane mode").as_deref(), Some("airplane|on"));
        assert_eq!(r("turn flight mode off").as_deref(), Some("airplane|off"));
        for t in ["wifi", "airplane mode", "turn off wifi and open chrome", "turn wifi", "turn on off wifi", "open wifi", "turn off the wifi router"] {
            assert!(r(t).is_none(), "{t}");
        }
    }

    #[test]
    fn brightness_levels_and_steps() {
        let b = |t: &str| brightness(t).map(|p| p.0);
        assert_eq!(b("set brightness to 70 percent").as_deref(), Some("70"));
        assert_eq!(b("brightness 40%").as_deref(), Some("40"));
        assert_eq!(b("set the screen brightness to fifty").as_deref(), Some("50"));
        assert_eq!(b("turn the brightness up").as_deref(), Some("+10"));
        assert_eq!(b("brightness down").as_deref(), Some("-10"));
        assert_eq!(b("lower the brightness by 25").as_deref(), Some("-25"));
        assert_eq!(b("increase brightness to 90").as_deref(), Some("90"));
        assert_eq!(b("max brightness").as_deref(), Some("100"));
        assert_eq!(b("minimum brightness").as_deref(), Some("10"));
    }

    #[test]
    fn brightness_ignores_other_sentences() {
        for t in ["brightness", "what is the brightness of the sun", "set brightness", "brightness by 10", "lower brightness by 0", "open brightness", "max brightness now"] {
            assert!(brightness(t).is_none(), "{t}");
        }
    }

    #[test]
    fn lock_and_theme_phrases() {
        assert_eq!(plan("lock the computer").map(|p| p.0), Some("LockPc"));
        assert_eq!(plan("please lock down my pc").map(|p| p.0), Some("LockPc"));
        assert_eq!(plan("switch to dark mode"), Some(("Theme", "dark", "DARK MODE")));
        assert_eq!(plan("dark mode"), Some(("Theme", "dark", "DARK MODE")));
        assert_eq!(plan("turn on the light theme").map(|p| p.1), Some("light"));
        assert_eq!(plan("turn off dark mode").map(|p| p.1), Some("light"));
        assert_eq!(plan("enable dark mode").map(|p| p.1), Some("dark"));
    }

    #[test]
    fn longer_sentences_are_left_alone() {
        for text in ["lock the computer and open chrome", "lock the door", "lock", "switch to dark mode in chrome", "turn up the volume", "set the light"] {
            assert!(plan(text).is_none(), "{text}");
        }
    }
}
