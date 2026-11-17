//! Saying the name of a Quick Launch setup ("project mode") opens everything in it. Port of
//! `Oasis.Core/QuickLaunch/QuickLaunchVoice.cs` (the matching part). The setups themselves are read
//! from `profiles.json` by `oasis-settings`.
//!
//! The whole utterance has to be the phrase, optionally after a word like "launch" or "open": a
//! phrase that merely appears inside a longer sentence does not count. Opening half a dozen apps
//! because the words came up in passing would be worse than asking to say it again.

/// Shortest phrase, in letters and digits. Shorter ones are what speech-to-text invents from noise.
pub const MIN_PHRASE_LETTERS: usize = 3;

// Longest first, so "switch to" is tried before a shorter word could match part of it.
const LEAD_INS: [&str; 14] = [
    "quick launch", "switch to", "bring up", "turn on", "set up", "go to", "activate", "launch", "start", "begin", "enter",
    "open", "load", "run",
];
const POLITE: [&str; 7] = ["could you please", "can you please", "could you", "can you", "please", "hey oasis", "oasis"];
const ARTICLES: [&str; 2] = ["the", "my"];
const TRAILING: [&str; 2] = ["please", "now"];

/// What the router needs to know about one setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickSetupRef {
    pub id: String,
    /// The name, then the extra phrases, as typed.
    pub phrases: Vec<String>,
}

/// Lower case, letters and digits only, single spaces: "Project-Mode!" → "project mode".
pub fn normalize(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            out.extend(c.to_lowercase());
            gap = false;
        } else if c != '\'' && c != '’' {
            // An apostrophe joins ("let's" → "lets"); anything else separates words.
            gap = true;
        }
    }
    out
}

fn letters(phrase: &str) -> usize {
    phrase.chars().filter(|c| c.is_alphanumeric()).count()
}

fn compact(phrase: &str) -> String {
    phrase.replace(' ', "")
}

/// The id of the setup the utterance asks for, or `None`. The utterance as spoken is tried first,
/// so a phrase that itself starts with "open" still matches exactly.
pub fn matching<'a>(transcript: &str, setups: &'a [QuickSetupRef]) -> Option<&'a str> {
    let forms = forms(&normalize(transcript));
    for form in &forms {
        let squeezed = compact(form);
        for setup in setups {
            let hit = setup.phrases.iter().map(|p| normalize(p)).filter(|p| letters(p) >= MIN_PHRASE_LETTERS).any(|p| p == *form || compact(&p) == squeezed);
            if hit {
                return Some(&setup.id);
            }
        }
    }
    None
}

/// The utterance, then without politeness, then without a lead-in word, then without an article.
fn forms(normalized: &str) -> Vec<String> {
    let mut forms: Vec<String> = Vec::new();
    let mut add = |form: &str| {
        if !form.is_empty() && !forms.iter().any(|f| f == form) {
            forms.push(form.to_string());
        }
    };
    add(normalized);
    let plain = strip_suffix(strip_prefix(normalized, &POLITE), &TRAILING);
    add(plain);
    let named = strip_prefix(plain, &LEAD_INS);
    add(named);
    add(strip_prefix(named, &ARTICLES));
    forms
}

fn strip_prefix<'a>(text: &'a str, prefixes: &[&str]) -> &'a str {
    prefixes.iter().find_map(|p| text.strip_prefix(p).and_then(|rest| rest.strip_prefix(' '))).unwrap_or(text)
}

fn strip_suffix<'a>(text: &'a str, suffixes: &[&str]) -> &'a str {
    suffixes.iter().find_map(|s| text.strip_suffix(s).and_then(|rest| rest.strip_suffix(' '))).unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setups() -> Vec<QuickSetupRef> {
        vec![
            QuickSetupRef { id: "a".into(), phrases: vec!["Project Mode".into(), "time to work".into()] },
            QuickSetupRef { id: "b".into(), phrases: vec!["Gaming".into(), "ok".into()] },
        ]
    }

    #[test]
    fn the_name_and_extra_phrases_open_a_setup() {
        let s = setups();
        for said in ["project mode", "Project-Mode!", "open project mode", "please launch the project mode now", "hey oasis start my project mode", "projectmode", "Time to work."] {
            assert_eq!(matching(said, &s), Some("a"), "{said}");
        }
        assert_eq!(matching("switch to gaming", &s), Some("b"));
    }

    #[test]
    fn a_phrase_inside_a_longer_sentence_does_not_count() {
        let s = setups();
        for said in ["what is project mode", "open project mode and close chrome", "it is time to work now friends", "ok", "", "open"] {
            assert_eq!(matching(said, &s), None, "{said}");
        }
    }

    #[test]
    fn a_phrase_that_starts_with_a_lead_in_word_still_matches_whole() {
        let s = vec![QuickSetupRef { id: "x".into(), phrases: vec!["open day".into()] }];
        assert_eq!(matching("open day", &s), Some("x"));
    }
}
