//! The intent gate (`docs/VOICE_DECISIONS.md` V3, V4): is a phrase text to type, or a command?
//!
//! It turns what was said into a plain command phrase the router understands: the address
//! ("Oasis, …") and polite carriers ("can you …", "I want to …") come off, and other ways of saying
//! an action become the router's own word ("bring up" → "open", "quit" → "close"; "my browser" →
//! "browser"). It also says how much evidence the phrase carries, which the engine weighs against
//! where the cursor is:
//!
//! - **A text field has focus:** [`super::intent::judge`] weighs this evidence with what the router
//!   found; only a clear command runs. Anything unsure is typed.
//! - **No text field:** carriers are fine; the router and the target check decide.
//!
//! Whether the target exists (an app installed or running) is the engine's to check: it needs the
//! machine.

use super::classifier::{is_addressed, without_address};

/// What the gate made of a phrase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phrase {
    /// The phrase as a command: lower case, without address or carriers, the action in the
    /// router's words. Empty when nothing is left.
    pub command: String,
    /// It began "Oasis, …" or "hey Oasis …": always meant as a command.
    pub addressed: bool,
    /// It was said politely or to someone ("can you", "please", "I want to", "for me"): how people
    /// write to a person or a chatbot, so in a text field it counts against a command.
    pub carrier: bool,
    /// One sentence ("Open Chrome." not "Open Chrome. Then …").
    pub single_sentence: bool,
    /// Words in `command`.
    pub words: usize,
}

/// Longest phrase with a prose verb ("play", "go to") that still reads as a command with no text
/// field. "set volume to forty percent" is 5.
pub const MAX_WORDS_IN_TEXT_FIELD: usize = 6;

const LEADING_CARRIERS: [&str; 24] = [
    "can you please", "could you please", "would you please", "will you please", "can you", "could you",
    "would you", "will you", "can u", "please", "kindly", "i want to", "i wanna", "i'd like to",
    "i would like to", "i need to", "let's", "lets", "let us", "go ahead and", "just", "now", "and", "then",
];
const TRAILING_CARRIERS: [&str; 5] = ["please", "for me", "thanks", "thank you", "now"];

/// Other ways to say an action, longest first, and the router's word for it. Only at the start.
const ACTION_SYNONYMS: [(&str, &str); 11] = [
    ("bring up", "open"),
    ("pull up", "open"),
    ("fire up", "open"),
    ("start up", "open"),
    ("load up", "open"),
    ("show me", "open"),
    ("close down", "close"),
    ("quit", "close"),
    ("exit", "close"),
    ("kill", "close"),
    ("get me", "open"),
];

/// Ways to name "the default browser", longest first.
const BROWSER_NAMES: [&str; 9] =
    ["my web browser", "the web browser", "web browser", "my browser", "the browser", "a browser", "the internet", "internet", "the web"];

/// Turn what was said into a command phrase, and say how much it looks like one.
pub fn prepare(transcript: &str) -> Phrase {
    let addressed = is_addressed(transcript);
    let spoken = without_address(transcript);
    let single_sentence = is_single_sentence(&spoken);
    let mut text = without_disfluencies(&plain(&spoken));

    let mut carrier = false;
    loop {
        let before = text.len();
        if let Some(rest) = LEADING_CARRIERS.iter().find_map(|c| strip_word_prefix(&text, c)) {
            text = rest;
        }
        if let Some(rest) = TRAILING_CARRIERS.iter().find_map(|c| strip_word_suffix(&text, c)) {
            text = rest;
        }
        if text.len() == before {
            break;
        }
        carrier = true;
    }

    if let Some((said, word)) = ACTION_SYNONYMS.iter().find(|(said, _)| strip_word_prefix(&text, said).is_some()) {
        text = format!("{word} {}", strip_word_prefix(&text, said).unwrap_or_default());
    }
    // "Opencloud" → "open claude": a verb glued to a known app name.
    if let Some((verb, app)) = ["open", "close", "launch"].iter().find_map(|v| {
        let rest = text.strip_prefix(v).filter(|r| !r.is_empty() && !r.contains(' '))?;
        Some((*v, super::apps::canonical_app(rest)?))
    }) {
        text = format!("{verb} {app}");
    }
    // "Close Claude" heard as "Claude could"; it only closes a Claude that is open (router `try_close`).
    if text == "claude could" {
        text = "close claude".into();
    }
    for name in BROWSER_NAMES {
        text = replace_words(&text, name, "browser");
    }
    let command = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let words = command.split_whitespace().count();
    Phrase { command, addressed, carrier, single_sentence, words }
}

