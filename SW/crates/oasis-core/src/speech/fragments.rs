//! Fragments said on their own (`docs/VOICE_DECISIONS.md` V5): "my LinkedIn", or "insert my
//! LinkedIn", types the user's stored text, exactly as written, with no cleanup and no model.
//!
//! A fragment inside a longer sentence is still a replacement ([`super::glossary::replace`]): "send
//! it to my mail" types the sentence with the address in it. This module is only for the phrase
//! that is the whole utterance.

/// Polite openings that may come before the verb: "can you insert my LinkedIn".
const OPENINGS: [&str; 5] = ["can you please", "could you please", "can you", "could you", "please"];
/// The verbs that introduce a fragment, so "insert my LinkedIn" and "my LinkedIn" mean the same.
const VERBS: [&str; 4] = ["insert", "type", "paste", "add"];

/// The text to type when the whole utterance `said` is the trigger of one of `fragments`
/// (`(spoken, written)` pairs), alone or after "insert", "type", "paste" or "add". Case and
/// punctuation do not matter. `None` when it is anything else, including a phrase that only
/// contains a trigger.
pub fn whole_phrase<'a>(said: &str, fragments: &'a [(String, String)]) -> Option<&'a str> {
    let said = plain(said);
    if said.is_empty() {
        return None;
    }
    let find = |phrase: &str| {
        fragments
            .iter()
            .find(|(spoken, written)| !written.is_empty() && plain(spoken) == phrase)
            .map(|(_, written)| written.as_str())
    };
    // The whole utterance first: a trigger may itself begin with a verb ("insert signature").
    if let Some(written) = find(&said) {
        return Some(written);
    }
    let mut rest = said.as_str();
    if let Some(after) = OPENINGS.iter().find_map(|o| word_prefix(rest, o)) {
        rest = after;
    }
    let rest = VERBS.iter().find_map(|v| word_prefix(rest, v))?;
    let rest = word_suffix(rest, "please").unwrap_or(rest);
    find(rest)
}

/// Lower case, with everything but letters, digits and apostrophes turned to single spaces.
fn plain(text: &str) -> String {
    let spaced: String = text.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' }).collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn word_prefix<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(prefix)?;
    rest.starts_with(' ').then(|| rest.trim_start())
}

fn word_suffix<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let rest = text.strip_suffix(suffix)?;
    rest.ends_with(' ').then(|| rest.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mine() -> Vec<(String, String)> {
        [("my linkedin", "https://linkedin.com/in/example"), ("my mail", "me@example.com"), ("insert signature", "Regards,\nExample"), ("empty one", "")]
            .iter()
            .map(|(s, w)| ((*s).to_string(), (*w).to_string()))
            .collect()
    }

    #[test]
    fn the_trigger_alone_or_after_a_verb_gives_the_text() {
        let f = mine();
        for said in ["My LinkedIn.", "my linkedin", "Insert my LinkedIn.", "type my linkedin", "Paste my LinkedIn", "add my linkedin"] {
            assert_eq!(whole_phrase(said, &f), Some("https://linkedin.com/in/example"), "{said}");
        }
        assert_eq!(whole_phrase("Can you please insert my mail?", &f), Some("me@example.com"));
        assert_eq!(whole_phrase("insert my mail please", &f), Some("me@example.com"));
    }

    #[test]
    fn a_trigger_that_starts_with_a_verb_is_found_whole_first() {
        assert_eq!(whole_phrase("Insert signature.", &mine()), Some("Regards,\nExample"));
    }

    #[test]
    fn a_phrase_that_only_contains_a_trigger_is_not_a_fragment() {
        let f = mine();
        for said in ["send it to my mail", "insert my mail and my linkedin", "my mailbox", "what is my linkedin", "hello", ""] {
            assert_eq!(whole_phrase(said, &f), None, "{said:?}");
        }
    }

    #[test]
    fn a_verb_alone_or_with_something_else_is_left_to_the_router() {
        // "type hello" is the router's own "type X" command unless "hello" is a trigger.
        assert_eq!(whole_phrase("type hello", &mine()), None);
        assert_eq!(whole_phrase("insert", &mine()), None);
    }

    #[test]
    fn a_fragment_with_no_text_is_ignored() {
        assert_eq!(whole_phrase("empty one", &mine()), None);
    }
}
