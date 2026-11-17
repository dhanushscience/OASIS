//! Casing and light punctuation. Port of `DictationFormatter.cs`. Command mode stays
//! planner-friendly (no terminal period).

use super::text::{collapse_whitespace, ends_sentence, token_key, trailing_punctuation, words};
use super::{CleanupContext, CleanupMode};

pub fn apply(text: &str, context: &CleanupContext) -> String {
    let mut current = collapse_whitespace(text);
    if current.is_empty() {
        return String::new();
    }
    current = capitalize_standalone_i(&current);
    if !context.apply_formatting || context.mode != CleanupMode::Dictate {
        return current;
    }
    current = capitalize_sentences(&comma_before_but(&current));
    mark_questions(&ensure_terminal_punctuation(&current))
}

/// "It works but we still need…" → "It works, but we still need…": "but" joining two clauses
/// takes a comma. "Not this but that" and "all but the last" have no clause after it.
fn comma_before_but(text: &str) -> String {
    let mut toks = super::text::tokens(text);
    let key = |toks: &[String], k: usize| toks.get(k).map(|t| token_key(t)).unwrap_or_default();
    let mut changed = false;
    let mut since_stop = 0;
    for i in 0..toks.len() {
        let clause_after = ["i", "we", "they", "he", "she", "it", "you", "there"].contains(&key(&toks, i + 1).as_str())
            || (["the", "this", "our", "my", "your", "its"].contains(&key(&toks, i + 1).as_str())
                && ["is", "are", "was", "were"].contains(&key(&toks, i + 3).as_str()));
        if key(&toks, i) == "but" && since_stop >= 3 && clause_after && trailing_punctuation(&toks[i - 1]).is_empty() {
            toks[i - 1].push(',');
            changed = true;
        }
        since_stop = if ends_sentence(&toks[i]) { 0 } else { since_stop + 1 };
    }
    if changed { toks.join(" ") } else { text.to_string() }
}

/// Verbs that open a yes/no question when a subject follows: "Is this ready", "Can you send".
const QUESTION_VERBS: [&str; 34] = [
    "is", "are", "was", "were", "am", "can", "could", "would", "will", "should", "shall", "do", "does", "did",
    "have", "has", "had", "may", "might", "must", "isn't", "aren't", "wasn't", "weren't", "can't", "couldn't",
    "wouldn't", "won't", "shouldn't", "don't", "doesn't", "didn't", "haven't", "hasn't",
];
const SUBJECTS: [&str; 24] = [
    "i", "you", "we", "they", "he", "she", "it", "this", "that", "these", "those", "there", "the", "a", "an", "your",
    "my", "our", "his", "her", "their", "anyone", "someone", "everyone",
];
const QUESTION_WORDS: [&str; 8] = ["what", "why", "how", "when", "where", "who", "which", "whose"];
/// Words that may open a question before its verb: "Hey, can you…", "Okay so, is it…".
const OPENERS: [&str; 10] = ["hey", "hi", "so", "okay", "ok", "well", "and", "but", "also", "oh"];

/// A greeting, a filler opening, or a name ("Sam, can you…": one capitalised word).
fn is_opener(token: &str) -> bool {
    OPENERS.contains(&token_key(token).as_str()) || token.chars().next().is_some_and(char::is_uppercase)
}

/// A sentence that opens like a question ("is this ready", "what time is it", "how about") ends
/// with "?" instead of ".". "Do it now." and "When I got home, I slept." stay statements.
fn mark_questions(text: &str) -> String {
    let mut toks: Vec<String> = text.split(' ').map(str::to_string).collect();
    let mut start = 0;
    for i in 0..toks.len() {
        let last = i + 1 == toks.len();
        if !ends_sentence(&toks[i]) && !last {
            continue;
        }
        // "Hey, can you…", "So, is it…": a greeting or name set off by a comma comes first.
        while start < i && toks[start].ends_with(',') && !toks[start].trim_end_matches(',').contains(',') && token_key(&toks[start]).len() > 1 && is_opener(&toks[start]) {
            start += 1;
        }
        let key = |k: usize| toks.get(k).filter(|_| k <= i).map(|t| token_key(t)).unwrap_or_default();
        let (first, second, third) = (key(start), key(start + 1), key(start + 2));
        let verb = |w: &str| QUESTION_VERBS.contains(&w);
        // "what time is it" is a question; "when I was young" is not.
        // "Do it now", "do this first" are orders, not questions.
        let order = first == "do" && matches!(second.as_str(), "it" | "this" | "that");
        // Cleft "What is happening here is the exact style trap": a second is/was with words after it.
        // shortcut: any later is/was counts, so "What is the part that is broken" stays a statement.
        let cleft = matches!(second.as_str(), "is" | "was")
            && (start + 3..i).any(|k| matches!(key(k).as_str(), "is" | "was"));
        let question = (verb(&first) && SUBJECTS.contains(&second.as_str()) && !order)
            || (QUESTION_WORDS.contains(&first.as_str()) && !cleft
                && (verb(&second) || second == "about" || (!SUBJECTS.contains(&second.as_str()) && verb(&third))));
        if question && i > start && trailing_punctuation(&toks[i]) == "." {
            toks[i].pop();
            toks[i].push('?');
        }
        start = i + 1;
    }
    toks.join(" ")
}

