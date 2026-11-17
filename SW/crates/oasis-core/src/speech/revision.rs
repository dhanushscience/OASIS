//! Mid-utterance self-corrections: "meet at 5 actually 6pm" → "meet at 6pm". Port of
//! `RevisionRewriter.cs`. Markers that are ordinary English ("wait for me") are left alone.
//!
//! Matching is on an ASCII-lower-cased copy, which keeps byte offsets identical to the original.

use super::text::{collapse_whitespace, words};
use super::CleanupMode;

const ALWAYS_MARKERS: [&str; 8] =
    ["scratch that", "never mind", "nevermind", "forget that", "no wait", "i meant", "i mean", "actually"];

const SCRATCH_THAT: &str = "scratch that";

const RESTART_STARTS: [&str; 12] = [
    "let's ", "lets ", "i ", "we ", "open ", "close ", "play ", "set ", "please ", "sorry ", "no ", "start ",
];

/// Rewrite revisions for `mode`.
pub fn apply_for(text: &str, mode: CleanupMode) -> String {
    match mode {
        CleanupMode::Dictate => apply_dictation(text),
        CleanupMode::Command => apply(text),
    }
}

/// Dictation reads "actually", "I mean" and "never mind" as the ordinary English they usually are:
/// "it was actually good" must not become "good". Only two edits are safe to infer from prose:
/// "scratch that" drops the sentence before it, and any marker directly followed by a time or
/// number swaps the last one ("at 5 actually 6pm").
fn apply_dictation(text: &str) -> String {
    let mut current = collapse_whitespace(text);
    let mut search_from = 0;
    for _ in 0..16 {
        if current.is_empty() {
            break;
        }
        let Some((index, marker)) = find_marker(&current, search_from) else { return current };

        let before = current[..index].trim_end();
        let after = current[index + marker.len()..]
            .trim_start()
            .trim_start_matches([',', '.', ';'])
            .trim_start();

        let next = if marker == SCRATCH_THAT {
            Some(collapse_whitespace(&format!("{} {}", drop_last_sentence(before), after)))
        } else if is_time_or_number(after) {
            replace_last_time_or_number_token(before, after)
                .map(|r| r.trim_end().trim_end_matches([',', ';']).to_string()) // the comma led into the marker
        } else if matches!(marker, "actually" | "i mean" | "i meant") {
            swap_same_kind(before, after)
        } else {
            None
        };

        match next {
            Some(n) if n != current => {
                current = n;
                search_from = 0;
            }
            _ => search_from = index + marker.len(),
        }
    }
    current
}

