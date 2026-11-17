//! The only cleanup step that produces line breaks, so it runs last and only on a final. Port of
//! `DictationLayout.cs`. Two things: "new line" / "next line" / "new paragraph" said out loud, and a
//! counted list — "one I want this, two, I want that" becomes a numbered list, one item per line.
//! A counted series is one too: "the three things I need are A, B and C" (three items, said as
//! three) becomes a lead-in and a numbered list (`docs/VOICE_DECISIONS.md` V7, step 6).
//!
//! The recogniser punctuates counters badly ("…test the product three. I want…"), so only the first
//! counter has to open a clause; later ones need a clause start or punctuation of their own.
//! Counting ("one, two, three, go") and figures ("1 pm works, 2 pm also") stay prose. Lines are
//! separated by `\n`; the shell converts for the clipboard.

use super::numbers::is_unit_word;
use super::punctuation::{matches_words, MENTION_LEADS};
use super::text::{
    collapse_whitespace_keep_lines, ends_sentence, token_key, tokens, trailing_punctuation,
};
use super::{CleanupContext, CleanupMode};

const LINE_BREAKS: [&[&str]; 3] = [&["new", "line"], &["next", "line"], &["newline"]];
const PARAGRAPH_BREAKS: [&[&str]; 2] = [&["new", "paragraph"], &["next", "paragraph"]];

/// "you can say next line": naming the command is not giving it.
const NAMING_WORDS: [&str; 8] = ["say", "says", "said", "saying", "mention", "mentions", "mentioned", "called"];
const NAMING_WORD_EXTRA: &str = "word";

/// Joining words before a counter: "…this, and then two, …".
const JOINERS: [&str; 4] = ["then", "and", "next", "also"];

/// After a counter these make it a quantity or a pronoun, not an item number.
const NOT_AN_ITEM: [&str; 8] = ["of", "more", "or", "to", "and", "by", "at", "after"];

/// "One thing I noticed…", "One is enough": an unpunctuated "one" before these is prose.
const PROSE_AFTER_ONE: [&str; 37] = [
    "thing", "day", "time", "way", "reason", "problem", "issue", "question", "option", "example", "moment",
    "person", "guy", "side", "hand", "part", "point", "step", "last", "small", "big", "good", "bad", "quick",
    "simple", "other", "another", "such", "single", "is", "was", "has", "had", "can", "could", "should",
    "will",
];
const PROSE_AFTER_ONE_MORE: [&str; 3] = ["would", "might", "must"];

const COUNTERS: [&str; 21] = [
    "", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
    "thirteen", "fourteen", "fifteen", "sixteen", "seventeen", "eighteen", "nineteen", "twenty",
];

fn is_prose_after_one(key: &str) -> bool {
    PROSE_AFTER_ONE.contains(&key) || PROSE_AFTER_ONE_MORE.contains(&key) || key == "may"
}

fn is_mention_lead(key: &str) -> bool {
    MENTION_LEADS.contains(&key) || key == "and"
}

/// Lay out `text` (a final). Anything but formatted dictation passes through unchanged.
pub fn apply(text: &str, context: &CleanupContext) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    if !context.apply_formatting || context.mode != CleanupMode::Dictate {
        return text.to_string();
    }

    // Already laid-out text passes through line by line, so a second run changes nothing.
    let mut lines: Vec<String> = Vec::new();
    for existing in collapse_whitespace_keep_lines(text).split('\n') {
        if existing.is_empty() {
            lines.push(String::new());
            continue;
        }
        for line in split_spoken_breaks(&tokens(existing)) {
            if line.is_empty() {
                lines.push(String::new());
            } else {
                let listed = format_list(&line);
                match counted_series(&line) {
                    Some(series) if listed.len() == 1 => lines.extend(series),
                    _ => lines.extend(listed),
                }
            }
        }
    }

    number_counted_lines(&mut lines);
    // The line that leads into a list and the line after it start afresh, like the dictation
    // itself: "So I want three things:" → "I want three things:".
    for i in 0..lines.len() {
        let leads = lines.get(i + 1).is_some_and(|l| is_item(l));
        let follows = i > 0 && is_item(&lines[i - 1]);
        if !is_item(&lines[i]) && (leads || follows) {
            lines[i] = super::filler::strip_openers(&lines[i]);
        }
    }
    // Prose only: a list and its lead-in keep their shape.
    for line in &mut lines {
        if !line.is_empty() && !is_item(line) && !line.ends_with(':') {
            *line = topic_paragraphs(&join_short(line));
        }
    }
    one_blank_line(lines.join("\n").trim_matches('\n'))
}

/// (start, end) of each sentence in `toks`.
fn sentence_ranges(toks: &[String]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, t) in toks.iter().enumerate() {
        if ends_sentence(t) || i + 1 == toks.len() {
            out.push((start, i + 1));
            start = i + 1;
        }
    }
    out
}

