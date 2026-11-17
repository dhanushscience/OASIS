//! Spoken self-corrections in dictation: "let's meet on Tuesday 2 pm. No, actually Wednesday
//! 3 p.m." → "let's meet on Wednesday 3 p.m.". Port of `SpokenRepair.cs`.
//!
//! The words after the cue replace what they correct, and that is only knowable when the two line
//! up: a day for a day, a month for a month, a number or time for a number, or the same two words
//! said again ("send it to John, no, actually send it to Mark"). With nothing to line up, the
//! sentence is left exactly as spoken — "No, actually it was fine." is an answer, not a correction.

use super::numbers::{is_month, is_number_word, split};
use super::text::{collapse_whitespace, ends_sentence, token_key, tokens, trailing_punctuation};

/// Unmistakable corrections; may follow a full stop the model put before "No".
const STRONG_CUES: [&[&str]; 13] = [
    &["no", "i", "meant"], &["no", "i", "mean"], &["sorry", "i", "meant"], &["sorry", "i", "mean"],
    &["let's", "make", "that"], &["lets", "make", "that"],
    &["no", "actually"], &["actually", "no"], &["actually", "wait"], &["no", "wait"], &["no", "sorry"],
    &["oh", "sorry"], &["i", "meant"],
];

/// Ordinary words most of the time; a correction only inside one sentence, and briefly.
const WEAK_CUES: [&[&str]; 8] = [&["or", "rather"], &["make", "that"], &["make", "it"], &["i", "mean"], &["actually"], &["wait"], &["sorry"], &["no"]];

/// "next Monday" corrects "Friday" without taking the "by" before it: these go with the new day
/// only.
const DAY_MODIFIERS: [&str; 4] = ["next", "this", "last", "coming"];

const PARTS_OF_DAY: [&str; 5] = ["morning", "afternoon", "evening", "night", "noon"];

/// "…by Thursday. Yeah." — the yes that closes a correction is not part of the text.
const CONFIRMATIONS: [&str; 6] = ["yeah", "yes", "yep", "yup", "right", "okay"];

const DAYS: [&str; 11] = [
    "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday", "today", "tomorrow",
    "tonight", "yesterday",
];

/// Little words that travel with the corrected item: "at 5 … at 6", "on Tuesday … Wednesday".
const LEAD_INS: [&str; 15] =
    ["at", "on", "in", "to", "by", "for", "the", "a", "an", "around", "about", "till", "until", "from", "every"];

const LOOK_BACK_TOKENS: usize = 15;
const WEAK_CUE_MAX_WORDS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    None,
    Day,
    Month,
    Number,
}

/// Apply spoken corrections until none is left (at most eight rounds).
pub fn apply(text: &str) -> String {
    let mut current = collapse_whitespace(text);
    for _ in 0..8 {
        // "no—actually" is two words to the speaker; a dash must not hide the cue.
        let toks = tokens(&current.replace('—', "— "));
        match try_repair_once(&toks) {
            Some(repaired) => current = repaired.join(" "),
            None => return current,
        }
    }
    current
}

fn try_repair_once(toks: &[String]) -> Option<Vec<String>> {
    for cue in 1..toks.len().saturating_sub(1) {
        let Some((length, strong)) = try_match_cue(toks, cue) else { continue };

        let before = &toks[cue - 1];
        let start = cue + length;
        let count = sentence_length(toks, start);
        // "Is it Tuesday? No, actually Wednesday." answers the question; both halves stay. "Can you
        // send it by Monday? No, wait, by Thursday." corrects it: the same little word leads both.
        let asked = trailing_punctuation(before).contains('?') && !is_name_fragment(toks, cue);
        if asked && !(strong && opens_with_lead_in(toks, start, count)) {
            continue;
        }
        // A cue that opens a sentence corrects the sentence before it, when the two line up.
        if ends_sentence(before) && count > 0 {
            if let Some(repaired) = correct_previous_sentence(toks, cue, start, count) {
                return Some(repaired);
            }
            if !strong {
                continue;
            }
        }
        if count == 0 || (!strong && count > WEAK_CUE_MAX_WORDS) {
            continue;
        }
        if let Some(repaired) = try_replace(toks, cue, start, count, strong) {
            return Some(repaired);
        }
    }
    None
}