fn capitalize_standalone_i(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = chars.clone();
    for i in 0..chars.len() {
        if !matches!(chars[i], 'i' | 'I') {
            continue;
        }
        let before_ok = i == 0 || !chars[i - 1].is_alphanumeric();
        let after_ok = i + 1 == chars.len() || !chars[i + 1].is_alphanumeric();
        if before_ok && after_ok {
            out[i] = 'I';
        }
    }
    out.into_iter().collect()
}

/// Uppercase the first letter of the text and of every sentence after a terminator.
fn capitalize_sentences(text: &str) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    let mut at_start = true;
    for i in 0..chars.len() {
        let c = chars[i];
        if at_start && c.is_alphabetic() {
            chars[i] = c.to_uppercase().next().unwrap_or(c);
            at_start = false;
            continue;
        }
        if matches!(c, '!' | '?') || (c == '.' && !ends_initial(&chars, i)) {
            at_start = true;
            continue;
        }
        // Whitespace and opening quotes/brackets do not end a pending sentence start.
        if at_start && (c.is_whitespace() || matches!(c, '"' | '\'' | '(' | '[' | '{' | '<')) {
            continue;
        }
        if !c.is_whitespace() {
            at_start = false;
        }
    }
    chars.into_iter().collect()
}

/// A full stop right after a one-letter word is an initial ("I.T.", "U.S.", "e.g."), not a
/// sentence end.
fn ends_initial(chars: &[char], dot: usize) -> bool {
    dot >= 1 && chars[dot - 1].is_alphabetic() && (dot == 1 || !chars[dot - 2].is_alphabetic())
}

fn ensure_terminal_punctuation(text: &str) -> String {
    let Some(last) = text.chars().next_back() else { return String::new() };
    if matches!(last, '.' | '!' | '?' | ':' | ',') || words(text).len() < 3 {
        return text.to_string();
    }
    format!("{text}.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictation_is_capitalized_and_ended() {
        let ctx = CleanupContext::dictate(true);
        assert_eq!(apply("send it today then call me", &ctx), "Send it today then call me.");
        assert_eq!(apply("i think so. it works", &ctx), "I think so. It works.");
        assert_eq!(apply("I.T. is down today", &ctx), "I.T. is down today.");
        assert_eq!(apply("ok", &ctx), "Ok");
    }

    #[test]
    fn questions_end_with_a_question_mark() {
        let ctx = CleanupContext::dictate(true);
        assert_eq!(apply("is this ready for review", &ctx), "Is this ready for review?");
        assert_eq!(apply("Can you send the draft by next Monday.", &ctx), "Can you send the draft by next Monday?");
        assert_eq!(apply("It works. what time is it", &ctx), "It works. What time is it?");
        assert_eq!(apply("how about lunch on Friday", &ctx), "How about lunch on Friday?");
        assert_eq!(apply("Hey, can you send me the mail by Thursday.", &ctx), "Hey, can you send me the mail by Thursday?");
        assert_eq!(apply("Sam, is it done", &ctx), "Sam, is it done?");
        for statement in ["Do it now please.", "Do this first.", "When I got home, I slept.", "When I was young, I ran.", "What a great day it was.", "This is ready for review."] {
            assert_eq!(apply(statement, &ctx), statement);
        }
    }

    #[test]
    fn cleft_sentences_stay_statements() {
        let ctx = CleanupContext::dictate(true);
        assert_eq!(apply("What is happening here is the exact style trap.", &ctx), "What is happening here is the exact style trap.");
        for q in ["What is the baud rate", "What time is it", "Where is the board", "What is happening here"] {
            assert_eq!(apply(q, &ctx), format!("{q}?"));
        }
    }

    #[test]
    fn commands_and_unformatted_text_only_fix_the_pronoun() {
        assert_eq!(apply("open chrome", &CleanupContext::command()), "open chrome");
        assert_eq!(apply("i said i can", &CleanupContext::dictate(false)), "I said I can");
    }
}