/// Openers of a short sentence that carries on the one before (V46): "…a long session. So I want
/// to test it." → "…a long session, so I want to test it." With a comma after the opener
/// ("So, we restarted.") the speaker paused on purpose, and it stays.
const CARRY_ON: [&str; 4] = ["so", "and", "but", "because"];
const CARRY_ON_WORDS: usize = 8;
const PRONOUNS: [&str; 7] = ["i", "we", "you", "he", "she", "it", "they"];
const AUXILIARIES: [&str; 17] =
    ["is", "am", "are", "was", "were", "will", "would", "can", "could", "should", "have", "has", "had", "do", "did", "does", "may"];

/// Joins a short carry-on sentence to the one before, and puts the comma before "and" / "but"
/// that join two clauses ("…through my voice, and it is able to…"), as Wispr Flow writes them.
fn join_short(line: &str) -> String {
    let mut toks = tokens(line);
    for (s, e) in sentence_ranges(&toks) {
        let prev = s.checked_sub(1);
        let opener = token_key(&toks[s]);
        let bare = trailing_punctuation(&toks[s]).is_empty();
        let carries = bare
            && ((CARRY_ON.contains(&opener.as_str()) && e - s <= CARRY_ON_WORDS)
                || (CONDITIONAL_CARRY_ON.contains(&opener.as_str()) && is_fragment(&toks[s..e])));
        // A plain full stop only: "p.m." or "..." are not sentence ends to rewrite.
        if let Some(p) = prev.filter(|&p| carries && toks[p].ends_with('.') && !toks[p][..toks[p].len() - 1].contains('.')) {
            toks[p].pop();
            toks[p].push(',');
            toks[s] = toks[s].to_lowercase();
        }
        for j in s + 4..e.saturating_sub(2) {
            let clause = matches!(token_key(&toks[j]).as_str(), "and" | "but")
                && trailing_punctuation(&toks[j - 1]).is_empty()
                && trailing_punctuation(&toks[j]).is_empty()
                && opens_clause(&toks[j + 1], &toks[j + 2]);
            if clause {
                toks[j - 1].push(',');
            }
        }
    }
    toks.join(" ")
}

/// Openers that join only when the grammar says the sentence cannot stand alone (V46, owner
/// 2026-11-12): "We use BLE. Which saves power." → "We use BLE, which saves power."
const CONDITIONAL_CARRY_ON: [&str; 8] = ["which", "although", "though", "since", "unless", "until", "whereas", "or"];
const CONDITIONAL_WORDS: usize = 12;

/// A dependent clause with no main clause of its own: a statement, short, with no ", we …" after the
/// opening clause. "Although it works, we need to test it." is a whole sentence and stays.
fn is_fragment(sentence: &[String]) -> bool {
    let n = sentence.len();
    let statement = sentence.last().is_some_and(|t| t.ends_with('.'));
    let main_clause = (1..n.saturating_sub(1)).any(|j| {
        sentence[j].ends_with(',')
            && (PRONOUNS.contains(&token_key(&sentence[j + 1]).as_str())
                || matches!(token_key(&sentence[j + 1]).as_str(), "the" | "there" | "this" | "that"))
    });
    statement && n >= 2 && n <= CONDITIONAL_WORDS && !main_clause
}

/// "it is", "I am", "we'll": a subject and its verb, so a new clause starts here.
fn opens_clause(subject: &str, verb: &str) -> bool {
    let key = token_key(subject);
    let contracted = key.split_once('\'').is_some_and(|(p, _)| PRONOUNS.contains(&p));
    contracted || (PRONOUNS.contains(&key.as_str()) && trailing_punctuation(subject).is_empty() && AUXILIARIES.contains(&token_key(verb).as_str()))
}

/// Words that open a new topic (V47). A break goes before one only when the paragraph so far has
/// two sentences or more, so a short dictation stays one block.
const TOPIC_TURNS: [&[&str]; 12] = [
    &["also"], &["another", "thing"], &["one", "more", "thing"], &["moving", "on"], &["anyway"], &["apart", "from", "that"],
    &["on", "another", "note"], &["by", "the", "way"], &["coming", "to"], &["regarding"], &["other", "than", "that"], &["besides", "that"],
];

fn topic_paragraphs(line: &str) -> String {
    let toks = tokens(line);
    let mut out = String::new();
    let mut in_paragraph = 0;
    for (s, e) in sentence_ranges(&toks) {
        let turns = TOPIC_TURNS.iter().any(|w| matches_words(&toks, s, w));
        if turns && in_paragraph >= 2 {
            out.push_str("\n\n");
            in_paragraph = 0;
        } else if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&join(&toks, s, e));
        in_paragraph += 1;
    }
    out
}

/// Cuts at "new line" / "new paragraph"; an empty entry is the blank line of a paragraph break.
fn split_spoken_breaks(toks: &[String]) -> Vec<Vec<String>> {
    let mut lines: Vec<Vec<String>> = vec![Vec::new()];
    let mut i = 0;
    while i < toks.len() {
        let Some((length, paragraph)) = match_break(toks, i) else {
            let last = lines.last_mut().expect("never empty");
            let word = if last.is_empty() { capitalize(&toks[i]) } else { toks[i].clone() };
            last.push(word);
            i += 1;
            continue;
        };
        // "Hello there, new line, how are you": the commas were the model hearing the pause.
        if let Some(last_word) = lines.last_mut().and_then(|l| l.last_mut()) {
            *last_word = last_word.trim_end_matches(',').to_string();
        }
        if paragraph {
            lines.push(Vec::new());
        }
        lines.push(Vec::new());
        i += length;
    }
    lines
}