/// The cue at `index`, and any cues straight after it ("actually, wait, let's make that" is one).
fn try_match_cue(toks: &[String], index: usize) -> Option<(usize, bool)> {
    let (mut length, mut strong) = match_one_cue(toks, index)?;
    while let Some((more, more_strong)) = match_one_cue(toks, index + length) {
        length += more;
        strong |= more_strong;
    }
    Some((length, strong))
}

fn match_one_cue(toks: &[String], index: usize) -> Option<(usize, bool)> {
    for words in STRONG_CUES {
        if matches_cue(toks, index, words) {
            return Some((words.len(), true));
        }
    }
    for words in WEAK_CUES {
        if !matches_cue(toks, index, words) {
            continue;
        }
        // "wait", "sorry" and "no" are corrections only when set off: "at 4, wait, 5".
        let set_off = !trailing_punctuation(&toks[index - 1]).is_empty()
            || !trailing_punctuation(&toks[index + words.len() - 1]).is_empty();
        if matches!(words, ["wait"] | ["sorry"] | ["no"]) && !set_off {
            continue;
        }
        // "final version, no, final version two": a "no" between two commas is as clear as "no, I meant".
        let between_commas = toks[index - 1].ends_with(',') && toks[index + words.len() - 1].ends_with(',');
        return Some((words.len(), matches!(words, ["no"]) && between_commas));
    }
    None
}

/// "For Shashank?", "Dhanush?": the sentence before the cue is only a name, so its question mark
/// is the speaker doubting the name, not asking something.
fn is_name_fragment(toks: &[String], cue: usize) -> bool {
    let first = (0..cue - 1).rev().find(|&i| ends_sentence(&toks[i])).map_or(0, |i| i + 1);
    let names = toks[first..cue].iter().filter(|t| !LEAD_INS.contains(&token_key(t).as_str())).count();
    cue - first <= 3 && names >= 1 && toks[first..cue].iter().all(|t| t.chars().next().is_some_and(char::is_uppercase))
}

/// The correction opens with a lead-in and an item: "by Thursday", "on Monday".
fn opens_with_lead_in(toks: &[String], start: usize, count: usize) -> bool {
    count >= 2 && LEAD_INS.contains(&token_key(&toks[start]).as_str()) && classify(&toks[start + 1]) != Class::None
}

/// Words that judge the correction rather than add to it: "February would be better", "archive
/// it instead". Dropped before lining up.
const VERDICTS: [&[&str]; 8] = [
    &["would", "be", "better"], &["will", "be", "better"], &["is", "better"], &["works", "better"],
    &["would", "be", "fine"], &["is", "fine"], &["instead"], &["rather"],
];
/// Words that add rather than replace: "Actually, we bought a blue one too" keeps both.
const ADDING: [&str; 4] = ["too", "also", "again", "well"];
/// Words in a re-said phrase that may be left out without losing anything: "No, just the
/// transcript", "Wait, send it as …".
const DROPPABLE: [&str; 4] = ["just", "it", "them", "that"];
/// Little words too common to line two phrases up on their own.
const ARTICLES: [&str; 3] = ["a", "an", "the"];
/// Words that make the rest of a correction a new statement, not a replacement.
const VERBS_AFTER: [&str; 12] = ["is", "are", "was", "were", "be", "would", "will", "should", "could", "can", "do", "does"];
const MAX_RESAID_WORDS: usize = 10;
const MAX_REPLACED_WORDS: usize = 8;

