//! Spoken fillers. Port of `FillerStripper.cs`. Run after revision rewriting.

use super::text::{collapse_whitespace, token_key, tokens, trailing_punctuation};
use super::CleanupMode;

const TOKEN_FILLERS: [&[&str]; 10] = [&["um"], &["uh"], &["er"], &["ah"], &["hmm"], &["hm"], &["uhh"], &["umm"], &["erm"], &["huh"]];

/// Dictation drops hesitation sounds anywhere, and the owner's filler words where they carry
/// nothing (V45; "I actually tested" and "basically operates" keep theirs):
/// "like", "so", "please" and "thank you" are words people mean in a message, and "er", "ah",
/// "huh" can be real text. "I mean" and "you know" go only where a clause starts or a comma sets
/// them off ("I'll let you know" stays); "kind of" stays after a word that makes it a noun
/// ("what kind of phone").
const DICTATION_FILLERS: [&[&str]; 22] = [
    &["um"], &["umm"], &["uh"], &["uhh"], &["uhm"], &["erm"], &["hmm"], &["hm"], &["er"], &["ah"], &["eh"],
    &["mm"], &["mmm"],
    &["basically"], &["actually"],
    // Longest first: "the local model I mean the one that runs" → "the local model that runs".
    &["i", "mean", "the", "one"], &["i", "mean"], &["you", "know"], &["kind", "of"], &["sort", "of"],
    &["pretty", "much"], &["like"],
];
/// "the device like very small": "like" before these is a filler, unless a subject likes something.
const INTENSIFIERS: [&str; 6] = ["very", "really", "super", "totally", "literally", "extremely"];
const LIKE_SUBJECTS: [&str; 16] = [
    "i", "you", "we", "they", "he", "she", "people", "would", "do", "does", "did", "don't", "doesn't", "didn't",
    "not", "really",
];
/// "The problem is you know the sensor…": "you know" after these is a filler without commas;
/// "Do you know the answer?" and "let you know" keep it.
const COPULAS: [&str; 4] = ["is", "was", "are", "were"];
const PRONOUN_SUBJECTS: [&str; 7] = ["i", "we", "you", "they", "he", "she", "it"];

/// Where the sentence holding token `i` starts.
fn sentence_start(toks: &[String], i: usize) -> usize {
    (0..i).rev().find(|&k| super::text::ends_sentence(&toks[k])).map_or(0, |k| k + 1)
}
/// Before "kind of" these make it "a sort of", not a hedge.
const KIND_OF_NOUN: [&str; 24] = [
    "what", "this", "that", "a", "the", "any", "some", "every", "one", "same", "different", "which", "each", "no",
    "another", "these", "those", "all", "its", "his", "her", "their", "our", "my",
];

const PHRASE_FILLERS: [&str; 6] = ["you know", "i guess", "sort of", "kind of", "i don't know", "i do not know"];

const LEADING: [&str; 13] = [
    "hey oasis", "ok oasis", "okay oasis", "please", "okay so", "ok so", "well", "okay", "ok", "so", "yeah",
    "yes so", "like",
];

const TRAILING: [&str; 3] = ["please", "thanks", "thank you"];

/// Strip fillers for `mode`.
pub fn apply(text: &str, mode: CleanupMode) -> String {
    let current = collapse_whitespace(text);
    if current.is_empty() {
        return String::new();
    }
    if mode == CleanupMode::Dictate {
        return strip_closer(&strip_set_off(&strip_openers(&strip_leading_ack(&strip_tokens(&current, &DICTATION_FILLERS)))));
    }
    let mut current = current;
    for phrase in PHRASE_FILLERS {
        current = remove_word_phrase(&current, phrase);
    }
    current = strip_tokens(&current, &TOKEN_FILLERS);
    current = strip_leading(&current);
    current = strip_trailing(&current);
    collapse_whitespace(&current)
}

/// Code editors and terminals: only the sounds nobody types on purpose. "Actually", "I mean" and
/// "er"/"ah" can be meant in code, so they stay.
const CODE_FILLERS: [&[&str]; 10] = [&["um"], &["umm"], &["uh"], &["uhh"], &["uhm"], &["erm"], &["hmm"], &["hm"], &["mm"], &["mmm"]];

