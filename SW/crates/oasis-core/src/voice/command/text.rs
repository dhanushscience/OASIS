//! Cleaning up what the speech model heard so it can be matched. Port of `VoiceText` in
//! `Services/Voice/VoiceLexicon.cs`.

/// Words people say to get OASIS's attention, dropped from the front.
const FILLERS: [&str; 9] = ["please", "hey oasis", "ok oasis", "okay oasis", "hey", "ok", "okay", "um", "uh"];

/// Speech models often invent these on silence or noise.
const SILENCE_HALLUCINATIONS: [&str; 33] = [
    "thank you", "thanks", "thanks for watching", "thanks for listening", "thank you for watching",
    "thank you for listening", "you", "the", "a", "i", "it", "so", "oh", "uh", "um", "hmm", "hm", "ah",
    "bye", "goodbye", "see you", "see you next time", "subtitle", "subtitles", "caption", "captions",
    "music", "applause", "silence", "blank audio", "blank", "continue", "subscribe",
];

/// Lower case, letters, digits and `%` only, single spaces, leading fillers removed, and the few
/// words the model routinely slurs repaired ("spot i fi" is "spotify").
pub fn normalize(text: &str) -> String {
    let mut cleaned: String = text
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '%' { c } else { ' ' })
        .collect();
    cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return cleaned;
    }
    for filler in FILLERS {
        if let Some(rest) = cleaned.strip_prefix(&format!("{filler} ")) {
            cleaned = rest.trim().to_string();
        } else if cleaned == filler {
            cleaned.clear();
        }
    }
    repair_command_homophones(&cleaned)
}

/// Whether a phrase is worth acting on: not empty, not a silence hallucination, not one short
/// noise word, and at least three letters.
pub fn is_usable_command(text: &str) -> bool {
    let normalized = normalize(text);
    if normalized.is_empty() || SILENCE_HALLUCINATIONS.contains(&normalized.as_str()) {
        return false;
    }
    let words: Vec<&str> = normalized.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() || (words.len() == 1 && words[0].chars().count() <= 2) {
        return false;
    }
    normalized.chars().filter(|c| c.is_alphabetic()).count() >= 3
}

const HOMOPHONES: [(&str, &str); 10] = [
    ("spot i fi", "spotify"),
    ("spot if i", "spotify"),
    ("spot if y", "spotify"),
    ("spotfire", "spotify"),
    ("spot ify", "spotify"),
    ("spot i", "spotify"),
    ("y sis", "oasis"),
    ("why sis", "oasis"),
    ("oh aces", "oasis"),
    ("o aces", "oasis"),
];

/// Replace whole-word slurred phrases with the intended word.
pub fn repair_command_homophones(normalized: &str) -> String {
    let mut value = normalized.to_string();
    for (from, to) in HOMOPHONES {
        let mut at = 0;
        while let Some(found) = value[at..].find(from) {
            let start = at + found;
            let end = start + from.len();
            let before_ok = start == 0 || value.as_bytes()[start - 1] == b' ';
            let after_ok = end == value.len() || value.as_bytes()[end] == b' ';
            if before_ok && after_ok {
                value.replace_range(start..end, to);
                at = start + to.len();
            } else {
                at = end;
            }
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_lowercased_depunctuated_and_stripped_of_leading_fillers() {
        assert_eq!(normalize("  Open   Spotify, please! "), "open spotify please");
        assert_eq!(normalize("Hey Oasis, open notepad"), "open notepad");
        assert_eq!(normalize("OK Oasis open notepad"), "open notepad");
        assert_eq!(normalize("please"), "");
        assert_eq!(normalize("Volume to 50%"), "volume to 50%");
        assert_eq!(normalize("   "), "");
    }

    #[test]
    fn slurred_command_words_are_repaired_only_as_whole_words() {
        assert_eq!(normalize("open spot i fi"), "open spotify");
        assert_eq!(normalize("play spotfire"), "play spotify");
        assert_eq!(normalize("open y sis"), "open oasis");
        assert_eq!(normalize("the spotted dog"), "the spotted dog", "inside another word: left alone");
        assert_eq!(repair_command_homophones("spot i spot i"), "spotify spotify");
    }

    #[test]
    fn silence_and_noise_are_not_commands() {
        for noise in ["Thank you.", "thanks for watching", "Um", "you", "a", "ok", "hm", "", "   ", "!!", "ab", "x y"] {
            assert!(!is_usable_command(noise), "{noise:?}");
        }
        for real in ["open notepad", "mute", "volume up", "type hello world"] {
            assert!(is_usable_command(real), "{real:?}");
        }
    }
}
