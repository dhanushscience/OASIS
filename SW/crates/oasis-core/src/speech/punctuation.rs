//! Punctuation said out loud: "full stop", "comma", "question mark", "open square bracket …
//! close square bracket", "open quote … close quote". Port of `SpokenPunctuation.cs`.
//!
//! A command may stand alone: people open a bracket in one hold and say "close bracket" in the
//! next, so a lone closer types `)`. The same words are also ordinary nouns ("put a full stop
//! there", "say comma"), so a command right after an article, "say" or "called", or right before
//! "key"/"symbol", is left as words. "period" is a symbol only as its own clause ("tomorrow,
//! period.") because "a trial period" is far more common. Line breaks ("new line") belong to the
//! layout step, which is not ported yet.

use std::sync::OnceLock;

use super::text::{collapse_whitespace, token_key, tokens, trailing_punctuation};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Mark,
    Opener,
    Closer,
}

struct Command {
    words: Vec<&'static str>,
    symbol: &'static str,
    kind: Kind,
}

const QUOTE_CLOSERS: [&[&str]; 4] = [&["close", "quote"], &["close", "quotes"], &["end", "quote"], &["unquote"]];

/// Words that talk about the symbol instead of asking for it.
pub(crate) const MENTION_LEADS: [&str; 9] = ["a", "an", "the", "some", "any", "each", "every", "another", "or"];
const MENTION_LEAD_AND: &str = "and";

/// "say comma" names the symbol. Not for quotes: he said "hello" is what quoting is for.
const SAY_WORDS: [&str; 6] = ["say", "says", "said", "saying", "called", "word"];

const MENTION_TAILS: [&str; 8] = ["key", "keys", "character", "characters", "symbol", "symbols", "sign", "or"];

const SENTENCE_MARKS: [char; 6] = ['.', ',', ';', ':', '!', '?'];

fn is_mention_lead(key: &str) -> bool {
    MENTION_LEADS.contains(&key) || key == MENTION_LEAD_AND
}

fn commands() -> &'static [Command] {
    static COMMANDS: OnceLock<Vec<Command>> = OnceLock::new();
    COMMANDS.get_or_init(build_commands)
}