pub fn hesitations(text: &str) -> String {
    strip_closer(&strip_tokens(&collapse_whitespace(text), &CODE_FILLERS))
}

/// Fillers people set off with commas in the middle of a sentence: ", you know,", ", like,".
const SET_OFF_FILLERS: [&[&str]; 3] = [&["you", "know"], &["like"], &["basically"]];

/// Openings that carry nothing at the start of a dictation: "So, yeah, basically, …", "Well,
/// actually, …", "Okay, so, …", "I mean, …". "So", "basically" and "actually" go even without a
/// comma ("So the main issue …").
/// "Well" is handled in `empty_frame_end`: "Well, this approach might work" keeps it.
const OPENERS: [&[&str]; 8] = [&["so"], &["basically"], &["actually"], &["okay"], &["ok"], &["yeah"], &["like"], &["i", "mean"]];
const OPENERS_WITHOUT_COMMA: [&str; 3] = ["so", "basically", "actually"];
/// After "so" with no comma these show it starts the sentence's content: "so the …", "so what …";
/// "so please …" and "so much" are meant.
const SO_BEFORE: [&str; 14] =
    ["the", "what", "we", "i", "basically", "this", "that", "these", "my", "our", "yeah", "okay", "ok", "well"];
/// Frames that open a dictation and say nothing: "Let me think we have…", "The thing is the
/// battery…", "What I am trying to say is that we…". Dropped only when a clause follows, so "Let
/// me think about it" and "The thing is broken" stay.
const FRAMES: [&[&str]; 11] = [
    &["what", "i", "am", "trying", "to", "say", "is"], &["what", "i'm", "trying", "to", "say", "is"],
    &["what", "i", "want", "to", "say", "is"], &["what", "i", "mean", "is"], &["you", "know", "what", "i", "mean"],
    &["what", "i", "mean"], &["the", "thing", "is"], &["let", "me", "think"], &["let", "me", "see"], &["you", "see"],
    &["you", "know"],
];
/// Subjects that show a clause starts; "it" and "you" are left out ("let me think it through").
const SUBJECTS: [&str; 15] = [
    "i", "we", "they", "he", "she", "there", "i'm", "we're", "they're", "it's", "i've", "we've", "i'll", "we'll",
    "that's",
];
const OPENER_WORDS: [&str; 7] = ["so", "yeah", "okay", "ok", "well", "basically", "actually"];

/// A clause starts at `at`: "we have…", "the battery is…".
fn starts_clause(toks: &[String], at: usize) -> bool {
    let key = |k: usize| toks.get(at + k).map(|t| token_key(t)).unwrap_or_default();
    SUBJECTS.contains(&key(0).as_str())
        || (["the", "this", "our", "my", "your"].contains(&key(0).as_str())
            && ["is", "was", "are", "were"].contains(&key(2).as_str()))
}

/// The frame or bare opener at `at` that carries nothing: where the content starts after it.
fn empty_frame_end(toks: &[String], at: usize) -> Option<usize> {
    for frame in FRAMES {
        let end = at + frame.len();
        let said = end < toks.len()
            && frame.iter().enumerate().all(|(k, w)| token_key(&toks[at + k]) == *w)
            && toks[at..end - 1].iter().all(|t| trailing_punctuation(t).is_empty());
        if !said {
            continue;
        }
        // "…is that we need" → "We need".
        let end = if frame.last() == Some(&"is") && token_key(&toks[end]) == "that" { end + 1 } else { end };
        if end < toks.len() && (trailing_punctuation(&toks[end - 1]) == "," || starts_clause(toks, end)) {
            return Some(end);
        }
    }
    // "Okay so …", "So yeah I think …": an acknowledgement before another opener, or after one.
    let key = token_key(toks.get(at)?);
    let next = toks.get(at + 1).map(|t| token_key(t)).unwrap_or_default();
    let bare = trailing_punctuation(&toks[at]).is_empty();
    // "Well I don't think…", "Well, actually, I…" go; "Well, this approach might work" stays.
    if key == "well" && at + 1 < toks.len() && (OPENER_WORDS.contains(&next.as_str()) || starts_clause(toks, at + 1)) {
        return Some(at + 1);
    }
    let ack = ["okay", "ok", "yeah"].contains(&key.as_str());
    (ack && bare && at + 1 < toks.len() && (OPENER_WORDS.contains(&next.as_str()) || (at > 0 && starts_clause(toks, at + 1))))
        .then_some(at + 1)
}
/// A filler's comma before one of these marks a new clause, and stays: "Hi John, um, how are you".
const CLAUSE_OPENERS: [&str; 16] = [
    "i", "we", "you", "he", "she", "they", "it", "how", "what", "why", "when", "where", "who", "which", "and", "but",
];