/// "…on Wednesday. Actually, make it Friday." and the like: the sentence after the cue replaces
/// the part of the sentence before it that it lines up with. `None` when nothing lines up.
fn correct_previous_sentence(toks: &[String], cue: usize, start: usize, count: usize) -> Option<Vec<String>> {
    let end = start + count;
    let closing = sentence_closing(&toks[end - 1]).to_string();
    // The correction's words, without a verdict at its end, and with its closing kept.
    let mut said: Vec<String> = toks[start..end].to_vec();
    if let Some(last) = said.last_mut() {
        last.truncate(last.len() - closing.len());
    }
    let keys: Vec<String> = said.iter().map(|t| token_key(t)).collect();
    if let Some(v) = VERDICTS.iter().find(|v| keys.len() > v.len() && keys.ends_with(&v.iter().map(|w| (*w).to_string()).collect::<Vec<_>>())) {
        said.truncate(said.len() - v.len());
    }
    if said.is_empty() || said.len() > MAX_RESAID_WORDS || said.iter().any(|t| ADDING.contains(&token_key(t).as_str())) {
        return None;
    }
    if let Some(last) = said.last_mut() {
        last.push_str(&closing);
    }
    let first = (0..cue - 1).rev().find(|&i| ends_sentence(&toks[i])).map_or(0, |i| i + 1);
    // The sentence before, without its closing mark; replaced parts take the correction's.
    let mut before: Vec<String> = toks[first..cue].to_vec();
    if let Some(last) = before.last_mut() {
        *last = last.trim_end_matches(['.', '!', '?', ',']).to_string();
    }

    let mut fixed = by_item(&before, &said).or_else(|| by_words(&before, &said)).or_else(|| by_name(&before, &said))?;
    // "Let's use Python. Actually, let's use C++.": the sentence keeps its capital.
    if before[0].chars().next().is_some_and(char::is_uppercase) {
        if let Some(word) = fixed.first_mut() {
            let mut chars = word.chars();
            if let Some(c) = chars.next() {
                *word = c.to_uppercase().chain(chars).collect();
            }
        }
    }
    let mut repaired: Vec<String> = toks[..first].to_vec();
    // "…a mail on Thursday. For Shashank." — a lead-in and a name is no sentence: it ends the one
    // before it.
    let fragment = fixed.len() <= 3 && closing == "." && LEAD_INS.contains(&token_key(&fixed[0]).as_str()) && is_name_fragment(&fixed, fixed.len());
    if let Some(last) = repaired.last_mut().filter(|last| fragment && sentence_closing(last) == ".") {
        last.pop();
        fixed[0] = fixed[0].to_lowercase();
    }
    repaired.extend(fixed);
    repaired.extend(after_correction(toks, end));
    Some(repaired)
}

/// A day, month or number for one of its own kind: "…at 10 AM. No, 11:30 AM.", "…five interns.
/// Actually, three interns.", "…today. No, tomorrow morning."
fn by_item(before: &[String], said: &[String]) -> Option<Vec<String>> {
    let lead = |t: &str| LEAD_INS.contains(&t) || DAY_MODIFIERS.contains(&t);
    let at = if classify(&said[0]) != Class::None { 0 } else if said.len() > 1 && lead(&token_key(&said[0])) && classify(&said[1]) != Class::None { 1 } else { return None };
    if said[at + 1..].iter().any(|t| VERBS_AFTER.contains(&token_key(t).as_str())) {
        return None;
    }
    let mut toks: Vec<String> = before.to_vec();
    let cue = toks.len();
    toks.push("no,".into());
    toks.extend(said.iter().cloned());
    let repaired = try_replace(&toks, cue, cue + 1, said.len(), true)?;
    (repaired != toks).then_some(repaired)
}

