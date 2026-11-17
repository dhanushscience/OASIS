//! Collapses the repeats overlapping speech-to-text emits. Port of `RepeatCollapser.cs`. Works on
//! punctuation-bearing tokens and returns the input untouched when nothing repeats, so it never
//! strips the commas and question marks the model produced.

use super::numbers::is_number_word;
use super::text::{collapse_whitespace, token_key, tokens, trailing_punctuation, with_trailing_punctuation};

const MAX_PHRASE_WORDS: usize = 6;
const MAX_DICTATION_PHRASE_WORDS: usize = 12;

/// Words people double on purpose; a stutter filter must leave "bye bye" alone. Not "very": an
/// unpunctuated "very very" is a stutter, only the paused "very, very" is kept (2.3).
const DELIBERATE_DOUBLES: [&str; 10] = ["bye", "had", "that", "no", "yes", "yeah", "ha", "hey", "so", "is"];

/// Commands and chunk merges: "open open Spotify" is always a seam echo.
pub fn collapse(text: &str) -> String {
    run(text, false, 2)
}

/// Dictation keeps people's own repeats. A single word collapses only as an unpunctuated stutter
/// ("and and"); "No, no, no" and "bye bye" stay. Two-word repeats stay; three or more words said
/// twice is a decode loop. A restart set off by a comma is a stutter too, when the sentence goes
/// on: "the, the sensor", "I think, I think we should", "I, I, I don't".
pub fn collapse_dictation(text: &str) -> String {
    let text = &drop_cut_words(&collapse_triples(text));
    let once = run(text, true, 3);
    let mut toks = tokens(&once);
    let restarts = collapse_comma_restarts(&mut toks);
    let connectors = collapse_connector_restarts(&mut toks);
    if collapse_two_word_restarts(&mut toks) || restarts || connectors || shrink_of_the_loop(&mut toks, once != text.trim()) {
        toks.join(" ")
    } else {
        once
    }
}

/// "top of the top" left by a collapsed "the top of the top of the top" loop → "the top".
/// shortcut: only the whole-text 4-token case, widen if recordings show longer cut loops.
fn shrink_of_the_loop(toks: &mut Vec<String>, collapsed: bool) -> bool {
    let hit = collapsed
        && toks.len() == 4
        && token_key(&toks[1]) == "of"
        && token_key(&toks[2]) == "the"
        && same_words(toks, 0, 3, 1);
    if hit {
        toks.drain(..2);
    }
    hit
}

const RESTART_CONNECTORS: [&str; 4] = ["so", "and", "but", "like"];

/// "X so X more…" → "X more…" for a phrase X of up to four words, when the sentence goes on. A
/// single word with "and" stays: "more and more", "again and again".
fn collapse_connector_restarts(toks: &mut Vec<String>) -> bool {
    let mut changed = false;
    'again: loop {
        for n in 1..=MAX_RESTART_WORDS {
            for i in 0..toks.len().saturating_sub(2 * n + 1) {
                let connector = token_key(&toks[i + n]);
                let restart = RESTART_CONNECTORS.contains(&connector.as_str())
                    && !(n == 1 && connector == "and")
                    && same_words(toks, i, i + n + 1, n)
                    && (i..=i + n).all(|k| trailing_punctuation(&toks[k]).is_empty())
                    && !super::text::ends_sentence(&toks[i + 2 * n]);
                if restart {
                    let capital = toks[i].chars().next().is_some_and(char::is_uppercase);
                    toks.drain(i..i + n + 1);
                    if capital {
                        let mut chars = toks[i].chars();
                        if let Some(c) = chars.next() {
                            toks[i] = c.to_uppercase().chain(chars).collect();
                        }
                    }
                    changed = true;
                    continue 'again;
                }
            }
        }
        return changed;
    }
}

/// Said three times or more on purpose: "no no no", "ha ha ha", "very very very".
const DELIBERATE_TRIPLES: [&str; 8] = ["bye", "no", "yes", "yeah", "very", "ha", "hey", "so"];

/// "is is is" → "is": a word said three times without a comma is a stutter, even one that is
/// doubled on purpose ("what it is is").
fn collapse_triples(text: &str) -> String {
    let mut toks = tokens(text);
    let mut changed = false;
    let mut i = 0;
    while i + 2 < toks.len() {
        let key = token_key(&toks[i]);
        let triple = !key.is_empty()
            && same_words(&toks, i, i + 1, 1)
            && same_words(&toks, i, i + 2, 1)
            && trailing_punctuation(&toks[i]).is_empty()
            && trailing_punctuation(&toks[i + 1]).is_empty()
            && !DELIBERATE_TRIPLES.contains(&key.as_str())
            && !is_number_word(&key);
        if triple {
            while i + 1 < toks.len() && same_words(&toks, i, i + 1, 1) {
                let closing = trailing_punctuation(&toks[i + 1]).to_string();
                toks[i] = with_trailing_punctuation(&toks[i], &closing);
                toks.remove(i + 1);
            }
            changed = true;
        }
        i += 1;
    }
    if changed { toks.join(" ") } else { text.to_string() }
}