/// Drop filler words without rebuilding the rest, so commas and question marks survive. A filler
/// that ended a sentence hands its terminator to the word before it. One set off by commas
/// ("wanted to, uh, ask") takes the comma before it along, unless a new clause follows. One that
/// opened a sentence hands its capital to the next word.
fn strip_tokens(text: &str, fillers: &[&[&str]]) -> String {
    let mut kept: Vec<String> = Vec::new();
    let mut changed = false;
    let mut capital = false;
    let toks = tokens(text);
    let mut i = 0;
    while i < toks.len() {
        let Some(f) = fillers.iter().find(|f| is_filler_at(&toks, i, f)) else {
            let mut token = toks[i].clone();
            if std::mem::take(&mut capital) {
                token = capitalize(&token);
            }
            kept.push(token);
            i += 1;
            continue;
        };
        changed = true;
        let last = &toks[i + f.len() - 1];
        let closing = trailing_punctuation(last);
        let opens = i == 0 || super::text::ends_sentence(&toks[i - 1]);
        capital = opens && toks[i].starts_with(char::is_uppercase);
        i += f.len();
        if closing == "," {
            drop_comma_before(&mut kept, toks.get(i).map(String::as_str));
        }
        if closing.contains(['.', '!', '?']) {
            if let Some(prev) = kept.last_mut() {
                // "brilliant, actually." → "brilliant."
                if prev.ends_with(',') {
                    prev.pop();
                }
                if trailing_punctuation(prev).is_empty() {
                    prev.push_str(closing);
                }
            }
            capital = true;
        }
    }
    if changed { kept.join(" ") } else { text.to_string() }
}

/// `filler` is said at `i`, and here it carries nothing.
fn is_filler_at(toks: &[String], i: usize, filler: &[&str]) -> bool {
    let n = filler.len();
    let said = i + n <= toks.len()
        && filler.iter().enumerate().all(|(k, w)| {
            let key = token_key(&toks[i + k]);
            key == *w || (n == 1 && HESITATIONS.contains(w) && squeeze(&key) == squeeze(w))
        })
        // "I, mean" is not the phrase; only its last word may carry punctuation.
        && toks[i..i + n - 1].iter().all(|t| trailing_punctuation(t).is_empty());
    if !said {
        return false;
    }
    let prev = (i > 0).then(|| token_key(&toks[i - 1]));
    let prev_in = |set: &[&str]| prev.as_ref().is_some_and(|p| set.contains(&p.as_str()));
    let next = toks.get(i + n).map(|t| token_key(t)).unwrap_or_default();
    let after_pause = i == 0 || !trailing_punctuation(&toks[i - 1]).is_empty();
    match filler {
        // Only once the sentence has its subject: "We need to make the device like very small";
        // "Dogs like very small spaces" keeps it.
        ["like"] => {
            prev.is_some()
                && !prev_in(&LIKE_SUBJECTS)
                && INTENSIFIERS.contains(&next.as_str())
                && toks[sentence_start(toks, i)..i].iter().any(|t| PRONOUN_SUBJECTS.contains(&token_key(t).as_str()))
        }
        // Where they add nothing: opening, set off, closing, or after "is" ("It is basically sort
        // of working"). Before a verb they carry meaning: "I actually tested", "basically operates".
        // A new clause after it makes it a restart marker: "Meet at 5 actually let's do lunch".
        ["actually" | "basically"] => {
            after_pause
                || prev_in(&COPULAS)
                || !trailing_punctuation(&toks[i]).is_empty()
                || PRONOUN_SUBJECTS.contains(&next.as_str())
                || next == "let's"
        }
        // "5 mm" and "five mm" are millimetres.
        ["mm" | "mmm"] => !prev.as_ref().is_some_and(|p| p.ends_with(|c: char| c.is_ascii_digit()) || super::numbers::is_number_word(p)),
        [_] => true,
        // "that's pretty much what we need"; "pretty much the same" means "almost".
        ["pretty", "much"] => ["what", "it", "how"].contains(&next.as_str()),
        ["kind" | "sort", "of"] => !prev_in(&KIND_OF_NOUN),
        ["you", "know"] => after_pause || prev_in(&COPULAS),
        ["i", "mean", "the", "one"] => !after_pause && ["that", "which", "who"].contains(&next.as_str()),
        // "I mean it", "I mean that", "I mean to call" are meant.
        ["i", "mean"] => {
            (after_pause && !["it", "that", "to", "what", "well", "business"].contains(&next.as_str()))
                || ["the", "a", "an", "not"].contains(&next.as_str())
        }
        _ => after_pause,
    }
}