/// The correction says part of the sentence again: from the same words ("Let's use Python.
/// Actually, let's use C++."), or ending on the same words ("I want the blue one. No, the black
/// one."). The two must differ, and nothing said is lost except a re-said verb or "just".
fn by_words(before: &[String], said: &[String]) -> Option<Vec<String>> {
    let bk: Vec<String> = before.iter().map(|t| token_key(t)).collect();
    let sk: Vec<String> = said.iter().map(|t| token_key(t)).collect();

    // Same start: said[j..] goes on from before[p..] for m words, then both go their own way.
    let mut best: Option<(usize, usize, usize)> = None; // (m, j, p)
    for j in 0..sk.len().min(4) {
        if !sk[..j].iter().all(|w| DROPPABLE.contains(&w.as_str()) || bk.contains(w)) {
            break;
        }
        for p in (0..bk.len()).rev() {
            let m = bk[p..].iter().zip(&sk[j..]).take_while(|(a, b)| a == b).count();
            let shared_enough = m >= 2 || (m == 1 && !ARTICLES.contains(&sk[j].as_str()));
            let both_differ = p + m < bk.len() && j + m < sk.len();
            if shared_enough && both_differ && bk.len() - p <= MAX_REPLACED_WORDS && best.is_none_or(|(bm, bj, _)| m > bm || (m == bm && j < bj)) {
                best = Some((m, j, p));
            }
        }
    }
    if let Some((_, j, p)) = best {
        let mut out = before[..p].to_vec();
        out.extend(said[j..].iter().cloned());
        return Some(out);
    }

    // Same end: "the hardware team" / "the firmware team" — the words before the shared end swap.
    let shared = bk.iter().rev().zip(sk.iter().rev()).take_while(|(a, b)| a == b).count();
    let fresh = sk.len().checked_sub(shared).filter(|n| (1..=3).contains(n))?;
    let old_from = bk.len().checked_sub(shared + fresh)?;
    if shared == 0 || old_from == 0 || bk[old_from..bk.len() - shared] == sk[..fresh] {
        return None;
    }
    let mut out = before[..old_from].to_vec();
    out.extend(said.iter().cloned());
    Some(out)
}

/// One name for another at the end: "I want to travel to Mumbai. No, Bengaluru.", "For Dhanush?
/// No, wait, actually it is Shashank." A day or a month is not a name: those line up by kind.
fn by_name(before: &[String], said: &[String]) -> Option<Vec<String>> {
    let capital = |t: &String| t.chars().next().is_some_and(char::is_uppercase);
    let name = |t: &String| capital(t) && classify(t) == Class::None && !LEAD_INS.contains(&token_key(t).as_str());
    let said = match said {
        [it, is, rest @ ..] if token_key(it) == "it" && token_key(is) == "is" => rest,
        [its, rest @ ..] if token_key(its) == "it's" => rest,
        _ => said,
    };
    if said.is_empty() || said.len() > 2 || !said.iter().all(name) {
        return None;
    }
    let run = before.iter().rev().take_while(|t| name(t)).count();
    if run == 0 || run >= before.len() {
        return None;
    }
    let mut out = before[..before.len() - run].to_vec();
    out.extend(said.iter().cloned());
    Some(out)
}

fn matches_cue(toks: &[String], index: usize, words: &[&str]) -> bool {
    if index + words.len() >= toks.len() {
        return false;
    }
    words.iter().enumerate().all(|(k, w)| token_key(&toks[index + k]) == *w)
}

/// Tokens from `start` through the end of that sentence.
fn sentence_length(toks: &[String], start: usize) -> usize {
    (start..toks.len()).find(|&i| ends_sentence(&toks[i])).map_or(toks.len() - start, |i| i - start + 1)
}