/// Earliest revision marker (or "wait") at or after `start`; at the same spot the longer marker.
fn find_marker(text: &str, start: usize) -> Option<(usize, &'static str)> {
    let lower = text.to_ascii_lowercase();
    let mut best: Option<(usize, &'static str)> = None;
    for candidate in ALWAYS_MARKERS.iter().copied().chain(["wait"]) {
        let Some(idx) = index_of_word_phrase(&lower, candidate, start) else { continue };
        if best.is_none_or(|(bi, bm)| idx < bi || (idx == bi && candidate.len() > bm.len())) {
            best = Some((idx, candidate));
        }
    }
    best
}

/// Everything up to and including the last sentence terminator; "" if there is none.
fn drop_last_sentence(before: &str) -> &str {
    // Strip the dropped sentence's own terminator first, or it would be the one found.
    let trimmed = before.trim_end().trim_end_matches([',', ';', '.', '!', '?']);
    trimmed.rfind(['.', '!', '?']).map_or("", |cut| &trimmed[..=cut])
}

/// "Use ESP32, actually nRF52": after a comma, one word that is the same kind as the word before
/// the comma (day, part number, name) replaces it. shortcut: one-word replacements only.
fn swap_same_kind(before: &str, after: &str) -> Option<String> {
    let head = before.strip_suffix(',')?;
    let new = after.trim_end_matches(['.', '!', '?']);
    if new.is_empty() || new.contains(char::is_whitespace) {
        return None;
    }
    let mut toks = super::text::tokens(head);
    let last = toks.pop()?;
    let kind = |w: &str| -> u8 {
        const DAYS: [&str; 7] = ["monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday"];
        let w = w.trim_matches(|c: char| !c.is_alphanumeric());
        if DAYS.contains(&w.to_ascii_lowercase().as_str()) {
            1
        } else if w.chars().any(|c| c.is_ascii_digit()) && w.chars().any(|c| c.is_ascii_alphabetic()) {
            2
        } else if w.chars().next().is_some_and(char::is_uppercase) && w.chars().all(char::is_alphabetic) {
            3
        } else {
            0
        }
    };
    let k = kind(&last);
    // A name needs a lead-in word, so a sentence-initial "Call, actually Bob" stays put.
    if k == 0 || k != kind(new) || (k == 3 && toks.is_empty()) {
        return None;
    }
    toks.push(after.trim().to_string());
    Some(toks.join(" "))
}

/// Token version of [`replace_last_time_or_number`] that keeps punctuation.
fn replace_last_time_or_number_token(before: &str, replacement: &str) -> Option<String> {
    let mut toks = super::text::tokens(before);
    for i in (0..toks.len()).rev() {
        let closing = super::text::trailing_punctuation(&toks[i]).to_string();
        let core = toks[i][..toks[i].len() - closing.len()].to_string();
        if !is_time_or_number(&core) && !is_bare_hour(&core) {
            continue;
        }
        toks[i] = format!("{}{closing}", replacement.trim().trim_end_matches(['.', ',', ';']));
        return Some(toks.join(" "));
    }
    None
}

/// Apply revisions the command way: every marker is a correction.
pub fn apply(text: &str) -> String {
    let mut current = collapse_whitespace(text);
    if current.is_empty() {
        return String::new();
    }
    for _ in 0..8 {
        let Some((before, after)) = try_split_first_marker(&current) else { return current };
        let combined = combine(&before, &after);
        if combined.eq_ignore_ascii_case(&current) {
            return current;
        }
        current = combined;
    }
    current
}

/// Split `text` at its first revision marker into what came before and after it.
pub fn try_split_first_marker(text: &str) -> Option<(String, String)> {
    let lower = text.to_ascii_lowercase();
    let mut best: Option<usize> = None;
    let mut marker_len = 0;
    for marker in ALWAYS_MARKERS {
        let Some(idx) = index_of_word_phrase(&lower, marker, 0) else { continue };
        if best.is_none_or(|b| idx < b) {
            best = Some(idx);
            marker_len = marker.len();
        }
    }
    if let Some(wait) = index_of_wait_revision(&lower) {
        if best.is_none_or(|b| wait < b) {
            best = Some(wait);
            marker_len = 4;
        }
    }
    let best = best?;

    let before = text[..best].trim_end().to_string();
    let mut after = text[best + marker_len..].trim_start();
    if after.starts_with([',', '.', ';']) {
        after = after[1..].trim_start();
    }
    (!after.is_empty() || !before.is_empty()).then(|| (before, after.to_string()))
}

fn combine(before: &str, after: &str) -> String {
    let after = collapse_whitespace(after);
    if after.is_empty() {
        return before.to_string();
    }
    if before.is_empty() {
        return after;
    }
    if is_time_or_number(&after) {
        if let Some(replaced) = replace_last_time_or_number(before, &after) {
            return replaced;
        }
    }
    if looks_like_restart(&after) {
        return after;
    }
    if words(&after).len() <= 6 {
        return replace_last_clause(before, &after);
    }
    after
}

/// A time ("5", "5:30", "6pm", "noon") or a short number.
pub fn is_time_or_number(text: &str) -> bool {
    let t = text.trim().trim_end_matches(['.', ',', ';']);
    if t.eq_ignore_ascii_case("noon") || t.eq_ignore_ascii_case("midnight") {
        return true;
    }
    let compact = t.replace(' ', "").to_lowercase().replace("a.m.", "am").replace("p.m.", "pm");
    if compact.is_empty() {
        return false;
    }
    let chars: Vec<char> = compact.chars().collect();
    let mut i = 0;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if !(1..=4).contains(&i) {
        return false;
    }
    if i < chars.len() && chars[i] == ':' {
        i += 1;
        let from = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            i += 1;
        }
        if i - from != 2 {
            return false;
        }
    }
    if i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        if !matches!(rest.as_str(), "am" | "pm" | "a.m" | "p.m") {
            return false;
        }
    }
    true
}

fn replace_last_time_or_number(before: &str, replacement: &str) -> Option<String> {
    let mut ws = words(before);
    for i in (0..ws.len()).rev() {
        if !is_time_or_number(&ws[i]) && !is_bare_hour(&ws[i]) {
            continue;
        }
        ws[i] = replacement.trim().to_string();
        return Some(ws.join(" "));
    }
    None
}

fn is_bare_hour(word: &str) -> bool {
    !word.is_empty() && word.chars().all(|c| c.is_ascii_digit()) && word.parse::<u32>().is_ok_and(|h| h <= 24)
}