/// Sounds the speech model stretches ("ummm", "uhhh", "ahh"): matched whatever their length.
const HESITATIONS: [&str; 7] = ["um", "uh", "hm", "er", "ah", "eh", "erm"];

/// "ummm" → "um": runs of one letter shrink to a single letter.
fn squeeze(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if !out.ends_with(c) {
            out.push(c);
        }
    }
    out
}

fn capitalize(token: &str) -> String {
    let mut chars = token.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// The comma on the last kept word goes, unless what follows opens a new clause.
fn drop_comma_before(kept: &mut [String], next: Option<&str>) {
    if next.is_some_and(|n| CLAUSE_OPENERS.contains(&token_key(n).as_str())) {
        return;
    }
    if let Some(last) = kept.last_mut().filter(|l| l.ends_with(',')) {
        last.pop();
    }
}

/// ", you know,", ", like," in a sentence: the filler and its commas go.
fn strip_set_off(text: &str) -> String {
    let toks = tokens(text);
    let mut kept: Vec<String> = Vec::new();
    let mut i = 0;
    let mut changed = false;
    while i < toks.len() {
        let hit = SET_OFF_FILLERS.iter().find(|f| {
            i > 0
                && toks[i - 1].ends_with(',')
                && i + f.len() < toks.len()
                && f.iter().enumerate().all(|(k, w)| token_key(&toks[i + k]) == *w)
                && trailing_punctuation(&toks[i + f.len() - 1]) == ","
        });
        if let Some(f) = hit {
            if let Some(last) = kept.last_mut() {
                last.pop();
            }
            i += f.len();
            changed = true;
            continue;
        }
        kept.push(toks[i].clone());
        i += 1;
    }
    if changed { kept.join(" ") } else { text.to_string() }
}

/// Last words that are only "Okay.", "Yeah.", "Okay, yeah.", "Right." or "Mm-hmm." after the real
/// text: the speaker signing off as the button is let go, not part of the text (V40). Said alone,
/// or in the middle, they stay.
const CLOSERS: [&str; 14] = ["okay", "ok", "yeah", "right", "alright", "so", "yep", "mmhmm", "mhm", "uhhuh", "mm", "mmm", "hmm", "mhmm"];

fn strip_closer(text: &str) -> String {
    let toks = tokens(text);
    let mut end = toks.len();
    while end > 0 && CLOSERS.contains(&token_key(&toks[end - 1]).as_str()) {
        end -= 1;
    }
    let closer = end < toks.len() && end > 0 && super::text::ends_sentence(&toks[end - 1]);
    if closer { toks[..end].join(" ") } else { text.to_string() }
}

/// "Mm-hmm, I think…", "Okay. The plan…": a leading acknowledgement that is its own sentence
/// (or "mm-hmm" with a comma) goes when more text follows. "Okay the plan" / "Okay, so" are
/// left to the opener rules; said alone it stays.
fn strip_leading_ack(text: &str) -> String {
    let toks = tokens(text);
    let mut at = 0;
    while at + 1 < toks.len() {
        let (key, trail) = (token_key(&toks[at]), trailing_punctuation(&toks[at]));
        let sound = ["mmhmm", "mhmm", "mhm", "uhhuh"].contains(&key.as_str());
        let word = ["okay", "ok"].contains(&key.as_str());
        if (sound && (trail == "," || trail == ".")) || (word && trail == ".") {
            at += 1;
        } else {
            break;
        }
    }
    if at == 0 {
        return text.to_string();
    }
    let mut rest = toks[at..].to_vec();
    rest[0] = capitalize(&rest[0]);
    rest.join(" ")
}

/// Openings with nothing in them, at the start of the dictation only: later, "So, we restarted"
/// says why.
pub(super) fn strip_openers(text: &str) -> String {
    let toks = tokens(text);
    let mut at = 0;
    'again: loop {
        for opener in OPENERS {
            let end = at + opener.len();
            if end >= toks.len() || !opener.iter().enumerate().all(|(k, w)| token_key(&toks[at + k]) == *w) {
                continue;
            }
            let last = &toks[end - 1];
            let comma = trailing_punctuation(last) == ",";
            let next = token_key(&toks[end]);
            let bare = trailing_punctuation(last).is_empty()
                && OPENERS_WITHOUT_COMMA.contains(&opener[0])
                && (opener[0] != "so" || SO_BEFORE.contains(&next.as_str()));
            if comma || bare {
                at = end;
                continue 'again;
            }
        }
        if let Some(end) = empty_frame_end(&toks, at) {
            at = end;
            continue 'again;
        }
        break;
    }
    // "What we can do is first update…" → "We can first update…".
    let key = |k: usize| toks.get(at + k).map(|t| token_key(t)).unwrap_or_default();
    let what_do = key(0) == "what"
        && ["we", "i", "you", "they"].contains(&key(1).as_str())
        && ["can", "should", "could"].contains(&key(2).as_str())
        && key(3) == "do"
        && key(4) == "is"
        && at + 5 < toks.len()
        && toks[at..at + 5].iter().all(|t| trailing_punctuation(t).is_empty());
    if at == 0 && !what_do {
        return text.to_string();
    }
    let mut rest = if what_do {
        [toks[at + 1].to_lowercase(), toks[at + 2].to_lowercase()].into_iter().chain(toks[at + 5..].iter().cloned()).collect()
    } else {
        toks[at..].to_vec()
    };
    if rest[0] == "i" {
        rest[0] = "I".into();
    }
    // The new first word takes the capital the sentence had.
    if toks[0].chars().next().is_some_and(char::is_uppercase) {
        rest[0] = capitalize(&rest[0]);
    }
    rest.join(" ")
}

