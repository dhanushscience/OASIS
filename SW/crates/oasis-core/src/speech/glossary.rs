//! Longest-match, whole-phrase glossary plus the built-in command homophones. Port of
//! `GlossaryApplier.cs`.

use super::text::{collapse_whitespace, ends_sentence, fold_case, tokens, trailing_punctuation};
use super::{CorrectionPair, GlossaryEntry};
use crate::matching::levenshtein;

/// Words the speech models routinely hear wrong, mapped to what was meant.
pub const BUILTIN_HOMOPHONES: [(&str, &str); 23] = [
    ("spot i fi", "Spotify"),
    ("spot if i", "Spotify"),
    ("spot if y", "Spotify"),
    ("spotfire", "Spotify"),
    ("spot ify", "Spotify"),
    ("spot i", "Spotify"),
    ("y sis", "Oasis"),
    ("why sis", "Oasis"),
    ("oh aces", "Oasis"),
    ("o aces", "Oasis"),
    // Parakeet and Whisper hear the name as the letters or as a near-word: "OSS", "o s s".
    ("oss", "Oasis"),
    ("o s s", "Oasis"),
    ("o.s.s.", "Oasis"),
    ("oh sis", "Oasis"),
    ("o sis", "Oasis"),
    ("oasys", "Oasis"),
    ("oasus", "Oasis"),
    ("owasis", "Oasis"),
    ("o a sis", "Oasis"),
    ("oh a sis", "Oasis"),
    ("oh ay sis", "Oasis"),
    ("oasis", "Oasis"),
    ("wispr flow", "Wispr Flow"),
];

/// Apply the user's glossary and the built-in homophones. Returns the text and what changed.
pub fn apply(text: &str, glossary: &[GlossaryEntry]) -> (String, Vec<CorrectionPair>) {
    let mut current = collapse_whitespace(text);
    if current.is_empty() {
        return (String::new(), Vec::new());
    }
    let mut applied = Vec::new();
    for (builtin, entry) in combine(glossary) {
        if entry.spoken.trim().is_empty() || entry.canonical.trim().is_empty() || entry.spoken == entry.canonical {
            continue;
        }
        let next = replace_phrase(&current, &entry.spoken, &entry.canonical, builtin);
        if next != current {
            applied.push(CorrectionPair::new(&entry.spoken, &entry.canonical, "glossary"));
            current = next;
        }
    }
    (current, applied)
}

/// The user's replacements ("my mail" → their address), longest phrase first, whole words, any
/// case. Unlike [`apply`] the text is left exactly as it is otherwise, line breaks and all.
pub fn replace(text: &str, replacements: &[(String, String)]) -> String {
    let mut list: Vec<&(String, String)> = replacements.iter().filter(|(s, w)| !s.trim().is_empty() && !w.is_empty()).collect();
    list.sort_by_key(|(s, _)| std::cmp::Reverse(s.chars().count()));
    list.into_iter().fold(text.to_string(), |text, (spoken, written)| replace_phrase(&text, spoken, written, false))
}

/// Shortest Dictionary spelling a misheard name is matched against.
const NEAR_MIN_LETTERS: usize = 6;

/// A name the speech model misheard by a letter or two takes the Dictionary's spelling (V41):
/// "Chashank", "Shashamo" → "Shashank". Only a capitalised word inside a sentence (the speech
/// model wrote a name) against one-word Dictionary spellings that are names themselves; a day, a
/// month, or a word close to two spellings stays.
pub fn near(text: &str, glossary: &[GlossaryEntry]) -> String {
    let is_name = |w: &str| w.chars().count() >= NEAR_MIN_LETTERS && w.chars().all(char::is_alphabetic) && w.starts_with(char::is_uppercase);
    let names: Vec<&str> = glossary.iter().map(|e| e.canonical.trim()).filter(|c| is_name(c)).collect();
    if names.is_empty() {
        return text.to_string();
    }
    let toks = tokens(text);
    let fixed: Vec<String> = toks
        .iter()
        .enumerate()
        .map(|(i, tok)| {
            let trail = trailing_punctuation(tok);
            let word = &tok[..tok.len() - trail.len()];
            let inside = i > 0 && !ends_sentence(&toks[i - 1]);
            if !inside || !is_name(word) || super::repair::is_date_word(word) || names.iter().any(|n| n.eq_ignore_ascii_case(word)) {
                return tok.clone();
            }
            let mut hits = names.iter().filter(|n| is_near(word, n));
            match (hits.next(), hits.next()) {
                (Some(name), None) => format!("{name}{trail}"),
                _ => tok.clone(),
            }
        })
        .collect();
    fixed.join(" ")
}