fn match_break(toks: &[String], index: usize) -> Option<(usize, bool)> {
    for words in PARAGRAPH_BREAKS {
        if is_break(toks, index, words) {
            return Some((words.len(), true));
        }
    }
    for words in LINE_BREAKS {
        if is_break(toks, index, words) {
            return Some((words.len(), false));
        }
    }
    None
}

/// "start a new line of code" and "the new paragraph" talk about one; they do not ask for one.
fn is_break(toks: &[String], index: usize, words: &[&str]) -> bool {
    if !matches_words(toks, index, words) {
        return false;
    }
    let end = index + words.len();
    if index > 0 && trailing_punctuation(&toks[index - 1]).is_empty() {
        let before = token_key(&toks[index - 1]);
        if is_mention_lead(&before)
            || NAMING_WORDS.contains(&before.as_str())
            || before == NAMING_WORD_EXTRA
            || matches!(before.as_str(), "to" | "in" | "on")
        {
            return false;
        }
    }
    end >= toks.len()
        || !trailing_punctuation(&toks[end - 1]).is_empty()
        || !matches!(token_key(&toks[end]).as_str(), "of" | "character" | "characters")
}

#[derive(Clone, Copy)]
struct Marker {
    start: usize,
    end: usize,
    /// "one is …": prose unless the sentence before gave the count.
    one_is: bool,
}

/// "1. Milk": a line of a numbered list.
fn is_item(line: &str) -> bool {
    line.split(' ').next().is_some_and(|w| w.len() > 1 && w.ends_with('.') && w[..w.len() - 1].chars().all(|c| c.is_ascii_digit()))
}

fn format_list(toks: &[String]) -> Vec<String> {
    let Some(markers) = try_find_markers(toks) else { return vec![toks.join(" ")] };

    let mut lines = Vec::new();
    let mut intro = join(toks, 0, strip_joiners(toks, 0, markers[0].start)).trim_end_matches([',', ';']).to_string();
    // "I need 3 things to be done. One, …": the sentence that gave the count leads into the list.
    if intro.ends_with('.') && announces(&intro, markers.len()) {
        intro.pop();
        intro.push(':');
        // What was said before that sentence is not part of the list: it keeps its own line.
        if let Some(cut) = intro.rfind(". ") {
            lines.push(intro[..=cut].to_string());
            intro = capitalize(&intro[cut + 2..]);
        }
    }
    if let Some(last) = intro.chars().next_back() {
        lines.push(if matches!(last, '.' | ':' | '?' | '!') { intro } else { format!("{intro}:") });
    }

    let mut rest = toks.len();
    for k in 0..markers.len() {
        let from = markers[k].end;
        let to;
        if k + 1 < markers.len() {
            to = strip_joiners(toks, from, markers[k + 1].start);
        } else {
            // Nothing says where the last item stops; its own sentence is the safe guess.
            let mut t = from;
            while t < toks.len() && !ends_sentence(&toks[t]) {
                t += 1;
            }
            to = toks.len().min(t + 1);
            rest = to;
        }
        lines.push(format!("{}. {}", k + 1, format_item(toks, from, to)));
    }

    if rest < toks.len() {
        lines.push(capitalize(&join(toks, rest, toks.len())));
    }
    lines
}

/// Paragraph breaks from long pauses, placed after cleanup. Each of `openings` is the first words
/// heard after a pause; a break goes before the first sentence, later than the last break, that
/// still opens with those words. Where cleanup took them away (a correction) or made them a list
/// line, or the pause fell inside a sentence, there is no such sentence and no break.
pub fn place_paragraphs(text: &str, openings: &[String]) -> String {
    let mut out = text.to_string();
    let mut from = 0;
    for opening in openings {
        let want: Vec<String> = opening.split_whitespace().map(token_key).filter(|k| !k.is_empty()).take(3).collect();
        if want.is_empty() {
            continue;
        }
        let found = sentence_starts(&out, from).into_iter().find(|&at| {
            let got: Vec<String> = out[at..].split_whitespace().map(token_key).filter(|k| !k.is_empty()).take(want.len()).collect();
            got == want
        });
        if let Some(at) = found {
            if !thought_boundary(&out, at) {
                continue;
            }
            out.replace_range(at - 1..at, "\n\n");
            from = at + 1;
        }
    }
    one_blank_line(&out)
}

/// Words that carry the thought before on: a pause before them is breath, not a new paragraph.
const CONTINUES: [&str; 18] = [
    "so", "and", "but", "because", "which", "although", "though", "since", "unless", "until", "or", "then", "it",
    "this", "that", "these", "those", "they",
];
const GREETINGS: [&str; 5] = ["hi", "hello", "hey", "dear", "good"];

