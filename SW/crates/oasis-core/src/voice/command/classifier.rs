//! Is a phrase addressed to OASIS ("Oasis, …", "hey Oasis …")? Port of part of
//! `Services/Voice/VoiceUtteranceClassifier.cs`.
//!
//! Whether a phrase is typed or run is decided by [`super::gate`] (`docs/VOICE_DECISIONS.md` V3,
//! V4). The old rule here (escalate any short phrase that starts with a command word) was removed
//! on 2026-11-12: it ran "open the door for me" and dropped "close WhatsApp" when nothing matched.
//! [`looks_like_command`] stays because a bare "Oasis" counts as an address only before a command.

use super::text::normalize;

const COMMAND_PREFIXES: [&str; 40] = [
    "open ", "launch ", "start ", "run ", "close ", "quit ", "exit ", "kill ", "switch to ", "focus ",
    "go to ", "minimize ", "maximize ", "click ", "press the ", "tap ", "play ", "pause", "resume",
    "next track", "previous track", "next song", "previous song", "skip", "set volume", "volume up",
    "volume down", "volume to ", "mute", "unmute", "shuffle ", "repeat ", "type ", "paste ", "press ",
    "hotkey ", "shortcut ", "write ", "volume mute", "play pause",
];

const EXACT_COMMANDS: [&str; 8] = ["pause", "resume", "mute", "unmute", "play", "next", "previous", "skip"];

const SPOKEN_ADDRESSES: [&str; 5] = ["hey oasis", "ok oasis", "okay oasis", "hi oasis", "oasis"];

/// The phrase is spoken to OASIS: "hey Oasis …", or "Oasis, …" / "Oasis open …".
pub fn is_addressed(text: &str) -> bool {
    strip_address(text).is_some()
}

/// The phrase without its leading "Oasis," / "hey Oasis"; unchanged when not addressed.
pub fn without_address(text: &str) -> String {
    strip_address(text).unwrap_or_else(|| text.trim().to_string())
}

fn strip_address(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let lower = trimmed.to_lowercase();
    for address in SPOKEN_ADDRESSES {
        if !lower.starts_with(address) {
            continue;
        }
        if lower[address.len()..].chars().next().is_some_and(char::is_alphanumeric) {
            continue;
        }
        let after = &trimmed[address.len()..];
        let candidate = after.trim_start_matches([',', '.', '!', ':', ';', ' ']).trim();
        if candidate.is_empty() {
            return None;
        }
        // Bare "Oasis" is also a word people type ("Oasis is a great band"). It counts as an
        // address only with a comma after it, or when what follows is a command.
        if address == "oasis" && !after.trim_start().starts_with(',') && !looks_like_command(candidate) {
            return None;
        }
        return Some(candidate.to_string());
    }
    None
}

/// Clearly a desktop action, not free-form dictation.
pub fn looks_like_command(text: &str) -> bool {
    if is_typed_punctuation(text) {
        return false;
    }
    let normalized = normalize(text);
    if normalized.is_empty() {
        return false;
    }
    if EXACT_COMMANDS.contains(&normalized.as_str()) {
        return true;
    }
    for prefix in COMMAND_PREFIXES {
        if !normalized.starts_with(prefix) {
            continue;
        }
        // "start of the meeting" is prose; "start notepad" is a command.
        if matches!(prefix, "start " | "run " | "play ") {
            let rest = normalized[prefix.len()..].trim();
            if ["of ", "the day", "with ", "nice"].iter().any(|p| rest.starts_with(p)) {
                continue;
            }
        }
        return true;
    }
    false
}

/// Spoken punctuation, or text that already starts with a bracket or quote. "Close parenthesis"
/// starts with "close" like "close WhatsApp", but nothing on the desktop is called a bracket.
fn is_typed_punctuation(text: &str) -> bool {
    let trimmed = text.trim_start();
    if trimmed.chars().next().is_some_and(|c| matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | '"')) {
        return true;
    }
    let normalized = normalize(trimmed);
    let words: Vec<&str> = normalized.split_whitespace().collect();
    let (Some(first), Some(second)) = (words.first(), words.get(1)) else { return false };
    if !matches!(*first, "open" | "close" | "left" | "right") {
        return false;
    }
    let third = words.get(2).copied().unwrap_or("");
    matches!(*second, "paren" | "parenthesis" | "parentheses" | "bracket" | "brace" | "quote" | "quotes")
        || (matches!(*second, "square" | "curly" | "angle") && matches!(third, "bracket" | "brackets" | "brace"))
        || (*second == "quotation" && third == "mark")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_told_from_prose() {
        for command in ["open Spotify", "close WhatsApp", "mute", "Pause", "volume up", "type hello", "start notepad", "press control t"] {
            assert!(looks_like_command(command), "{command}");
        }
        for prose in ["Start of the meeting is at nine", "play with the kids later", "the weather is nice", "I opened the door", "run with it"] {
            assert!(!looks_like_command(prose), "{prose}");
        }
    }

    #[test]
    fn spoken_punctuation_is_not_a_command() {
        for typed in ["close parenthesis", "Open square bracket", "(Open the file", "close quote", "[Open", "open curly brace", "open quotation mark"] {
            assert!(!looks_like_command(typed), "{typed}");
        }
        assert!(looks_like_command("close the window"));
    }

    #[test]
    fn being_addressed_is_recognised_and_stripped() {
        assert_eq!(without_address("Hey Oasis, open notepad"), "open notepad");
        assert_eq!(without_address("OK Oasis: mute"), "mute");
        assert_eq!(without_address("Oasis, what time is it"), "what time is it");
        assert_eq!(without_address("open notepad"), "open notepad");
        assert!(is_addressed("hi oasis open spotify"));
    }

    #[test]
    fn the_word_oasis_alone_is_not_an_address() {
        assert!(!is_addressed("Oasis is a great band"));
        assert!(!is_addressed("Oasis"));
        assert!(!is_addressed("Oasisland open"), "part of a longer word");
        assert!(is_addressed("Oasis open notepad"), "followed by a command");
        assert!(is_addressed("Oasis, is a great band"), "with a comma");
    }
}