/// One letter apart; or two, for a long name that starts with the same four letters.
fn is_near(word: &str, name: &str) -> bool {
    let (w, n) = (fold_case(word), fold_case(name));
    let distance = levenshtein(&w, &n);
    distance == 1 || (distance == 2 && n.chars().count() >= 8 && w.chars().take(4).eq(n.chars().take(4)))
}

/// Built-ins first, then the user's entries (so their casing wins); each group longest spoken
/// phrase first (stable). The flag marks a built-in.
fn combine(glossary: &[GlossaryEntry]) -> Vec<(bool, GlossaryEntry)> {
    let by_len = |l: &mut Vec<GlossaryEntry>| l.sort_by_key(|e| std::cmp::Reverse(e.spoken.chars().count()));
    let mut builtin: Vec<GlossaryEntry> = BUILTIN_HOMOPHONES.iter().map(|(s, c)| GlossaryEntry::new(s, c)).collect();
    let mut user = glossary.to_vec();
    by_len(&mut builtin);
    by_len(&mut user);
    builtin.into_iter().map(|e| (true, e)).chain(user.into_iter().map(|e| (false, e))).collect()
}

/// `keep_caps`: leave a match already written as the canonical in capitals ("OASIS").
fn replace_phrase(text: &str, spoken: &str, canonical: &str, keep_caps: bool) -> String {
    let needle = fold_case(spoken.trim());
    if needle.is_empty() {
        return text.to_string();
    }
    let mut text = text.to_string();
    let mut lower = fold_case(&text);
    if lower.len() != text.len() {
        return text;
    }
    let mut idx = 0;
    while idx + needle.len() <= lower.len() {
        let Some(rel) = lower.get(idx..).and_then(|s| s.find(&needle)) else { break };
        let found = idx + rel;
        let after = found + needle.len();
        let before_ok = lower[..found].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after_ok = lower[after..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        let shouting = keep_caps && text[found..after] == canonical.to_uppercase();
        if before_ok && after_ok && !shouting {
            text.replace_range(found..after, canonical);
            lower = fold_case(&text);
            if lower.len() != text.len() {
                return text;
            }
            idx = found + canonical.len();
            continue;
        }
        idx = found + 1;
        while idx < lower.len() && !lower.is_char_boundary(idx) {
            idx += 1;
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_terms_apply() {
        let glossary = [GlossaryEntry::new("kicad", "KiCad")];
        let (text, applied) = apply("open kicad", &glossary);
        assert_eq!(text, "open KiCad");
        assert_eq!(applied, vec![CorrectionPair::new("kicad", "KiCad", "glossary")]);
    }

    #[test]
    fn builtin_homophones_apply_longest_first() {
        assert_eq!(apply("open spot i fi", &[]).0, "open Spotify");
        assert_eq!(apply("hey o s s open it", &[]).0, "hey Oasis open it");
        assert_eq!(apply("the oasis", &[]).0, "the Oasis");
    }

    #[test]
    fn a_replacement_types_the_users_own_text_in_any_case() {
        let r = [("my mail".to_string(), "me@x.com".to_string()), ("my mail address".to_string(), "Mail: me@x.com".to_string())];
        assert_eq!(replace("Send it to my mail.", &r), "Send it to me@x.com.");
        assert_eq!(replace("My mail address is\nhere", &r), "Mail: me@x.com is\nhere");
        assert_eq!(replace("dummy mail", &r), "dummy mail");
    }

    #[test]
    fn a_misheard_name_takes_the_dictionary_spelling() {
        let glossary = [GlossaryEntry::new("Shashank", "Shashank"), GlossaryEntry::new("kay cad", "KiCad"), GlossaryEntry::new("Sundar", "Sundar")];
        assert_eq!(near("Actually, it is Chashank.", &glossary), "Actually, it is Shashank.");
        assert_eq!(near("send it to Shashamo, then call", &glossary), "send it to Shashank, then call");
        for kept in [
            "Meet me on Sunday.",              // a day, one letter from "Sundar"
            "Shashamo is here.",               // opens the sentence: the capital says nothing
            "send it to Shashank",             // already right
            "send it to Shankar",              // a different name
            "the shashamo board",              // not written as a name
        ] {
            assert_eq!(near(kept, &glossary), kept);
        }
    }

    #[test]
    fn user_casing_beats_the_builtin() {
        let g = [GlossaryEntry::new("project oasis", "Project OASIS")];
        assert_eq!(apply("the project oasis plan", &g).0, "the Project OASIS plan");
        assert_eq!(apply("OASIS", &[]).0, "OASIS");
        assert_eq!(apply("hey OSS", &[]).0, "hey Oasis");
        assert_eq!(apply("use wispr flow", &[]).0, "use Wispr Flow");
    }

    #[test]
    fn whole_words_only() {
        assert_eq!(apply("the bosses are here", &[]).0, "the bosses are here");
    }
}