/// "update the dri- the driver" → "update the the driver": a word cut off mid-way ("dri-") and
/// said in full within the next three words goes; the stutter pass then takes the double "the".
fn drop_cut_words(text: &str) -> String {
    let toks = tokens(text);
    let cut = |i: usize| {
        let key = token_key(&toks[i]);
        toks[i].ends_with('-')
            && key.chars().count() >= 2
            && toks[i + 1..].iter().take(3).any(|t| {
                let next = token_key(t);
                next.len() > key.len() && next.starts_with(&key)
            })
    };
    let kept: Vec<&String> = (0..toks.len()).filter(|&i| !cut(i)).map(|i| &toks[i]).collect();
    if kept.len() == toks.len() { text.to_string() } else { kept.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ") }
}

/// Two words said twice in a row: "I think I think that" is a restart; "thank you thank you" is meant.
const DELIBERATE_PAIRS: [[&str; 2]; 2] = [["thank", "you"], ["bye", "bye"]];

fn collapse_two_word_restarts(toks: &mut Vec<String>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + 4 <= toks.len() {
        let pair = [token_key(&toks[i]), token_key(&toks[i + 1])];
        let deliberate = DELIBERATE_PAIRS.iter().any(|d| pair[0] == d[0] && pair[1] == d[1]);
        // Unpunctuated: "I think. I think so." is two sentences.
        if same_words(toks, i, i + 2, 2) && !deliberate && trailing_punctuation(&toks[i + 1]).is_empty() {
            let closing = trailing_punctuation(&toks[i + 3]).to_string();
            toks[i + 1] = with_trailing_punctuation(&toks[i + 1], &closing);
            toks.drain(i + 2..i + 4);
            changed = true;
            continue;
        }
        i += 1;
    }
    changed
}

/// Said on purpose even with a comma: "Bye, bye.", "Ha, ha", "No, no, no", "very, very easy".
const DELIBERATE_COMMA_DOUBLES: [&str; 8] = ["bye", "ha", "knock", "no", "really", "so", "very", "yes"];
const MAX_RESTART_WORDS: usize = 4;

/// "X, X more…" → "X more…" for a phrase X of up to four words. The second copy is kept, so its
/// own punctuation stays.
fn collapse_comma_restarts(toks: &mut Vec<String>) -> bool {
    let mut changed = false;
    'again: loop {
        for n in (1..=MAX_RESTART_WORDS).rev() {
            let mut i = 0;
            while i + 2 * n < toks.len() {
                let restart = (same_words(toks, i, i + n, n) || (n == 1 && cut_short(&toks[i], &toks[i + 1])))
                    && trailing_punctuation(&toks[i + n - 1]) == ","
                    && !super::text::ends_sentence(&toks[i + 2 * n - 1])
                    && !(n == 1 && DELIBERATE_COMMA_DOUBLES.contains(&token_key(&toks[i]).as_str()));
                if restart {
                    // The sentence's capital stays with its first word.
                    let capital = toks[i].chars().next().is_some_and(char::is_uppercase);
                    toks.drain(i..i + n);
                    if capital {
                        let mut chars = toks[i].chars();
                        if let Some(c) = chars.next() {
                            toks[i] = c.to_uppercase().chain(chars).collect();
                        }
                    }
                    changed = true;
                    continue 'again;
                }
                i += 1;
            }
        }
        return changed;
    }
}

fn run(text: &str, dictation: bool, min_phrase_words: usize) -> String {
    let normalized = collapse_whitespace(text);
    let mut toks = tokens(&normalized);
    if toks.len() < 2 {
        return normalized;
    }
    let max_words = if dictation { MAX_DICTATION_PHRASE_WORDS } else { MAX_PHRASE_WORDS };
    let mut changed = false;
    // Re-scan until stable: one collapse can expose another copy.
    loop {
        let mut round = collapse_repeats(&mut toks, 1, dictation.then_some(is_stutter as fn(&str) -> bool));
        let mut n = max_words.min(toks.len() / 2);
        while n >= min_phrase_words {
            round |= collapse_repeats(&mut toks, n, None);
            n -= 1;
        }
        changed |= round;
        if !round {
            break;
        }
    }
    if changed { toks.join(" ") } else { normalized }
}

/// "and and" is a stutter; "No, no" (punctuated), "bye bye" and doubled digits ("nine nine
/// eight", a phone number) are meant.
fn is_stutter(first: &str) -> bool {
    let key = token_key(first);
    trailing_punctuation(first).is_empty() && !DELIBERATE_DOUBLES.contains(&key.as_str()) && !is_number_word(&key)
}