fn try_replace(toks: &[String], cue: usize, start: usize, count: usize, strong: bool) -> Option<Vec<String>> {
    let modifier = DAY_MODIFIERS.contains(&token_key(&toks[start]).as_str());
    let lead = usize::from(
        count >= 2
            && (LEAD_INS.contains(&token_key(&toks[start]).as_str()) || modifier)
            && classify(&toks[start + 1]) != Class::None,
    );
    let kind = classify(&toks[start + lead]);

    // The correction may reach back over the full stop right before the cue, no further.
    let mut floor = cue.saturating_sub(LOOK_BACK_TOKENS);
    let mut i = cue as isize - 2;
    while i >= floor as isize {
        if ends_sentence(&toks[i as usize]) {
            floor = i as usize + 1;
            break;
        }
        i -= 1;
    }

    let mut item = toks[start + lead].clone();
    let mut anchor;
    let class_anchor = if kind == Class::None { None } else { find_class_anchor(toks, floor, cue, kind, start + lead, start + count) };
    if class_anchor.is_none() && strong && lead == 1 && !modifier {
        // Nothing of the same kind to replace (the model misheard it: "by when this day"), but the
        // same lead-in word is there: everything from it to the cue is what is being corrected.
        let word = token_key(&toks[start]);
        let from = (floor..cue).rev().find(|&i| token_key(&toks[i]) == word)?;
        let mut repaired: Vec<String> = toks[..from].to_vec();
        repaired.extend(toks[start..start + count].iter().cloned());
        repaired.extend(after_correction(toks, start + count));
        return Some(repaired);
    }
    let replace = if kind != Class::None {
        anchor = class_anchor?;
        item = match_clock_form(&toks[anchor], &item, anchor > 0 && token_key(&toks[anchor - 1]) == "at");

        // "version 2 is actually 3 times faster" is not a correction. A bare "actually" or "I mean"
        // counts only when the item is all that follows, or sits right next to it.
        // "every 5 seconds, no every 10 seconds": the same unit after both counts as the item alone.
        let same_unit = count - lead == 2 && anchor + 1 < cue && token_key(&toks[start + lead + 1]) == token_key(&toks[anchor + 1]);
        let item_only = count - lead == 1
            || same_unit
            || (count - lead == 2 && matches!(token_key(&toks[start + lead + 1]).as_str(), "am" | "pm"));
        if !strong && !item_only && anchor != cue - 1 {
            return None;
        }

        let mut matched = count;
        if lead == 1 {
            let lead_words: &[&str] = if modifier { &DAY_MODIFIERS } else { &LEAD_INS };
            let has_lead = anchor > floor
                && lead_words.contains(&token_key(&toks[anchor - 1]).as_str())
                && trailing_punctuation(&toks[anchor - 1]).is_empty();
            if has_lead {
                anchor -= 1;
            } else {
                matched -= 1;
            }
        }
        let mut replace = matched.min(cue - anchor);
        // "tomorrow morning … No, on Monday": the morning was part of what is replaced.
        let said_part = toks[start..start + count].iter().any(|t| PARTS_OF_DAY.contains(&token_key(t).as_str()));
        if kind == Class::Day && !said_part && anchor + replace < cue && PARTS_OF_DAY.contains(&token_key(&toks[anchor + replace]).as_str()) {
            replace += 1;
        }
        replace
    } else {
        // Said again from the same two words: everything from there on is superseded. Only after an
        // explicit "no": "it was good, actually it was great" keeps both halves.
        anchor = if strong { find_retrace_anchor(toks, floor, cue, start, count)? } else { return None };
        cue - anchor
    };

    let mut repaired: Vec<String> = toks[..anchor].to_vec();
    for (i, token) in toks.iter().enumerate().skip(start).take(count) {
        repaired.push(if i == start + lead { item.clone() } else { token.clone() });
    }

    let kept_from = anchor + replace;
    if kept_from < cue {
        // "2 kids, I mean 3" → "3 kids": the kept tail ends the clause the correction ended.
        let closing = sentence_closing(&toks[start + count - 1]).to_string();
        if let Some(last) = repaired.last_mut() {
            last.truncate(last.len() - closing.len());
        }
        repaired.extend(toks[kept_from..cue].iter().cloned());
        let last = repaired.last_mut().expect("not empty");
        let mut tail = last.trim_end_matches([',', ';']).to_string();
        if ends_sentence(&tail) {
            tail = tail.trim_end_matches(['.', '!', '?']).to_string();
        }
        *last = format!("{tail}{closing}");
    }

    repaired.extend(after_correction(toks, start + count));
    Some(repaired)
}

/// What follows a correction, without a lone "Yeah." that only confirmed it.
fn after_correction(toks: &[String], from: usize) -> impl Iterator<Item = String> + '_ {
    let confirms = from < toks.len() && ends_sentence(&toks[from]) && CONFIRMATIONS.contains(&token_key(&toks[from]).as_str());
    toks[from + usize::from(confirms)..].iter().cloned()
}

/// Last token of the same class before the cue. With several ("3 apples for the 2 of us, no
/// actually 4 apples"), the one followed by the same word as the correction wins.
fn find_class_anchor(toks: &[String], floor: usize, cue: usize, kind: Class, item: usize, end: usize) -> Option<usize> {
    let following = if item + 1 < end { token_key(&toks[item + 1]) } else { String::new() };
    let mut last = None;
    for i in (floor..cue).rev() {
        if classify(&toks[i]) != kind {
            continue;
        }
        if last.is_none() {
            last = Some(i);
        }
        if !following.is_empty() && i + 1 < cue && token_key(&toks[i + 1]) == following {
            return Some(i);
        }
    }
    last
}