/// A long pause is evidence, not a rule (owner, 2026-11-12): the sentence at `at` starts a new
/// paragraph when it turns to a new topic, follows a greeting, or follows two sentences or more
/// without carrying them on ("So…", "It…").
fn thought_boundary(text: &str, at: usize) -> bool {
    let paragraph = text[..at].rsplit("\n\n").next().unwrap_or("");
    let sentences = tokens(paragraph).iter().filter(|t| ends_sentence(t)).count();
    let next = tokens(&text[at..]);
    let topic = TOPIC_TURNS.iter().any(|w| matches_words(&next, 0, w));
    let greeting = sentences == 1 && tokens(paragraph).first().is_some_and(|w| GREETINGS.contains(&token_key(w).as_str()));
    let continues = next.first().is_some_and(|w| CONTINUES.contains(&token_key(w).as_str()));
    topic || greeting || (sentences >= 2 && !continues)
}

/// Paragraphs are separated by exactly one blank line.
fn one_blank_line(text: &str) -> String {
    let mut out = text.to_string();
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out
}

/// Byte offsets after `from` where a sentence starts after ". ", "? " or "! " on a line that is
/// not a list item.
fn sentence_starts(text: &str, from: usize) -> Vec<usize> {
    text.char_indices()
        .map(|(i, _)| i)
        .filter(|&i| i > from && i >= 2 && text[..i].ends_with(' ') && text[..i - 1].ends_with(['.', '?', '!']))
        .filter(|&i| {
            let line = text[..i].rsplit('\n').next().unwrap_or("");
            !line.split_whitespace().next().is_some_and(|w| w.ends_with('.') && w[..w.len() - 1].chars().all(|c| c.is_ascii_digit()))
        })
        .collect()
}

/// The last sentence of `intro` says how many items follow ("I need 3 things").
fn announces(intro: &str, count: usize) -> bool {
    let last = intro.trim_end_matches('.').rsplit(['.', '!', '?']).next().unwrap_or("");
    tokens(last).iter().any(|t| trailing_punctuation(t).is_empty() && series_count(t) == Some(count))
}

/// Counters 1, 2, 3… in order, each followed by at least one word of content. `None` unless there
/// are two or more.
fn try_find_markers(toks: &[String]) -> Option<Vec<Marker>> {
    for first in 0..toks.len() {
        let Some(marker) = try_read_marker(toks, first, 1) else { continue };
        // "First …, then …, finally …" (V55); without "finally" the counted reading is tried.
        if matches!(token_key(&toks[marker.start]).as_str(), "first" | "firstly") {
            let steps = collect_markers(toks, marker, true);
            let last = token_key(&toks[steps[steps.len() - 1].start]);
            let closed = matches!(last.as_str(), "finally" | "lastly");
            // By intent (owner, 2026-11-12): distinct actions make a list, a story told in order
            // does not. Three commands make one even without "finally".
            let (actions, commands) = step_intent(toks, &steps);
            if steps.len() >= 3 && actions && (closed || commands) && steps[steps.len() - 1].end < toks.len() {
                return Some(steps);
            }
        }
        let markers = collect_markers(toks, marker, false);
        let announced = !marker.one_is || announces(&join(toks, 0, marker.start), markers.len());
        if markers.len() >= 2 && markers[markers.len() - 1].end < toks.len() && announced {
            return Some(markers);
        }
    }
    None
}

/// Past forms that tell a story ("First we went to the shop, then we bought milk").
const PAST: [&str; 24] = [
    "went", "was", "were", "had", "did", "got", "came", "saw", "made", "took", "said", "found", "bought", "left",
    "ran", "told", "gave", "thought", "felt", "kept", "began", "spent", "sent", "put",
];
/// Words a step opens with when it has a subject; a step without one is a command.
const STEP_SUBJECTS: [&str; 14] = ["i", "we", "you", "they", "he", "she", "it", "the", "a", "this", "there", "my", "our", "that"];

/// (every step is a distinct action and none tells what happened, every step is a command).
fn step_intent(toks: &[String], steps: &[Marker]) -> (bool, bool) {
    let bodies: Vec<Vec<String>> = (0..steps.len())
        .map(|k| {
            let to = steps.get(k + 1).map_or_else(
                || (steps[k].end..toks.len()).find(|&t| ends_sentence(&toks[t])).map_or(toks.len(), |t| t + 1),
                |n| n.start,
            );
            toks[steps[k].end..to].iter().map(|t| token_key(t)).filter(|t| !t.is_empty() && t != "and").collect()
        })
        .collect();
    let past = |w: &str| PAST.contains(&w) || (w.len() > 4 && w.ends_with("ed") && !w.ends_with("eed"));
    let distinct = bodies.iter().enumerate().all(|(k, b)| !b.is_empty() && !bodies[..k].contains(b));
    let story = bodies.iter().any(|b| b.iter().take(3).any(|w| past(w)));
    let commands = bodies.iter().all(|b| b.first().is_some_and(|w| !STEP_SUBJECTS.contains(&w.as_str())));
    (distinct && !story, commands)
}

/// Words that mark the next step after "first": "then", "next", "finally", "lastly".
const STEPS: [&str; 4] = ["then", "next", "finally", "lastly"];