fn build_commands() -> Vec<Command> {
    let mut list = vec![
        Command { words: vec!["full", "stop"], symbol: ".", kind: Kind::Mark },
        Command { words: vec!["period"], symbol: ".", kind: Kind::Mark },
        Command { words: vec!["comma"], symbol: ",", kind: Kind::Mark },
        Command { words: vec!["question", "mark"], symbol: "?", kind: Kind::Mark },
        Command { words: vec!["exclamation", "mark"], symbol: "!", kind: Kind::Mark },
        Command { words: vec!["exclamation", "point"], symbol: "!", kind: Kind::Mark },
        Command { words: vec!["semicolon"], symbol: ";", kind: Kind::Mark },
        Command { words: vec!["semi", "colon"], symbol: ";", kind: Kind::Mark },
        Command { words: vec!["colon"], symbol: ":", kind: Kind::Mark },
        Command { words: vec!["open", "quote"], symbol: "\"", kind: Kind::Opener },
        Command { words: vec!["open", "quotes"], symbol: "\"", kind: Kind::Opener },
        Command { words: vec!["quote"], symbol: "\"", kind: Kind::Opener },
    ];
    for closer in QUOTE_CLOSERS {
        list.push(Command { words: closer.to_vec(), symbol: "\"", kind: Kind::Closer });
    }
    let brackets: [(&[&'static str], &'static str, &'static str); 21] = [
        (&["bracket"], "(", ")"),
        (&["brackets"], "(", ")"),
        (&["round", "bracket"], "(", ")"),
        (&["round", "brackets"], "(", ")"),
        (&["parenthesis"], "(", ")"),
        (&["parentheses"], "(", ")"),
        (&["paren"], "(", ")"),
        (&["square", "bracket"], "[", "]"),
        (&["square", "brackets"], "[", "]"),
        (&["box", "bracket"], "[", "]"),
        (&["box", "brackets"], "[", "]"),
        (&["flower", "bracket"], "{", "}"),
        (&["flower", "brackets"], "{", "}"),
        (&["curly", "bracket"], "{", "}"),
        (&["curly", "brackets"], "{", "}"),
        (&["curly", "brace"], "{", "}"),
        (&["curly", "braces"], "{", "}"),
        (&["brace"], "{", "}"),
        (&["braces"], "{", "}"),
        (&["angle", "bracket"], "<", ">"),
        (&["angle", "brackets"], "<", ">"),
    ];
    for (shape, open, close) in brackets {
        for verb in ["open", "opening"] {
            let mut words = vec![verb];
            words.extend_from_slice(shape);
            list.push(Command { words, symbol: open, kind: Kind::Opener });
        }
        for verb in ["close", "closed", "closing"] {
            let mut words = vec![verb];
            words.extend_from_slice(shape);
            list.push(Command { words, symbol: close, kind: Kind::Closer });
        }
    }
    // Longest first, so "open square bracket" is not read as "open" + something else. Stable.
    list.sort_by_key(|c| std::cmp::Reverse(c.words.len()));
    list
}

/// Turn spoken punctuation in `text` into symbols.
pub fn apply(text: &str) -> String {
    let normalized = collapse_whitespace(text);
    let toks = tokens(&normalized);
    if toks.is_empty() {
        return normalized;
    }

    let mut output: Vec<String> = Vec::with_capacity(toks.len());
    let mut prefix = String::new();
    let mut lower_next = false;
    let mut changed = false;
    let mut i = 0;
    while i < toks.len() {
        let Some(command) = try_match(&toks, i) else {
            let word = format!("{prefix}{}", toks[i]);
            output.push(if lower_next { lower_sentence_start(&word) } else { word });
            prefix.clear();
            lower_next = false;
            i += 1;
            continue;
        };

        changed = true;
        let len = command.words.len();
        let own = trailing_punctuation(&toks[i + len - 1]).to_string();
        i += len;
        match command.kind {
            Kind::Opener => prefix.push_str(command.symbol),
            Kind::Closer if !prefix.is_empty() => {
                // "open bracket close bracket": an empty pair.
                output.push(format!("{prefix}{}{own}", command.symbol));
                prefix.clear();
            }
            Kind::Closer if output.is_empty() => {
                // The bracket was opened in an earlier hold; this one only closes it.
                output.push(format!("{}{own}", command.symbol));
            }
            Kind::Closer => {
                let last = output.last_mut().expect("not empty");
                *last = format!("{}{}{own}", last.trim_end_matches(','), command.symbol);
            }
            Kind::Mark if output.is_empty() => {
                // "full stop" said on its own, to end what was typed a moment ago.
                output.push(format!("{prefix}{}", command.symbol));
                prefix.clear();
            }
            Kind::Mark => {
                // The model often closes the clause itself ("today. Full stop."); the spoken mark wins.
                let last = output.last_mut().expect("not empty");
                let before_len = last.trim_end_matches(SENTENCE_MARKS).len();
                let replaced = last[before_len..].to_string();
                last.truncate(before_len);
                last.push_str(command.symbol);
                lower_next = matches!(command.symbol, "," | ";" | ":")
                    && (replaced.contains(['.', '!', '?']) || own.contains(['.', '!', '?']));
            }
        }
    }

    if !prefix.is_empty() {
        output.push(prefix);
    }
    if changed { output.join(" ") } else { normalized }
}

fn try_match(toks: &[String], index: usize) -> Option<&'static Command> {
    for command in commands() {
        if !matches_words(toks, index, &command.words) {
            continue;
        }
        let end = index + command.words.len();
        let own_trail = trailing_punctuation(&toks[end - 1]);
        let prev = if index > 0 { toks[index - 1].as_str() } else { "" };
        let prev_trail = trailing_punctuation(prev);
        let prev_key = if prev_trail.is_empty() { token_key(prev) } else { String::new() };
        if is_mention_lead(&prev_key) || (command.symbol != "\"" && SAY_WORDS.contains(&prev_key.as_str())) {
            return None;
        }
        if own_trail.is_empty() && end < toks.len() && MENTION_TAILS.contains(&token_key(&toks[end]).as_str()) {
            return None;
        }
        if command.words == ["period"] && (prev_trail.is_empty() || (end < toks.len() && own_trail.is_empty())) {
            return None;
        }
        if command.words == ["quote"] && !has_quote_closer(toks, end) {
            return None;
        }
        return Some(command);
    }
    None
}

/// True when the text opens with a bracket or quote command ("close parenthesis", "open square
/// bracket …"). Those start with "open" and "close", exactly like "close WhatsApp"; the
/// type-or-act check asks this first, because nothing on the desktop is called a bracket.
pub fn starts_with_bracket_command(text: &str) -> bool {
    let toks = tokens(text);
    commands().iter().any(|c| c.kind != Kind::Mark && c.words.len() > 1 && matches_words(&toks, 0, &c.words))
}

/// Consecutive tokens spell the command; only its last word may carry punctuation.
pub(crate) fn matches_words(toks: &[String], index: usize, words: &[&str]) -> bool {
    if index + words.len() > toks.len() {
        return false;
    }
    for (k, word) in words.iter().enumerate() {
        let token = &toks[index + k];
        if !token.chars().next().is_some_and(char::is_alphabetic) || token_key(token) != *word {
            return false;
        }
        if k < words.len() - 1 && !trailing_punctuation(token).is_empty() {
            return false;
        }
    }
    true
}

fn has_quote_closer(toks: &[String], from: usize) -> bool {
    (from + 1..toks.len()).any(|i| QUOTE_CLOSERS.iter().any(|closer| matches_words(toks, i, closer)))
}

/// "Hello. Comma. World" — the model capitalized "World" because it heard a sentence end that the
/// spoken comma just replaced. Only a plain capitalized word is lowered; "I", acronyms and brand
/// casing keep their capitals.
fn lower_sentence_start(word: &str) -> String {
    let mut chars = word.chars();
    let Some(first) = chars.next() else { return word.to_string() };
    if word.chars().count() < 2 || !first.is_uppercase() || word.starts_with("I'") {
        return word.to_string();
    }
    if chars.clone().any(char::is_uppercase) {
        return word.to_string();
    }
    first.to_lowercase().chain(chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spoken_punctuation_becomes_the_symbol() {
        let cases = [
            ("hello comma how are you question mark", "hello, how are you?"),
            ("Send it today. Full stop.", "Send it today."),
            ("Hello. Comma. World.", "Hello, world."),
            ("add the note open bracket optional close bracket here", "add the note (optional) here"),
            ("use open square bracket draft close square bracket in the title", "use [draft] in the title"),
            ("he said open quote hello close quote", "he said \"hello\""),
            ("he said quote hello unquote and left", "he said \"hello\" and left"),
            ("I am not going, period.", "I am not going."),
            ("wow exclamation mark", "wow!"),
            ("Close parenthesis.", ")."),
            ("close square brackets", "]"),
            ("open square bracket close square bracket", "[]"),
            ("full stop", "."),
            (
                "Open square bracket open flower brackets this is what it might need to type close flower brackets close square brackets.",
                "[{this is what it might need to type}].",
            ),
            ("open box bracket draft close box bracket", "[draft]"),
        ];
        for (raw, expected) in cases {
            assert_eq!(apply(raw), expected, "{raw}");
        }
    }

    #[test]
    fn talking_about_punctuation_is_not_a_command() {
        for raw in [
            "if a user prefers a full stop or some square brackets",
            "put a comma there",
            "you can say comma",
            "press the comma key",
            "during the trial period we shipped",
            "I will quote him on that",
        ] {
            assert_eq!(apply(raw), raw);
        }
    }

    #[test]
    fn bracket_command_detection() {
        assert!(starts_with_bracket_command("close parenthesis"));
        assert!(starts_with_bracket_command("open square bracket x"));
        assert!(!starts_with_bracket_command("close WhatsApp"));
        assert!(!starts_with_bracket_command("full stop"));
    }
}