fn find_retrace_anchor(toks: &[String], floor: usize, cue: usize, start: usize, count: usize) -> Option<usize> {
    if count < 2 {
        return None;
    }
    let first = token_key(&toks[start]);
    let second = token_key(&toks[start + 1]);
    if first.is_empty() || second.is_empty() {
        return None;
    }
    (floor..cue.saturating_sub(1)).rev().find(|&i| token_key(&toks[i]) == first && token_key(&toks[i + 1]) == second)
}

/// "at 4:30, no sorry, five thirty": the second time arrives as "530" because nothing next to it
/// said it was a time. It takes the form of the time it replaces.
fn match_clock_form(anchor: &str, item: &str, after_at: bool) -> String {
    let (lead, core, trail) = split(item);
    let hour_like = split(anchor).1.contains(':') || (after_at && split(anchor).1.parse::<i64>().is_ok_and(|h| (1..=12).contains(&h)));
    if !hour_like || !(3..=4).contains(&core.len()) || !core.chars().all(|c| c.is_ascii_digit()) {
        return item.to_string();
    }
    let (hour, minutes) = core.split_at(core.len() - 2);
    let h: i64 = hour.parse().unwrap_or(0);
    let m: i64 = minutes.parse().unwrap_or(99);
    if (1..=12).contains(&h) && m <= 59 { format!("{lead}{hour}:{minutes}{trail}") } else { item.to_string() }
}

/// A day, a month or a number: never a name.
pub(super) fn is_date_word(token: &str) -> bool {
    classify(token) != Class::None
}

fn classify(token: &str) -> Class {
    let key = token_key(token);
    let Some(first) = key.chars().next() else { return Class::None };
    if DAYS.contains(&key.as_str()) {
        return Class::Day;
    }
    // "may" is a verb far more often than a month; it never starts a correction on its own.
    if key != "may" && is_month(&key) {
        return Class::Month;
    }
    if first.is_ascii_digit() || is_number_word(&key) || key == "noon" || key == "midnight" {
        return Class::Number;
    }
    Class::None
}