fn looks_like_restart(after: &str) -> bool {
    let lower = after.to_ascii_lowercase();
    RESTART_STARTS.iter().any(|s| lower.starts_with(s))
}

fn replace_last_clause(before: &str, after: &str) -> String {
    let Some(split) = last_clause_split(before) else { return after.to_string() };
    let head = before[..split].trim_end();
    if head.is_empty() { after.to_string() } else { format!("{head} {after}") }
}

fn last_clause_split(before: &str) -> Option<usize> {
    let cut = [before.rfind(','), before.rfind(';')].into_iter().flatten().max();
    if let Some(cut) = cut {
        return Some(cut + 1);
    }
    let lower = before.to_ascii_lowercase();
    [last_phrase(&lower, " then "), last_phrase(&lower, " and ")].into_iter().flatten().max()
}

fn last_phrase(lower: &str, phrase: &str) -> Option<usize> {
    lower.rfind(phrase).map(|i| i + 1)
}

/// Byte index of `phrase` in `lower` as whole words, searching from byte `start`.
fn index_of_word_phrase(lower: &str, phrase: &str, mut start: usize) -> Option<usize> {
    while start + phrase.len() <= lower.len() {
        let idx = start + lower.get(start..)?.find(phrase)?;
        let before_ok = lower[..idx].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after_idx = idx + phrase.len();
        let after_ok = lower[after_idx..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return Some(idx);
        }
        start = idx + 1;
        while start < lower.len() && !lower.is_char_boundary(start) {
            start += 1;
        }
    }
    None
}

/// "wait" is a revision only when the remainder looks like a restart, not "wait for me".
fn index_of_wait_revision(lower: &str) -> Option<usize> {
    let mut start = 0;
    while start + 4 <= lower.len() {
        let idx = start + index_of_word_phrase(&lower[start..], "wait", 0)?;
        let after = lower[idx + 4..].trim_start_matches([',', '.', ';', ' ']);
        let head = words(after).into_iter().next().unwrap_or_default();
        if looks_like_restart(after) || is_time_or_number(&head) {
            return Some(idx);
        }
        start = idx + 4;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use CleanupMode::{Command, Dictate};

    #[test]
    fn dictation_swaps_a_time_or_number() {
        assert_eq!(apply_for("meet at 5 actually 6pm", Dictate), "meet at 6pm");
        assert_eq!(apply_for("meet at 5, no wait 6pm", Dictate), "meet at 6pm");
    }

    #[test]
    fn dictation_swaps_a_same_kind_word_after_a_comma() {
        assert_eq!(apply_for("Use ESP32, actually nRF52", Dictate), "Use nRF52");
        assert_eq!(apply_for("Use ESP32, I mean nRF52.", Dictate), "Use nRF52.");
        assert_eq!(apply_for("Meet on Tuesday, actually Friday", Dictate), "Meet on Friday");
        assert_eq!(apply_for("Send it to John, I mean Mark", Dictate), "Send it to Mark");
        for raw in ["I actually tested it", "Use ESP32 actually nRF52", "It was good, actually great", "Call John, actually later"] {
            assert_eq!(apply_for(raw, Dictate), raw);
        }
    }

    #[test]
    fn dictation_scratch_that_drops_the_last_sentence() {
        assert_eq!(apply_for("Send it today. Call him tomorrow scratch that", Dictate), "Send it today.");
        assert_eq!(apply_for("scratch that hello", Dictate), "hello");
    }

    #[test]
    fn dictation_keeps_ordinary_english() {
        for raw in ["it was actually good", "wait for me", "I mean it", "it is never mind the cost"] {
            assert_eq!(apply_for(raw, Dictate), raw);
        }
    }

    #[test]
    fn commands_take_the_last_clause() {
        assert_eq!(apply_for("open chrome actually open firefox", Command), "open firefox");
        assert_eq!(apply_for("set volume to 30 actually 50", Command), "set volume to 50");
        assert_eq!(apply_for("wait for me", Command), "wait for me");
        assert_eq!(apply_for("play jazz, never mind play rock", Command), "play rock");
    }

    #[test]
    fn time_or_number_shapes() {
        for yes in ["5", "5:30", "6pm", "6 p.m.", "noon", "1200"] {
            assert!(is_time_or_number(yes), "{yes}");
        }
        for no in ["", "hello", "12345", "5:3", "5xm"] {
            assert!(!is_time_or_number(no), "{no}");
        }
    }
}