/// A step word opening a clause: at the start, after punctuation, or after "and".
fn try_read_step(toks: &[String], index: usize) -> Option<Marker> {
    let closing = trailing_punctuation(&toks[index]);
    let opens = index == 0 || !trailing_punctuation(&toks[index - 1]).is_empty() || token_key(&toks[index - 1]) == "and";
    (STEPS.contains(&token_key(&toks[index]).as_str()) && opens && (closing.is_empty() || closing == ",") && index + 1 < toks.len())
        .then_some(Marker { start: index, end: index + 1, one_is: false })
}

/// The markers that follow `marker` in order; with `steps`, step words count as the next one.
fn collect_markers(toks: &[String], marker: Marker, steps: bool) -> Vec<Marker> {
    let mut markers = vec![marker];
    let mut i = marker.end;
    while i < toks.len() {
        let found = try_read_marker(toks, i, markers.len() + 1).or_else(|| if steps { try_read_step(toks, i) } else { None });
        if let Some(next) = found {
            let prev_end = markers[markers.len() - 1].end;
            if strip_joiners(toks, prev_end, next.start) <= prev_end {
                break; // "one, two, three": counting, not a list.
            }
            markers.push(next);
            i = next.end;
            continue;
        }
        i += 1;
    }
    markers
}

/// "one," / "1." / "number two," / a bare "two" opening a clause. The first counter must open a
/// clause; a later one may instead be closed by punctuation ("…the product three. I want").
fn try_read_marker(toks: &[String], index: usize, value: usize) -> Option<Marker> {
    let mut at = index;
    if matches!(token_key(&toks[at]).as_str(), "number" | "point")
        && trailing_punctuation(&toks[at]).is_empty()
        && at + 1 < toks.len()
    {
        at += 1;
    }
    let token = &toks[at];
    if !is_counter(token, value) || at + 1 >= toks.len() {
        return None;
    }
    let closing = trailing_punctuation(token);
    let closed = closing.contains([',', '.', ':', ')']);
    if !closing.is_empty() && !closed {
        return None;
    }
    let starts_clause = index == 0
        || !trailing_punctuation(&toks[index - 1]).is_empty()
        || JOINERS.contains(&token_key(&toks[index - 1]).as_str());
    let rejected = if value == 1 { !starts_clause } else { !starts_clause && !closed };
    if rejected {
        return None;
    }
    let mut end = at + 1;
    let mut one_is = false;
    if !closed {
        let following = token_key(&toks[at + 1]);
        // "One is better performance, second is quick latency": the "is" is numbering too (V39).
        if following == "is" && trailing_punctuation(&toks[at + 1]).is_empty() && at + 2 < toks.len() {
            end += 1;
            one_is = value == 1;
        } else if NOT_AN_ITEM.contains(&following.as_str())
            || is_unit_word(&following)
            || (value == 1 && is_prose_after_one(&following))
        {
            return None;
        }
    }
    Some(Marker { start: index, end, one_is })
}

/// "first", "second"… also count items: "One, … Second, … Third, …".
const ORDINALS: [&str; 11] = ["", "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth", "tenth"];

fn is_counter(token: &str, value: usize) -> bool {
    if !token.chars().next().is_some_and(char::is_alphanumeric) {
        return false;
    }
    let key = token_key(token);
    let key = key.strip_suffix("ly").filter(|k| ORDINALS.contains(k)).unwrap_or(&key);
    key == value.to_string()
        || (value < COUNTERS.len() && key == COUNTERS[value])
        || (value < ORDINALS.len() && key == ORDINALS[value])
}

/// The speaker broke the lines themselves: "next line one never lose, next line two the world is
/// short". Lines that open with 1, 2, 3… in order become "1. …", "2. …".
fn number_counted_lines(lines: &mut Vec<String>) {
    let mut start = 0;
    while start < lines.len() {
        let mut run = 0;
        while start + run < lines.len() && opens_with_counter(&lines[start + run], run + 1) {
            run += 1;
        }
        if run < 2 {
            start += 1;
            continue;
        }
        for k in 0..run {
            let toks = tokens(&lines[start + k]);
            // "1. One is better performance": the number was said and written; once is enough (V39).
            let said = toks.len() > 3
                && toks[0].starts_with(|c: char| c.is_ascii_digit())
                && !toks[1].starts_with(|c: char| c.is_ascii_digit())
                && is_counter(&toks[1], k + 1)
                && token_key(&toks[2]) == "is";
            lines[start + k] = format!("{}. {}", k + 1, format_item(&toks, if said { 3 } else { 1 }, toks.len()));
        }
        // Models put a blank line between the lead-in and the list; the rules do not.
        if start >= 2 && lines[start - 1].is_empty() && lines[start - 2].ends_with(':') {
            lines.remove(start - 1);
            start -= 1;
        }
        start += run;
    }
}

fn opens_with_counter(line: &str, value: usize) -> bool {
    let toks = tokens(line);
    if toks.len() < 2 || !is_counter(&toks[0], value) {
        return false;
    }
    if !trailing_punctuation(&toks[0]).is_empty() {
        return true;
    }
    let following = token_key(&toks[1]);
    !NOT_AN_ITEM.contains(&following.as_str())
        && !is_unit_word(&following)
        && !(value == 1 && is_prose_after_one(&following))
}