/// With no text field found: is this "write/type …" really a sentence to be typed? The router reads
/// "write an email explaining the project" as the typing command "write …" and would drop the words
/// before it; "type hello" is still a command. Long, or said as a request ("I want to write …").
pub fn is_prose_about_writing(phrase: &Phrase) -> bool {
    let writes = phrase.command.starts_with("write ") || phrase.command.starts_with("type ");
    writes && !phrase.addressed && (phrase.words > 4 || (phrase.carrier && phrase.words > 2))
}

/// Verbs that open ordinary sentences too ("go to the store and buy milk", "play it by ear").
const PROSE_VERBS: [&str; 7] = ["go to ", "play ", "start ", "run ", "type ", "paste ", "tap "];

/// With no text field found: is this a command, even one that matched nothing? One sentence that
/// starts with a command word ("open …", "close …") is, at any length, so a misheard app name is
/// reported and not pasted into nothing. The verbs prose also starts with keep the short limit.
pub fn reads_as_command(phrase: &Phrase) -> bool {
    super::classifier::looks_like_command(&phrase.command)
        && phrase.single_sentence
        && (phrase.words <= MAX_WORDS_IN_TEXT_FIELD || !PROSE_VERBS.iter().any(|v| phrase.command.starts_with(v)))
}

/// Sounds people make while thinking, as the speech model writes them ("Search for… ha… laptops").
/// Matched after stretched letters shrink ("ummm" → "um", "hmm" → "hm").
const HESITATIONS: [&str; 14] = ["um", "uh", "uhm", "er", "erm", "ah", "eh", "hm", "m", "mhm", "huh", "ha", "hah", "haha"];

/// A command without hesitation sounds and stutters: "search search for um ha best laptops" →
/// "search for best laptops". `text` is already [`plain`]. Dictation has its own, gentler filler
/// stage; a command has no word worth keeping among these.
fn without_disfluencies(text: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for word in text.split_whitespace() {
        let squeezed: String = word.chars().fold(String::new(), |mut s, c| {
            if !s.ends_with(c) {
                s.push(c);
            }
            s
        });
        if HESITATIONS.contains(&squeezed.as_str()) || kept.last() == Some(&word) {
            continue;
        }
        kept.push(word);
    }
    kept.join(" ")
}

