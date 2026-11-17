//! Reject rewrites (from a language model or anything else) that invent commands, apps or facts.
//! Port of `MeaningPreservation.cs`. Cleanup may delete fillers and fix grammar; it may not add a
//! "launch app" the user did not say.

use std::collections::HashSet;

use super::text::words;
use super::CorrectionPair;

const COMMAND_VERBS: [&str; 12] =
    ["open", "launch", "start", "run", "close", "quit", "kill", "delete", "shutdown", "format", "install", "uninstall"];

/// Whether `candidate` is an acceptable rewrite of `source`.
pub fn accepts(source: &str, candidate: &str) -> bool {
    if candidate.trim().is_empty() {
        return source.trim().is_empty();
    }
    let src = words(source);
    let dst = words(candidate);
    if dst.is_empty() {
        return src.is_empty();
    }
    if dst.len() as f64 > src.len() as f64 * 1.6 + 2.0 {
        return false;
    }
    // A rewrite that lost most of the words is a truncated generation, not a cleanup.
    if src.len() > 12 && (dst.len() as f64) < src.len() as f64 * 0.6 {
        return false;
    }
    if introduces_command(&src, &dst) {
        return false;
    }
    if src.is_empty() {
        return false;
    }
    // A changed or lowercased name (Danosh to danish, Git to git) is a meaning change, whatever the overlap.
    if capitalised_mid_sentence(source).iter().any(|n| !dst.contains(n)) {
        return false;
    }
    // Polish may not coin a new proper noun or acronym (opus to OASIS) the source never said.
    let known = lower_set(&src);
    if capitalised_mid_sentence(candidate).iter().any(|w| !known.contains(&w.to_lowercase())) {
        return false;
    }
    if jaccard(&src, &dst) >= 0.45 {
        return true;
    }
    is_subset(&dst, &src) || is_subset(&src, &dst)
}

/// Words capitalised away from a sentence start (names, acronyms, "I"), plus any all-caps or camel-case word anywhere.
pub fn capitalised_mid_sentence(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = true;
    for token in source.split_whitespace() {
        let word = words(token).into_iter().next().unwrap_or_default();
        let shouty = word.chars().count() > 1 && word.chars().skip(1).any(char::is_uppercase);
        if (!start || shouty) && word.chars().next().is_some_and(char::is_uppercase) {
            out.push(word);
        }
        start = token.ends_with(['.', '?', '!']);
    }
    out
}

/// Words that differ in place between `raw` and `cleaned` (same word count only).
pub fn spelling_substitutions(raw: &str, cleaned: &str) -> Vec<CorrectionPair> {
    let a = words(raw);
    let b = words(cleaned);
    if a.is_empty() || a.len() != b.len() {
        return Vec::new();
    }
    a.iter()
        .zip(&b)
        .filter(|(x, y)| x.to_lowercase() != y.to_lowercase())
        .map(|(x, y)| CorrectionPair::new(x, y, "spelling"))
        .collect()
}

fn is_command_verb(word: &str) -> bool {
    COMMAND_VERBS.iter().any(|v| word.eq_ignore_ascii_case(v))
}

fn introduces_command(source: &[String], dest: &[String]) -> bool {
    dest.first().is_some_and(|w| is_command_verb(w)) && !source.iter().any(|w| is_command_verb(w))
}

fn lower_set(w: &[String]) -> HashSet<String> {
    w.iter().map(|x| x.to_lowercase()).collect()
}

fn jaccard(a: &[String], b: &[String]) -> f64 {
    let (left, right) = (lower_set(a), lower_set(b));
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let inter = left.intersection(&right).count();
    let union = left.union(&right).count();
    if union == 0 { 0.0 } else { inter as f64 / union as f64 }
}

fn is_subset(inner: &[String], outer: &[String]) -> bool {
    let set = lower_set(outer);
    inner.iter().all(|w| set.contains(&w.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_fixes_pass_and_invented_commands_do_not() {
        assert!(accepts("um I think we should go", "I think we should go"));
        assert!(!accepts("what is the weather", "open chrome what is the weather"));
        assert!(!accepts("hello there", "Open the settings and also run a long unrelated sentence now"));
    }

    #[test]
    fn a_changed_or_lowercased_name_is_a_meaning_change() {
        assert!(!accepts("I told Danosh about the plan today", "I told danish about the plan today"));
        assert!(!accepts("please push it to Git before lunch", "Please push it to git before lunch"));
        assert!(accepts("um I think we should go", "I think we should go"));
        assert!(accepts("Um so we left. Then we ate", "So we left. Then we ate"));
    }

    #[test]
    fn polish_may_not_coin_a_new_name_or_acronym() {
        assert!(!accepts("Deploy opus", "Deploy OASIS"));
        assert!(!accepts("send it to the bus", "Send it to the SPI bus"));
        // Capitalising a word the source already said lower-case is fine.
        assert!(accepts("i think we should open chrome", "I think we should open Chrome"));
        assert!(accepts(
            "make sure UART initialized before SPI starts",
            "Make sure the UART is initialized before the SPI bus starts"
        ));
    }

    #[test]
    fn truncated_rewrites_are_rejected() {
        let long = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen";
        assert!(!accepts(long, "one two three four five"));
        assert!(accepts(long, long));
    }

    #[test]
    fn spelling_pairs_pick_out_changed_words() {
        let pairs = spelling_substitutions("open kay cad", "open KiCad");
        assert!(pairs.is_empty(), "different word counts give no pairs");
        let pairs = spelling_substitutions("open kicad now", "open KiCad now");
        assert!(pairs.is_empty());
        let pairs = spelling_substitutions("open cad now", "open KiCad now");
        assert_eq!(pairs, vec![CorrectionPair::new("cad", "KiCad", "spelling")]);
    }
}