/// Words that end a counted series' lead-in: "the 3 things are …", "… include …".
const SERIES_VERBS: [&str; 5] = ["are", "is", "were", "include", "includes"];
/// How far after the number the lead-in may end, and how long one item may be.
const SERIES_LEAD_WORDS: usize = 8;
const SERIES_ITEM_WORDS: usize = 10;

/// "The 3 things I need are A, B, and C." → the lead-in, then "1. A", "2. B", "3. C", when the
/// number said matches the items counted. Any other sentence of the line keeps its place.
fn counted_series(toks: &[String]) -> Option<Vec<String>> {
    let mut start = 0;
    while start < toks.len() {
        let end = (start..toks.len()).find(|&i| ends_sentence(&toks[i])).map_or(toks.len(), |i| i + 1);
        if let Some(lines) = series_in(toks, start, end) {
            let mut out = Vec::new();
            if start > 0 {
                out.push(join(toks, 0, start));
            }
            out.extend(lines);
            if end < toks.len() {
                out.push(capitalize(&join(toks, end, toks.len())));
            }
            return Some(out);
        }
        start = end;
    }
    None
}

fn series_in(toks: &[String], start: usize, end: usize) -> Option<Vec<String>> {
    for at in start..end {
        let Some(count) = series_count(&toks[at]) else { continue };
        // The lead-in ends at "are" / "include" or at a colon, soon after the number.
        let split = (at + 1..end.min(at + 2 + SERIES_LEAD_WORDS)).find(|&i| {
            trailing_punctuation(&toks[i]).ends_with(':') || (i > at + 1 && SERIES_VERBS.contains(&token_key(&toks[i]).as_str()))
        })?;
        let items = series_items(&join(toks, split + 1, end));
        if items.len() != count || items.iter().any(|i| i.is_empty() || i.split(' ').count() > SERIES_ITEM_WORDS) {
            return None;
        }
        let intro = join(toks, start, split + 1);
        let intro = intro.trim_end_matches([',', ':', ';']);
        let mut lines = vec![format!("{intro}:")];
        lines.extend(items.iter().enumerate().map(|(k, item)| format!("{}. {}", k + 1, capitalize(item))));
        return Some(lines);
    }
    None
}

/// 2 to 10, as a digit or a word, with nothing after it.
fn series_count(token: &str) -> Option<usize> {
    if !trailing_punctuation(token).is_empty() {
        return None;
    }
    let key = token_key(token);
    let n = key.parse().ok().or_else(|| COUNTERS.iter().position(|c| *c == key))?;
    (2..=10).contains(&n).then_some(n)
}

/// "A, B, and C." → [A, B, C]; "A or B" → [A, B].
fn series_items(text: &str) -> Vec<String> {
    let text = text.trim().trim_end_matches(['.', '!', '?']);
    let mut items: Vec<String> = text.split(", ").map(str::to_string).collect();
    let last = items.pop().unwrap_or_default();
    let last = last.strip_prefix("and ").or_else(|| last.strip_prefix("or ")).unwrap_or(&last).to_string();
    match last.rsplit_once(" and ").or_else(|| last.rsplit_once(" or ")) {
        // With an Oxford comma the last part is one item already.
        Some((a, b)) if items.iter().all(|i| !i.contains(" and ")) && !text.contains(", and ") && !text.contains(", or ") => {
            items.push(a.to_string());
            items.push(b.to_string());
        }
        _ => items.push(last),
    }
    items.into_iter().map(|i| i.trim().to_string()).collect()
}

/// End of the range once trailing "and" / "then" are dropped.
fn strip_joiners(toks: &[String], from: usize, mut to: usize) -> usize {
    while to > from
        && trailing_punctuation(&toks[to - 1]).is_empty()
        && JOINERS.contains(&token_key(&toks[to - 1]).as_str())
    {
        to -= 1;
    }
    to
}

fn format_item(toks: &[String], from: usize, to: usize) -> String {
    let mut item = join(toks, from, to).trim_end_matches([',', ';']).to_string();
    let sentences = (from..to).filter(|&i| ends_sentence(&toks[i])).count();
    // A one-clause item reads as a list entry without its full stop; longer ones keep theirs.
    if sentences <= 1 && item.ends_with('.') && to > 0 && ends_sentence(&toks[to - 1]) {
        item.pop();
    }
    capitalize(&item)
}

fn join(toks: &[String], from: usize, to: usize) -> String {
    if to <= from { String::new() } else { toks[from..to].join(" ") }
}