/// The "." / "?" / "!" that ends a sentence; "" for "p.m." and for commas.
fn sentence_closing(token: &str) -> &str {
    if !ends_sentence(token) {
        return "";
    }
    let trail = trailing_punctuation(token);
    trail.find(['.', '!', '?']).map_or("", |cut| &trail[cut..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_correction_replaces_what_it_lines_up_with() {
        let cases = [
            (
                "Let's meet on Tuesday 2 pm. No, actually Wednesday 3 p.m.",
                "Let's meet on Wednesday 3 p.m.",
            ),
            (
                "something like Tuesday, 2 p.m., no— actually Wednesday, 3 p.m.",
                "something like Wednesday, 3 p.m.",
            ),
            ("send it to John, no, actually send it to Mark", "send it to Mark"),
            (
                "on Tuesday at 2 pm with John, no actually Wednesday at 3 pm",
                "on Wednesday at 3 pm with John",
            ),
            ("it costs 50 dollars, no actually 60", "it costs 60 dollars"),
            ("It is at 4:30, no sorry, 530.", "It is at 5:30."),
            ("We need 2, no, 3 chairs.", "We need 3 chairs."),
            ("we ship in March, sorry, I meant April", "we ship in April"),
            ("it was Tuesday, actually Wednesday", "it was Wednesday"),
            ("I have 2 kids, I mean 3", "I have 3 kids"),
        ];
        for (raw, expected) in cases {
            assert_eq!(apply(raw), expected, "{raw}");
        }
    }

    #[test]
    fn a_cue_with_nothing_to_line_up_stays_as_spoken() {
        for raw in [
            "Is it done? No, actually it is not.",
            "No, actually it was fine.",
            "It was Tuesday. Actually Wednesday was better.",
            "Version 2 is actually 3 times faster.",
            "It was good, actually it was great.",
            "I said no, sorry about that.",
        ] {
            assert_eq!(apply(raw), raw);
        }
    }

    #[test]
    fn make_that_and_chained_cues_correct_too() {
        for (raw, expected) in [
            (
                "Can you send the draft by Friday... actually, wait, let's make that next Monday.",
                "Can you send the draft by next Monday.",
            ),
            ("Meet at 4, make that 5.", "Meet at 5."),
            ("It ships Friday, make that this Sunday", "It ships this Sunday"),
            ("Book 2 rooms. Let's make that 3 rooms.", "Book 3 rooms."),
        ] {
            assert_eq!(apply(raw), expected, "{raw}");
        }
        // Nothing lines up: ordinary uses stay.
        for raw in ["Let's make that happen.", "We should make that call on Friday.", "Can you make that 3D model?"] {
            assert_eq!(apply(raw), raw);
        }
    }

    #[test]
    fn corrections_as_people_really_say_them() {
        for (raw, expected) in [
            (
                "Hey, can you send me the draft copy of the documentation by Thursday. No, actually wait, make it Monday.",
                "Hey, can you send me the draft copy of the documentation by Monday.",
            ),
            (
                "Hey, can you send me the mail by when this day? No, wait, by Thursday. Yeah.",
                "Hey, can you send me the mail by Thursday.",
            ),
            ("Hey, I want the mail by Friday. No, Saturday.", "Hey, I want the mail by Saturday."),
        ] {
            assert_eq!(apply(raw), expected, "{raw}");
        }
        for raw in ["It was Friday. No, Saturday was the day we left.", "Is it Tuesday? No, Wednesday.", "We met at home? No, wait, at the office."] {
            assert_eq!(apply(raw), raw, "nothing lines up, or it answers a question");
        }
    }

    #[test]
    fn a_cue_that_opens_a_sentence_corrects_the_one_before() {
        for (raw, expected) in [
            ("Schedule the meeting at 10 AM. No, 11:30 AM.", "Schedule the meeting at 11:30 AM."),
            ("Let's use Python. Actually, let's use C++.", "Let's use C++."),
            ("Send it to Rahul. Actually, send it to Priya.", "Send it to Priya."),
            ("I want the blue one. No, the black one.", "I want the black one."),
            ("Set the temperature to 25 degrees. Wait, 22 degrees.", "Set the temperature to 22 degrees."),
            ("I need a monthly subscription. Actually, an annual subscription.", "I need an annual subscription."),
            ("Send the email today. No, tomorrow morning.", "Send the email tomorrow morning."),
            ("Remind me tomorrow morning. No, on Monday.", "Remind me on Monday."),
            ("Let's launch it in January. Actually, February would be better.", "Let's launch it in February."),
            ("I want to travel to Mumbai. No, Bengaluru.", "I want to travel to Bengaluru."),
            ("Send the file as a PDF. Wait, send it as a Word document.", "Send the file as a Word document."),
            ("I need the meeting recording. No, just the meeting transcript.", "I need the meeting transcript."),
            ("Call me at 6. No, 630.", "Call me at 6:30."),
            ("We need three developers. Actually, two developers and one tester.", "We need two developers and one tester."),
            ("Send data every 5 seconds, no every 10 seconds.", "Send data every 10 seconds."),
            ("Save the file as final version, no, final version two.", "Save the file as final version two."),
            (
                "Send a mail on Thursday. For Shashank? No, Dhanush? No, wait, actually it is Shashank.",
                "Send a mail on Thursday for Shashank.",
            ),
            ("Give it to Rahul. No, it's Priya.", "Give it to Priya."),
        ] {
            assert_eq!(apply(raw), expected, "{raw}");
        }
        for raw in [
            "I went to the office. Actually, the office was closed.",
            "We bought a red car. Actually, we bought a blue one too.",
            "It was Friday. No, Saturday was the day we left.",
            "Send the email to everyone. No, only send it to the management team.",
            "Sam called. No, nobody else did.",
            "Send a mail on Thursday. Actually, it is Chashank.",
            "Call Rahul. Actually, it is Sunday.",
        ] {
            assert_eq!(apply(raw), raw, "a new statement, not a correction");
        }
    }

    #[test]
    fn short_inputs_do_not_panic() {
        for raw in ["", "no", "no actually", "actually no 3"] {
            let _ = apply(raw);
        }
    }
}
