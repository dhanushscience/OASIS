//! Promote a spelling/glossary correction only after the same pair repeats; one-off model guesses
//! never enter the vocabulary. Port of `VocabularyPromotionPolicy.cs`.

use std::collections::{HashMap, HashSet};

use super::text::{collapse_whitespace, words};
use super::{CorrectionPair, GlossaryEntry};

/// How many times a pair must be seen before it is learned.
pub const PROMOTE_AFTER_SIGHTINGS: u32 = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VocabularyState {
    pub entries: Vec<GlossaryEntry>,
    /// Sighting counts keyed by [`key`].
    pub sightings: HashMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Promotion {
    pub promoted: bool,
    pub entry: Option<GlossaryEntry>,
    pub sightings: u32,
    pub key: String,
}

pub fn key(from: &str, to: &str) -> String {
    format!("{}\t{}", normalize(from), normalize(to))
}

pub fn is_promotable(from: &str, to: &str) -> bool {
    let a = collapse_whitespace(from);
    let b = collapse_whitespace(to);
    if a.is_empty() || b.is_empty() {
        return false;
    }
    // Case-only changes are worth learning only as brand casing ("KiCad", "iPhone"). Label caps and
    // title case turned every dictated "power", "up" and "code" into POWER, UP, Code.
    if a.to_lowercase() == b.to_lowercase() && !is_brand_casing(&b) {
        return false;
    }
    if a.chars().count() > 40 || b.chars().count() > 40 {
        return false;
    }
    let (aw, bw) = (words(&a), words(&b));
    if !(1..=4).contains(&aw.len()) || !(1..=4).contains(&bw.len()) || aw.len().abs_diff(bw.len()) > 1 {
        return false;
    }
    if !a.chars().any(char::is_alphabetic) || !b.chars().any(char::is_alphabetic) {
        return false;
    }
    !(looks_like_revision(&a) || looks_like_revision(&b) || is_filler_only(&aw) || is_filler_only(&bw))
}

/// Count one sighting of `from` → `to`; learn it once it has been seen often enough.
pub fn observe(state: &VocabularyState, from: &str, to: &str) -> VocabularyState {
    if !is_promotable(from, to) || contains(&state.entries, from, to) {
        return state.clone();
    }
    let k = key(from, to);
    let mut sightings = state.sightings.clone();
    let count = sightings.entry(k).or_insert(0);
    *count += 1;
    let reached = *count >= PROMOTE_AFTER_SIGHTINGS;

    let mut entries = state.entries.clone();
    if reached {
        entries.push(GlossaryEntry::new(&collapse_whitespace(from), &collapse_whitespace(to)));
    }
    VocabularyState { entries, sightings }
}

pub fn observe_all(state: &VocabularyState, corrections: &[CorrectionPair]) -> VocabularyState {
    let mut current = state.clone();
    for pair in corrections {
        // Glossary hits already come from a glossary: the user's own (nothing to learn) or the
        // command lexicon, whose pad labels and app names must not leak into dictation.
        if pair.kind == "spelling" || pair.kind == "llm" {
            current = observe(&current, &pair.from, &pair.to);
        }
    }
    current
}

/// Drops what this policy learned but would no longer learn: case-only swaps, and copies of
/// command vocabulary that leaked in when that list was also applied to dictation. An entry counts
/// as learned only when its sighting reached the promotion count; entries the user added by hand
/// have none and are always kept.
pub fn purge_learned(state: &VocabularyState, command_spoken_forms: &[String]) -> VocabularyState {
    let command: HashSet<String> = command_spoken_forms.iter().map(|s| normalize(s)).collect();
    let mut entries = Vec::with_capacity(state.entries.len());
    let mut sightings = state.sightings.clone();
    for entry in &state.entries {
        let k = key(&entry.spoken, &entry.canonical);
        let learned = state.sightings.get(&k).is_some_and(|c| *c >= PROMOTE_AFTER_SIGHTINGS);
        let unwanted = !is_promotable(&entry.spoken, &entry.canonical) || command.contains(&normalize(&entry.spoken));
        if learned && unwanted {
            sightings.remove(&k);
            continue;
        }
        entries.push(entry.clone());
    }
    // Pending sightings stay: this runs after every settings save, and wiping a half-counted
    // "KiCad" would stop brand casing from ever reaching the promotion count.
    VocabularyState { entries, sightings }
}

/// What the next sighting of `from` → `to` would do, without recording it.
pub fn peek(state: &VocabularyState, from: &str, to: &str) -> Promotion {
    let k = key(from, to);
    if !is_promotable(from, to) {
        return Promotion { promoted: false, entry: None, sightings: 0, key: k };
    }
    let next = state.sightings.get(&k).copied().unwrap_or(0) + 1;
    let promoted = next >= PROMOTE_AFTER_SIGHTINGS && !contains(&state.entries, from, to);
    Promotion {
        promoted,
        entry: promoted.then(|| GlossaryEntry::new(&collapse_whitespace(from), &collapse_whitespace(to))),
        sightings: next,
        key: k,
    }
}

/// Mixed case inside a word and not all caps: "KiCad", "iPhone", "WhatsApp".
fn is_brand_casing(value: &str) -> bool {
    let (mut has_lower, mut inner_upper) = (false, false);
    for word in words(value) {
        for (i, c) in word.chars().enumerate() {
            if c.is_lowercase() {
                has_lower = true;
            } else if i > 0 && c.is_uppercase() {
                inner_upper = true;
            }
        }
    }
    has_lower && inner_upper
}

fn contains(entries: &[GlossaryEntry], from: &str, to: &str) -> bool {
    let (spoken, canonical) = (normalize(from), normalize(to));
    entries.iter().any(|e| normalize(&e.spoken) == spoken && normalize(&e.canonical) == canonical)
}

fn normalize(value: &str) -> String {
    collapse_whitespace(value).to_lowercase()
}

fn looks_like_revision(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.contains(" actually ") || lower.contains(" i mean ") || lower.starts_with("actually ")
}

fn is_filler_only(words: &[String]) -> bool {
    words.len() == 1 && matches!(words[0].as_str(), "um" | "uh" | "er" | "ah" | "hmm" | "hm" | "like" | "so")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pair_is_learned_on_its_second_sighting() {
        let s0 = VocabularyState::default();
        let s1 = observe(&s0, "kay cad", "KiCad");
        assert!(s1.entries.is_empty());
        assert!(peek(&s1, "kay cad", "KiCad").promoted);
        let s2 = observe(&s1, "kay cad", "KiCad");
        assert_eq!(s2.entries, vec![GlossaryEntry::new("kay cad", "KiCad")]);
        // Already known: nothing changes.
        assert_eq!(observe(&s2, "kay cad", "KiCad"), s2);
    }

    #[test]
    fn unpromotable_pairs_are_ignored() {
        for (a, b) in [
            ("power", "POWER"),
            ("code", "Code"),
            ("um", "uh"),
            ("two", "2"),
            ("no actually 5", "6"),
            ("a b c d e", "x"),
            ("", "x"),
        ] {
            assert!(!is_promotable(a, b), "{a} -> {b}");
        }
        assert!(is_promotable("kicad", "KiCad"));
    }

    #[test]
    fn only_spelling_and_llm_corrections_count() {
        let pairs = vec![
            CorrectionPair::new("kay cad", "KiCad", "glossary"),
            CorrectionPair::new("kay cad", "KiCad", "spelling"),
            CorrectionPair::new("kay cad", "KiCad", "llm"),
        ];
        let state = observe_all(&VocabularyState::default(), &pairs);
        assert_eq!(state.entries.len(), 1);
    }

    #[test]
    fn purge_drops_learned_command_words_but_keeps_hand_made_entries() {
        let mut state = VocabularyState {
            entries: vec![GlossaryEntry::new("new tab", "New Tab"), GlossaryEntry::new("kay cad", "KiCad")],
            ..VocabularyState::default()
        };
        state.sightings.insert(key("new tab", "New Tab"), 2);
        let purged = purge_learned(&state, &["new tab".to_string()]);
        assert_eq!(purged.entries, vec![GlossaryEntry::new("kay cad", "KiCad")]);
        assert!(purged.sightings.is_empty());
    }
}