fn capitalize(text: &str) -> String {
    for (i, c) in text.char_indices() {
        if c.is_alphabetic() {
            if c.is_lowercase() {
                let mut out = String::with_capacity(text.len());
                out.push_str(&text[..i]);
                out.extend(c.to_uppercase());
                out.push_str(&text[i + c.len_utf8()..]);
                return out;
            }
            return text.to_string();
        }
        if c.is_numeric() {
            return text.to_string();
        }
    }
    text.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lay(text: &str) -> String {
        apply(text, &CleanupContext::dictate(true))
    }

    #[test]
    fn a_counted_list_is_laid_out_as_a_list() {
        assert_eq!(
            lay("And also, for example, 1, I'm saying this, then 2, something else."),
            "And also, for example:\n1. I'm saying this\n2. Something else"
        );
        assert_eq!(
            lay("One, buy milk. Two, call the bank. Three, send the invoice. Then we are done."),
            "1. Buy milk\n2. Call the bank\n3. Send the invoice\nThen we are done."
        );
        assert_eq!(lay("Number one, it is slow. Number two, it is ugly."), "1. It is slow\n2. It is ugly");
    }

    #[test]
    fn a_counted_series_becomes_a_list_when_the_count_matches() {
        assert_eq!(
            lay("The 3 things I need to complete are designing the PCB, writing the firmware, and testing the power consumption."),
            "The 3 things I need to complete are:\n1. Designing the PCB\n2. Writing the firmware\n3. Testing the power consumption"
        );
        assert_eq!(lay("We have two options: retry or give up. Pick one."), "We have two options:\n1. Retry\n2. Give up\nPick one.");
        assert_eq!(lay("Hi Sam. The 2 blockers are power and noise"), "Hi Sam.\nThe 2 blockers are:\n1. Power\n2. Noise");
        for prose in [
            "The 3 things I need are designing the PCB and writing the firmware.",
            "I bought 2 apples and a pear.",
            "There are 3 of us, me, you and Sam, and that is all we have for the trip, which is fine.",
        ] {
            assert_eq!(lay(prose), prose, "the count does not match, or there is no lead-in");
        }
    }

    #[test]
    fn pause_breaks_land_only_where_a_sentence_still_starts() {
        let open = |o: &[&str]| o.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert_eq!(place_paragraphs("Hi Sam. The demo went well.", &open(&["The demo went"])), "Hi Sam.\n\nThe demo went well.");
        // A correction took the words away: no break.
        assert_eq!(place_paragraphs("I want the mail by Saturday.", &open(&["No, Saturday."])), "I want the mail by Saturday.");
        // They became a list line: the list has its own lines.
        let list = "I need 3 things:\n1. Bug fixed\n2. Runs smoothly";
        assert_eq!(place_paragraphs(list, &open(&["Second, runs smoothly."])), list);
        // A pause in the middle of a sentence is not a paragraph.
        assert_eq!(place_paragraphs("We should ship it on Friday.", &open(&["ship it on"])), "We should ship it on Friday.");
        // Order is kept, and a repeated opening matches the later sentence.
        assert_eq!(place_paragraphs("Yes. Ok. Yes. Ok.", &open(&["Yes.", "Yes."])), "Yes. Ok.\n\nYes. Ok.");
    }

    #[test]
    fn a_pause_breaks_only_at_a_new_thought() {
        let open = |o: &[&str]| o.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        // One sentence so far, or a sentence that carries the last one on: breath, not a paragraph.
        let one = "The board works. The battery lasts a day.";
        assert_eq!(place_paragraphs(one, &open(&["The battery lasts"])), one);
        let carried = "The board works. The battery is new. It lasts a day.";
        assert_eq!(place_paragraphs(carried, &open(&["It lasts a"])), carried);
        // Two sentences and a fresh one, or a new topic: a paragraph.
        assert_eq!(
            place_paragraphs("The board works. The battery is new. We ship on Monday.", &open(&["We ship on"])),
            "The board works. The battery is new.\n\nWe ship on Monday."
        );
        assert_eq!(
            place_paragraphs("The board works. Also, the icon is old.", &open(&["Also, the icon"])),
            "The board works.\n\nAlso, the icon is old."
        );
        // Exactly one blank line between paragraphs.
        assert_eq!(lay("first topic done. New paragraph. New paragraph. Second topic starts"), "First topic done.\n\nSecond topic starts");
    }

    #[test]
    fn ordinals_count_items_too() {
        assert_eq!(
            lay("So, I need 3 things to be done. One, bug should be fixed. Second, they should be running smoothly. Third, the response time should be very smooth."),
            "I need 3 things to be done:\n1. Bug should be fixed\n2. They should be running smoothly\n3. The response time should be very smooth"
        );
        assert_eq!(lay("First, open it. Secondly, close it."), "1. Open it\n2. Close it");
        assert_eq!(lay("Wait a second, I am coming."), "Wait a second, I am coming.");
    }

    #[test]
    fn spoken_numbering_is_dropped_from_the_items() {
        let list = "I want three things:\n1. Better performance\n2. Quick latency\n3. Faster response time";
        assert_eq!(
            lay("It works. So I want three things. One is better performance, second is quick latency. Third is faster response time. So these are what I need."),
            format!("It works.\n{list}\nThese are what I need.")
        );
        // A model numbered the lines and kept the spoken numbers.
        assert_eq!(lay("So, I want three things:\n1. One is better performance\n2. Second is quick latency\n3. Third is faster response time"), list);
        // Stays as spoken: no count was given, or "one" is a word of the item.
        assert_eq!(lay("One is enough, two is too many."), "One is enough, two is too many.");
        let kept = "Options:\n1. One of the options is cheaper\n2. Second place is fine";
        assert_eq!(lay(kept), kept);
    }

    #[test]
    fn first_then_finally_is_a_list() {
        assert_eq!(
            lay("Make a task list, first check the hardware, then test the firmware and finally deploy the application."),
            "Make a task list:\n1. Check the hardware\n2. Test the firmware\n3. Deploy the application"
        );
        // No "finally": two steps in a sentence are prose.
        let prose = "First I check the board, then I call you.";
        assert_eq!(lay(prose), prose);
        // Three commands are a list without "finally"; a story told in order is not.
        assert_eq!(
            lay("First update the configuration, then restart the device, then check the logs."),
            "1. Update the configuration\n2. Restart the device\n3. Check the logs"
        );
        for prose in [
            "First we went to the shop, then we bought milk and finally we came home.",
            "First I tested the board, then I checked the logs and finally I called Sam.",
            "First I check the board, then I call you, then I go home.",
        ] {
            assert_eq!(lay(prose), prose);
        }
        // An ordinal list with a "then" after it stays an ordinal list.
        assert_eq!(lay("First, open it. Secondly, close it. Then we are done."), "1. Open it\n2. Close it\nThen we are done.");
    }

    #[test]
    fn a_short_carry_on_sentence_joins_the_one_before() {
        assert_eq!(
            lay("This is a test for a long session. So I want to test it."),
            "This is a test for a long session, so I want to test it."
        );
        assert_eq!(
            lay("I am typing through my voice and it is able to set it."),
            "I am typing through my voice, and it is able to set it."
        );
        for kept in [
            "It failed. So, we restarted.",
            "Meet at 3 p.m. So we can talk.",
            "It failed. So we restarted the whole system and checked every cable again.",
            "Tom and I went home.",
        ] {
            assert_eq!(lay(kept), kept);
        }
    }

    #[test]
    fn a_conditional_opener_joins_only_a_fragment() {
        assert_eq!(lay("We use the BLE module. Which saves a lot of power."), "We use the BLE module, which saves a lot of power.");
        assert_eq!(lay("We should ship it. Although it is not tested yet."), "We should ship it, although it is not tested yet.");
        for kept in [
            "It worked. Although it is slow, we can ship it.",
            "Which one do you want?",
            "Pick a slot. Which one do you want?",
            "It failed. Since then, the board has been fine.",
            "We can wait. Or, we can ship it today.",
        ] {
            assert_eq!(lay(kept), kept);
        }
    }

    #[test]
    fn a_new_topic_starts_a_paragraph() {
        assert_eq!(
            lay("The board works. The battery lasts a day. Also, the app needs a new icon."),
            "The board works. The battery lasts a day.\n\nAlso, the app needs a new icon."
        );
        let short = "The board works. Also, the app needs a new icon.";
        assert_eq!(lay(short), short);
        // Laid out once, it stays.
        let done = lay("The board works. The battery lasts a day. Also, the app needs a new icon.");
        assert_eq!(lay(&done), done);
    }

    #[test]
    fn counting_and_prose_are_not_lists() {
        for raw in [
            "One, two, three, go!",
            "One, I do not know.",
            "One thing I noticed, two of the buttons are swapped.",
            "It starts today. 1 pm works. 2 pm also works.",
        ] {
            assert!(!lay(raw).contains('\n'), "{raw}");
        }
    }

    #[test]
    fn unpunctuated_lists_are_still_lists() {
        assert_eq!(
            lay("Hello Mayurana, this is Danush. So I wanted to test the typing. One I want to complete the demo video. Two, I want to test the product three. I want to test the overall feature."),
            "Hello Mayurana, this is Danush, so I wanted to test the typing.\n1. I want to complete the demo video\n2. I want to test the product\n3. I want to test the overall feature"
        );
        assert_eq!(
            lay("For example, one I am saying this, two I am saying something else."),
            "For example:\n1. I am saying this\n2. I am saying something else"
        );
    }

    #[test]
    fn lines_the_speaker_broke_and_counted_are_numbered() {
        assert_eq!(
            lay("That is what I am trying to type. Next line. One never lose, either I win or I learn. Next line two. The world is short."),
            "That is what I am trying to type.\n1. Never lose, either I win or I learn\n2. The world is short"
        );
    }

    #[test]
    fn new_line_and_new_paragraph_break_the_text() {
        assert_eq!(lay("hello there new line how are you"), "Hello there\nHow are you");
        assert_eq!(
            lay("first topic done. New paragraph. Second topic starts"),
            "First topic done.\n\nSecond topic starts"
        );
    }

    #[test]
    fn naming_a_break_does_not_run_it() {
        assert!(!lay("he can mention next line so that it goes to the next line").contains('\n'));
        assert!(!lay("add a new line of code here").contains('\n'));
    }

    #[test]
    fn layout_is_stable_and_final_only() {
        let laid_out = "Shopping:\n1. Milk\n2. Eggs\n\nThen call me.";
        assert_eq!(lay(laid_out), laid_out);
        assert_eq!(apply("one, milk, two, eggs", &CleanupContext::dictate(false)), "one, milk, two, eggs");
        assert_eq!(apply("new line", &CleanupContext::command()), "new line");
    }
}