/// Drop each back-to-back copy of an n-token phrase. The first copy's last token takes the dropped
/// copy's closing punctuation, so "No, no." becomes "No.".
fn collapse_repeats(toks: &mut Vec<String>, n: usize, allow: Option<fn(&str) -> bool>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + 2 * n <= toks.len() {
        if !same_words(toks, i, i + n, n) || allow.is_some_and(|f| !f(&toks[i + n - 1])) {
            i += 1;
            continue;
        }
        let closing = trailing_punctuation(&toks[i + 2 * n - 1]).to_string();
        toks[i + n - 1] = with_trailing_punctuation(&toks[i + n - 1], &closing);
        toks.drain(i + n..i + 2 * n);
        changed = true;
    }
    changed
}

/// "esp, esp32": a word cut short and then said in full.
fn cut_short(first: &str, second: &str) -> bool {
    let (a, b) = (token_key(first), token_key(second));
    a.chars().count() >= 3 && b.len() > a.len() && b.starts_with(&a)
}

fn same_words(toks: &[String], a: usize, b: usize, n: usize) -> bool {
    (0..n).all(|k| {
        let left = token_key(&toks[a + k]);
        !left.is_empty() && left == token_key(&toks[b + k])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_echoes_collapse() {
        assert_eq!(collapse("open open Spotify"), "open Spotify");
        assert_eq!(collapse("let's meet at 5 let's meet at 5"), "let's meet at 5");
    }

    #[test]
    fn closing_punctuation_is_kept() {
        assert_eq!(collapse("No, no. Stop"), "No. Stop");
        assert_eq!(collapse("Kee-cadd, right?"), "Kee-cadd, right?");
    }

    #[test]
    fn dictation_collapses_stutters_and_loops_only() {
        assert_eq!(
            collapse_dictation("And and it was not able to detect user user"),
            "And it was not able to detect user"
        );
        assert_eq!(
            collapse_dictation("what is the rating what is the rating of it"),
            "what is the rating of it"
        );
        assert_eq!(
            collapse_dictation("Decided to do everything to do everything to do everything to do everything"),
            "Decided to do everything"
        );
    }

    #[test]
    fn dictation_keeps_deliberate_repeats() {
        assert_eq!(collapse_dictation("thank you thank you so much"), "thank you thank you so much");
        assert_eq!(collapse_dictation("I think I think that was good"), "I think that was good");
        assert_eq!(collapse_dictation("bye bye now"), "bye bye now");
        assert_eq!(collapse_dictation("No, no, no"), "No, no, no");
        assert_eq!(collapse_dictation("nine nine eight"), "nine nine eight");
        assert_eq!(collapse_dictation("Bye, bye, see you."), "Bye, bye, see you.");
        assert_eq!(collapse_dictation("It was good, good."), "It was good, good.", "a repeat that ends the sentence stays");
    }

    #[test]
    fn long_loops_and_connector_restarts_collapse() {
        let loop9 = "I am on my way of turning to the";
        assert_eq!(collapse_dictation(&format!("{loop9} {loop9} store")), format!("{loop9} store"));
        assert_eq!(collapse_dictation(&format!("{loop9} {loop9} {loop9} store")), format!("{loop9} store"));
        assert_eq!(collapse_dictation("top of the top of the top"), "the top");
        assert_eq!(collapse_dictation("Those so those were the problems"), "Those were the problems");
        assert_eq!(collapse_dictation("I went home and I went home late"), "I went home late");
        assert_eq!(collapse_dictation("It got more and more crowded"), "It got more and more crowded");
        assert_eq!(collapse_dictation("It was that that mattered"), "It was that that mattered");
        assert_eq!(collapse_dictation("He had had enough"), "He had had enough");
    }

    #[test]
    fn a_restart_after_a_comma_keeps_one_copy() {
        for (raw, expected) in [
            ("The, the sensor is not responding.", "The sensor is not responding."),
            ("I think, I think we should change it.", "I think we should change it."),
            ("I, I, I don't think so.", "I don't think so."),
            ("We need to fix the, fix the connection issue.", "We need to fix the connection issue."),
            // 2.3, changed on purpose: a paused intensifier is emphasis, an unpunctuated one a stutter.
            ("It is very, very easy.", "It is very, very easy."),
            ("It is really, really good.", "It is really, really good."),
            ("It is very very important.", "It is very important."),
            ("Can you, can you open the browser?", "Can you open the browser?"),
            ("The esp, esp32 should connect.", "The esp32 should connect."),
            ("The device is is is not responding.", "The device is not responding."),
            ("We need to update the dri- the driver configuration.", "We need to update the driver configuration."),
            ("No no no, not that.", "No no no, not that."),
            ("It is a well-known issue.", "It is a well-known issue."),
        ] {
            assert_eq!(collapse_dictation(raw), expected, "{raw}");
        }
    }
}