/// Lower case, the punctuation speech-to-text adds turned to spaces; apostrophes kept ("let's").
fn plain(text: &str) -> String {
    let lowered: String = text.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' }).collect();
    lowered.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A sentence ends at `.`, `!` or `?` followed by more words; "3.5" and a closing mark do not count.
fn is_single_sentence(text: &str) -> bool {
    let trimmed = text.trim().trim_end_matches(['.', '!', '?', ' ']);
    let chars: Vec<char> = trimmed.chars().collect();
    !chars.windows(2).any(|w| matches!(w[0], '.' | '!' | '?') && w[1].is_whitespace())
}

fn strip_word_prefix(text: &str, prefix: &str) -> Option<String> {
    let rest = text.strip_prefix(prefix)?;
    (rest.is_empty() || rest.starts_with(' ')).then(|| rest.trim_start().to_string())
}

fn strip_word_suffix(text: &str, suffix: &str) -> Option<String> {
    let rest = text.strip_suffix(suffix)?;
    (rest.is_empty() || rest.ends_with(' ')).then(|| rest.trim_end().to_string())
}

/// Replace `phrase` where it stands as whole words.
fn replace_words(text: &str, phrase: &str, with: &str) -> String {
    let padded = format!(" {text} ");
    padded.replace(&format!(" {phrase} "), &format!(" {with} ")).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(s: &str) -> String {
        prepare(s).command
    }

    #[test]
    fn hesitations_and_stutters_never_reach_a_command() {
        assert_eq!(cmd("Search for. Ha."), "search for", "a thinking sound is not the query");
        assert_eq!(cmd("Search for, um, ha, best laptops."), "search for best laptops");
        assert_eq!(cmd("Ummm, open, uhh, Spotify."), "open spotify");
        assert_eq!(cmd("Open open Chrome."), "open chrome");
        assert_eq!(cmd("Hmm."), "");
        assert_eq!(cmd("Play Mamma Mia."), "play mamma mia", "real words that look alike stay");
        assert_eq!(cmd("Search for bora bora hotels."), "search for bora hotels", "shortcut: a meant repeat is lost too");
    }

    #[test]
    fn a_verb_glued_to_an_app_name_is_split() {
        assert_eq!(cmd("Opencloud."), "open claude");
        assert_eq!(cmd("Claude could."), "close claude");
        assert_eq!(cmd("Launchchrome"), "launch chrome");
        assert_eq!(cmd("Closespotify"), "close spotify");
        assert_eq!(cmd("openly"), "openly", "not an app");
        assert_eq!(cmd("closeup"), "closeup");
    }

    #[test]
    fn humming_sounds_are_dropped_but_commands_survive() {
        assert_eq!(cmd("Mm-hmm Open Chrome"), "open chrome");
        assert_eq!(cmd("Uh-huh, open Chrome"), "open chrome");
        assert_eq!(cmd("Mm Search for a"), "search for a");
        assert_eq!(cmd("Mmm, search for best laptops"), "search for best laptops");
    }

    #[test]
    fn a_sentence_about_writing_is_text_but_a_short_type_command_is_not() {
        assert!(is_prose_about_writing(&prepare("I want to write an email explaining the project.")));
        assert!(is_prose_about_writing(&prepare("Write a long email to my team about the launch.")));
        assert!(!is_prose_about_writing(&prepare("Type hello world.")));
        assert!(!is_prose_about_writing(&prepare("Hey Oasis, write an email explaining the project.")));
        assert!(!is_prose_about_writing(&prepare("Launch Chrome.")));
    }

    #[test]
    fn with_no_text_field_one_sentence_starting_with_a_command_word_is_a_command() {
        assert!(reads_as_command(&prepare("Open VS Code in my primary screen and open Plot in my second screen.")));
        assert!(reads_as_command(&prepare("Can you open via score might primary screen and open clawed in my second screen?")));
        assert!(reads_as_command(&prepare("Play numb.")));
        assert!(!reads_as_command(&prepare("Go to the store and buy some milk on your way home.")), "a prose verb, long");
        assert!(!reads_as_command(&prepare("Open the pull request. Then tell me what the tests say.")), "two sentences");
        assert!(!reads_as_command(&prepare("Hey, this is still an issue.")));
        assert!(!reads_as_command(&prepare("Open parenthesis")));
    }

    #[test]
    fn your_five_ways_to_open_chrome_become_one_command() {
        assert_eq!(cmd("Open Chrome."), "open chrome");
        assert_eq!(cmd("Can you launch Chrome?"), "launch chrome");
        assert_eq!(cmd("I want to go to Chrome."), "go to chrome");
        assert_eq!(cmd("Bring up my browser."), "open browser");
        assert_eq!(cmd("Let's open the internet."), "open browser");
    }

    #[test]
    fn carriers_and_address_are_noticed() {
        let polite = prepare("Could you please close WhatsApp for me?");
        assert_eq!((polite.command.as_str(), polite.carrier), ("close whatsapp", true));
        let bare = prepare("Close WhatsApp.");
        assert_eq!((bare.carrier, bare.addressed, bare.words), (false, false, 2));
        let called = prepare("Oasis, can you open Spotify");
        assert!(called.addressed && called.carrier);
        assert_eq!(called.command, "open spotify");
    }

    #[test]
    fn other_ways_to_say_an_action_use_the_routers_word() {
        assert_eq!(cmd("quit spotify"), "close spotify");
        assert_eq!(cmd("Pull up notepad"), "open notepad");
        assert_eq!(cmd("exit the browser"), "close browser");
        // only at the start: "the exit sign" is not an action
        assert_eq!(cmd("the exit sign is broken"), "the exit sign is broken");
        // "shut down" alone is the power command and is left alone
        assert_eq!(cmd("shut down"), "shut down");
    }

    #[test]
    fn the_evidence_a_phrase_carries_is_noted() {
        // What the evidence decides in a text field is tested in `intent`.
        assert!(!prepare("Open Chrome. Then send the report.").single_sentence, "two sentences");
        assert!(prepare("Open version 3.5").single_sentence, "a decimal point is not a sentence end");
        assert!(prepare("open the door for me please").carrier);
        assert!(prepare("Oasis, can you open Spotify").addressed);
        assert_eq!(cmd("I think you should open Chrome."), "i think you should open chrome");
    }

    #[test]
    fn whole_words_only() {
        assert_eq!(cmd("open internetexplorer"), "open internetexplorer");
        assert_eq!(cmd("open the internet please"), "open browser");
        assert_eq!(cmd("please"), "");
    }
}