fn strip_leading(text: &str) -> String {
    let mut current = text.to_string();
    'again: loop {
        for filler in LEADING {
            if current.eq_ignore_ascii_case(filler) {
                return String::new();
            }
            if let Some(rest) = strip_prefix_ci(&current, &format!("{filler} ")) {
                current = rest.trim_start().to_string();
                continue 'again;
            }
        }
        return current;
    }
}

fn strip_trailing(text: &str) -> String {
    let mut current = text.to_string();
    'again: loop {
        for filler in TRAILING {
            if current.eq_ignore_ascii_case(filler) {
                return String::new();
            }
            let suffix = format!(" {filler}");
            let cut = current.len().saturating_sub(suffix.len());
            if current.len() >= suffix.len()
                && current.is_char_boundary(cut)
                && current[cut..].eq_ignore_ascii_case(&suffix)
            {
                current.truncate(cut);
                current = current.trim_end().to_string();
                continue 'again;
            }
        }
        return current;
    }
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let ok = text.len() >= prefix.len()
        && text.is_char_boundary(prefix.len())
        && text[..prefix.len()].eq_ignore_ascii_case(prefix);
    ok.then(|| &text[prefix.len()..])
}

/// Remove every whole-word occurrence of `phrase` (an ASCII phrase; any case in the text).
fn remove_word_phrase(text: &str, phrase: &str) -> String {
    let mut text = text.to_string();
    let mut lower = text.to_lowercase();
    // Lower-casing can change byte lengths for exotic letters; then the offsets would not line up,
    // so the text is left alone rather than cut in the wrong place.
    if lower.len() != text.len() {
        return text;
    }
    let mut idx = 0;
    while idx + phrase.len() <= lower.len() {
        let Some(rel) = lower[idx..].find(phrase) else { break };
        let found = idx + rel;
        let after = found + phrase.len();
        let before_ok = lower[..found].chars().next_back().is_none_or(|c| !c.is_alphanumeric());
        let after_ok = lower[after..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            text.replace_range(found..after, "");
            lower = text.to_lowercase();
            if lower.len() != text.len() {
                return text;
            }
            idx = found;
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
    use CleanupMode::{Command, Dictate};

    #[test]
    fn sign_off_words_and_mm_sounds_go_but_millimetres_stay() {
        assert_eq!(apply("It is not doing the action. Mm.", Dictate), "It is not doing the action.");
        assert_eq!(apply("It is not doing the action. Okay, yeah.", Dictate), "It is not doing the action.");
        assert_eq!(apply("That was the last one. Mm-hmm. Okay.", Dictate), "That was the last one.");
        assert_eq!(apply("Mm Hey, this is still an issue.", Dictate), "Hey, this is still an issue.");
        assert_eq!(apply("The hole is 5 mm wide.", Dictate), "The hole is 5 mm wide.");
        assert_eq!(apply("Okay.", Dictate), "Okay.", "said alone it is the text");
        assert_eq!(hesitations("it is typing it and not doing the action. Mm. Okay yeah."), "it is typing it and not doing the action.");
        assert_eq!(hesitations("make it five mm"), "make it five mm");
    }

    #[test]
    fn command_fillers_go() {
        assert_eq!(apply("um I think we should um go", Command), "I think we should go");
        assert_eq!(apply("please wait for me", Command), "wait for me");
        assert_eq!(apply("open kicad please", Command), "open kicad");
        assert_eq!(apply("hey oasis ok so you know open notepad", Command), "open notepad");
        assert_eq!(apply("please", Command), "");
    }

    #[test]
    fn oasis_alone_is_not_a_filler() {
        assert_eq!(apply("Oasis open notepad", Command), "Oasis open notepad");
    }

    #[test]
    fn dictation_drops_fillers_and_keeps_punctuation() {
        assert_eq!(apply("Um, I think so, kind of. Uh", Dictate), "I think so.");
        assert_eq!(apply("so please thank you", Dictate), "so please thank you");
    }

    #[test]
    fn the_owners_fillers_go_anywhere_in_the_text() {
        for (raw, expected) in [
            ("The application is actually, actually working fine now.", "The application is working fine now."),
            ("The application is quite brilliant, actually.", "The application is quite brilliant."),
            ("It works. Basically the board is fine.", "It works. The board is fine."),
            ("I think, you know, maybe we should reconsider it.", "I think maybe we should reconsider it."),
            ("It was fine. I mean we could still test it.", "It was fine. We could still test it."),
            ("It is kind of slow today.", "It is slow today."),
            // Meant, not filler.
            ("What kind of phone is that?", "What kind of phone is that?"),
            ("It is a kind of sensor.", "It is a kind of sensor."),
            ("I'll let you know tomorrow.", "I'll let you know tomorrow."),
            ("Do you know the answer?", "Do you know the answer?"),
        ] {
            assert_eq!(apply(raw, Dictate), expected, "{raw}");
        }
    }

    #[test]
    fn dictation_drops_empty_openings_and_set_off_fillers() {
        for (raw, expected) in [
            ("So, yeah, basically, we have completed the project.", "We have completed the project."),
            ("Okay, so, yeah, let's proceed with this.", "Let's proceed with this."),
            ("Well, actually, I wanted to discuss the project.", "I wanted to discuss the project."),
            ("So the main issue is with the database.", "The main issue is with the database."),
            ("Like, I was thinking, like, we could use another approach.", "I was thinking we could use another approach."),
            ("I think it is, you know, a little bit complicated.", "I think it is a little bit complicated."),
            ("I wanted to, uh, ask you something.", "I wanted to ask you something."),
            ("Hi John, um, how are you?", "Hi John, how are you?"),
            ("We need ummm to check the uhhh board, ah, again.", "We need to check the board again."),
            ("so please thank you", "so please thank you"),
            ("It failed. So, we restarted.", "It failed. So, we restarted."),
            ("Actually, um, I don't know how to explain this.", "I don't know how to explain this."),
            ("I think that was good. Okay.", "I think that was good."),
            ("Okay.", "Okay."),
            ("Send it to Shashank. Mm-hmm.", "Send it to Shashank."),
            ("Uh-huh.", "Uh-huh."),
            ("Mm-hmm. That works for me.", "That works for me."),
        ] {
            assert_eq!(apply(raw, Dictate), expected, "{raw}");
        }
    }

    #[test]
    fn leading_acknowledgements_go_when_a_sentence_follows() {
        for (raw, expected) in [
            ("Mm-hmm, I think the board is fine.", "I think the board is fine."),
            ("Uh-huh, send it tomorrow.", "Send it tomorrow."),
            ("Mm-hmm. That works for me.", "That works for me."),
            ("Okay. The plan works.", "The plan works."),
            ("Mm, the plan works.", "The plan works."),
            // Alone, or real content: stays.
            ("Okay.", "Okay."),
            ("Mm-hmm.", "Mm-hmm."),
            ("Okay the plan works", "Okay the plan works"),
            ("Okay and then we ship.", "Okay and then we ship."),
        ] {
            assert_eq!(apply(raw, Dictate), expected, "{raw}");
        }
    }

    #[test]
    fn empty_frames_and_hedges_go_meant_words_stay() {
        for (raw, expected) in [
            ("So basically what I am trying to say is that we need to reduce power.", "We need to reduce power."),
            ("Well I don't think this is a hardware problem.", "I don't think this is a hardware problem."),
            ("Let me think uh we have completed the design.", "We have completed the design."),
            ("You see the problem is that the microphone is noisy.", "The problem is that the microphone is noisy."),
            ("So yeah I think that's pretty much it.", "I think that's it."),
            ("The thing is actually the battery is weak.", "The battery is weak."),
            ("You know what I mean we should test this.", "We should test this."),
            ("Ah okay so what we can do is first update it.", "We can first update it."),
            ("The interface is sort of confusing.", "The interface is confusing."),
            ("We need to make the device like very small.", "We need to make the device very small."),
            ("The problem is you know the sensor is fine.", "The problem is the sensor is fine."),
            ("Use the local model I mean the one that runs offline.", "Use the local model that runs offline."),
            // Meant, not filler.
            ("Let me think about it.", "Let me think about it."),
            ("The thing is broken.", "The thing is broken."),
            ("Well water is clean.", "Well water is clean."),
            ("I like very spicy food.", "I like very spicy food."),
            ("What sort of sensor is it?", "What sort of sensor is it?"),
            ("Yeah I agree.", "Yeah I agree."),
            ("I mean that sincerely.", "I mean that sincerely."),
            ("Dogs like very small spaces.", "Dogs like very small spaces."),
            ("The results are pretty much the same.", "The results are pretty much the same."),
            ("I actually tested the firmware yesterday.", "I actually tested the firmware yesterday."),
            ("The device basically operates as a BLE peripheral.", "The device basically operates as a BLE peripheral."),
            ("Well, this approach might work.", "Well, this approach might work."),
            ("Just restart the device.", "Just restart the device."),
            ("It is basically sort of working.", "It is working."),
        ] {
            assert_eq!(apply(raw, Dictate), expected, "{raw}");
        }
    }

    #[test]
    fn phrase_inside_a_word_is_kept() {
        assert_eq!(apply("kind of", Command), "");
        assert_eq!(apply("unkind offer", Command), "unkind offer");
    }
}
