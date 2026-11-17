//! Command or typing? Decided from what was said, with the cursor in a text field or not.
//!
//! The router says what a phrase would do; this weighs how sure that is. A phrase is a command when
//! it starts with an action word, what follows names something that is really there (an installed
//! or running app, a known site, a window, the volume, a key), and it is short and plain. It is text
//! when it talks ("I", "we", "you", "because", "later"), runs long, has several sentences, or names
//! nothing. Each piece of evidence adds or takes away log-odds; the sum becomes a probability.
//!
//! With a text field in front an unsure phrase is typed (it lands in the field, so nothing is
//! lost). Without one the engine's own flow already prefers the command; [`Verdict::command`] uses
//! a lower bar there for callers that ask.
//!
//! shortcut: hand-set weights checked against the labelled set in the tests below; refit them on
//! more real recordings, or move to a small classifier, if testers report wrong calls.

use crate::actions::ButtonAction;

use super::classifier::{looks_like_command, without_address};
use super::gate::Phrase;
use super::safety::{evaluate, steps_of, SafetyTier};
use super::text::normalize;

/// What the router found for a phrase, as evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Found {
    /// Nothing matched.
    Nothing,
    /// It matched, but the app it names is not on this PC ("open the door").
    Missing,
    /// It runs, but its target is whatever was said: a search, a song to play, text to type.
    FreeText,
    /// It runs a known thing: an app that is there, a site the catalog knows, a window, a key. The
    /// router's score (1.0 for a fixed phrase).
    Known(f64),
}

/// How sure the phrase is a command, and the call.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    /// 0 to 1.
    pub p_command: f64,
    /// Run it (true) or type it (false).
    pub command: bool,
    /// `p_command` minus the bar it had to clear: how clear the call was.
    pub margin: f64,
}

/// With a text field in front, the bar a command must clear; an unsure phrase is typed.
pub const IN_TEXT_FIELD: f64 = 0.82;
/// With no text field, the bar is low: an unsure phrase is tried as a command.
pub const NO_TEXT_FIELD: f64 = 0.4;

/// Words that make a phrase talk rather than order: people, reasons, time, opinions.
const TALK: [&str; 52] = [
    "i", "i'm", "i'll", "i've", "i'd", "we", "we're", "we'll", "you", "you're", "he", "she", "they", "me", "us", "him",
    "them", "our", "their", "his", "was", "were", "is", "are", "am", "be", "been", "have", "has", "had", "will",
    "would", "should", "could", "because", "but", "if", "when", "that", "which", "who", "why", "what", "how", "not",
    "think", "want", "need", "later", "tomorrow", "yesterday", "maybe",
];

/// Action words the router knows that [`looks_like_command`] does not list.
const MORE_VERBS: [&str; 12] = ["move", "snap", "put", "send", "search", "lock", "set", "turn", "join", "restore", "show", "new"];

/// The evidence for "command", in log-odds, and the call.
/// `edit_commands` is the Voice setting that turns on [`super::edit`].
pub fn judge(transcript: &str, phrase: &Phrase, found: Found, tier: SafetyTier, in_text_field: bool, edit_commands: bool) -> Verdict {
    let command = normalize(&phrase.command);
    let first = command.split(' ').next().unwrap_or("");
    let verb_first = looks_like_command(&command) || MORE_VERBS.contains(&first);
    let spoken = without_address(transcript).to_lowercase();
    let talk = spoken.split(|c: char| !(c.is_alphanumeric() || c == '\'')).filter(|w| TALK.contains(w)).count().min(3);
    let window = super::window_cmd::plan(&command).is_some();

    let mut logit = -1.0;
    logit += if phrase.addressed { 5.0 } else { 0.0 };
    logit += if verb_first { 1.5 } else { -2.5 };
    logit -= if phrase.carrier { 2.0 } else { 0.0 };
    // "close the window": the screen itself is the target, which a sentence to a person rarely is.
    logit += if window { 1.5 } else { 0.0 };
    logit -= 1.5 * talk as f64;
    logit += match phrase.words {
        0..=3 => 1.0,
        4..=6 => 0.0,
        7..=8 => -1.5,
        n => -3.0 - 0.25 * (n - 9) as f64,
    };
    logit -= if phrase.single_sentence { 0.0 } else { 2.0 };
    logit += match found {
        Found::Known(score) => 3.0 * score.clamp(0.0, 1.0),
        Found::FreeText => 1.0,
        Found::Missing => -2.0,
        Found::Nothing => -3.0,
    };
    // Closing or locking asks first anyway; in a text field it also needs a little more evidence.
    logit -= if tier == SafetyTier::Safe { 0.0 } else { 0.5 };

    let mut p_command = 1.0 / (1.0 + (-logit).exp());
    // "copy", "undo", "ctrl z" said on their own, not to someone: that is what they are for, also
    // in a text field. "I want to copy" (a carrier) is scored as before.
    if edit_commands && !phrase.carrier && matches!(found, Found::Known(_)) && super::edit::keys(&command).is_some() {
        p_command = p_command.max(0.99);
    }
    let bar = if in_text_field { IN_TEXT_FIELD } else { NO_TEXT_FIELD };
    Verdict { p_command, command: p_command >= bar, margin: p_command - bar }
}

/// The evidence a routed action gives. `runnable` is the action left once apps that are not on this
/// PC are taken out (`None` if nothing is left); `matched` and `score` are the router's.
pub fn found(matched: bool, score: f64, runnable: Option<&ButtonAction>) -> Found {
    match runnable {
        None if matched => Found::Missing,
        None => Found::Nothing,
        Some(action) if is_free_text(action) => Found::FreeText,
        Some(_) => Found::Known(score),
    }
}

/// The safety tier of a whole plan: its riskiest step.
pub fn tier(action: &ButtonAction) -> SafetyTier {
    let checked = evaluate(&steps_of(action));
    if checked.blocked {
        SafetyTier::Blocked
    } else if checked.requires_confirmation {
        SafetyTier::Caution
    } else {
        SafetyTier::Safe
    }
}

/// "open chatgpt" names no app on this PC but a site the catalog knows: open the site instead.
/// Anything else comes back unchanged.
pub fn site_for_missing_app(action: ButtonAction, exists: &dyn Fn(&str) -> bool) -> ButtonAction {
    let is_launch = action.kind.eq_ignore_ascii_case("LaunchApp") && !action.value.contains('|');
    match crate::matching::web_catalog::try_resolve(&action.value).filter(|_| is_launch && !exists(&action.value)) {
        Some(url) => ButtonAction { kind: "OpenUrl".into(), value: url.into(), steps: vec![] },
        None => action,
    }
}

/// A search, a song or text to type: the target is whatever words were said.
fn is_free_text(action: &ButtonAction) -> bool {
    steps_of(action).iter().any(|(kind, value)| {
        let target = value.split_once('|').map_or(value.as_str(), |(_, url)| url);
        ["PasteText", "SpotifyPlay"].iter().any(|k| kind.eq_ignore_ascii_case(k)) || super::web_search::search_endpoint(target).is_some()
    })
}

#[cfg(test)]
mod tests {
    use super::super::gate::prepare;
    use super::super::lexicon::{build_lexicon, LexiconInputs, RunningApp};
    use super::super::router::{find_running, route, RouterSettings};
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Label {
        Command,
        Text,
        /// Either call is defensible; reported, not scored.
        Unsure,
    }
    use Label::{Command as C, Text as T, Unsure as U};

    /// Apps on the pretend PC, by launcher name or Start Menu name.
    const INSTALLED: [&str; 16] = [
        "chrome", "notepad", "code", "spotify", "calc", "explorer", "claude", "kicad", "cursor", "whatsapp", "msedge", "brave",
        "apple music", "firefox", "browser", "task manager",
    ];

    fn running() -> Vec<RunningApp> {
        [("Notepad", "Untitled - Notepad"), ("WhatsApp", "WhatsApp"), ("chrome", "Inbox - Google Chrome"), ("Spotify", "Spotify Premium"), ("AppleMusic", "Apple Music"), ("Taskmgr", "Task Manager")]
            .iter()
            .map(|(p, t)| RunningApp { process: (*p).into(), title: (*t).into() })
            .collect()
    }

    /// As the engine sees it: route, swap a missing app for a known site, drop apps that are not
    /// there, weigh. `None` is the old rule (before 2026-11-16) for comparison.
    fn decide(said: &str, in_text_field: bool) -> (bool, bool, Verdict) {
        let running = running();
        let exists = |app: &str| app.contains('|') || find_running(app, &running).is_some() || INSTALLED.contains(&app);
        let phrase = prepare(said);
        let lexicon = build_lexicon(&LexiconInputs { active: None, profiles: &[], running: &running });
        let settings = RouterSettings { spotify_running: true, running: running.clone(), ..RouterSettings::default() };
        let routed = route(&phrase.command, &lexicon, &settings);
        let runnable = routed
            .action
            .clone()
            .filter(|_| routed.success)
            .map(|a| site_for_missing_app(a, &exists))
            .filter(|a| steps_of(a).iter().all(|(k, v)| !k.eq_ignore_ascii_case("LaunchApp") || exists(v)));
        let verdict = judge(said, &phrase, found(routed.success, routed.score, runnable.as_ref()), runnable.as_ref().map_or(SafetyTier::Safe, tier), in_text_field, settings.edit_commands);
        // The rule this replaces: a short bare one-sentence phrase (or a polite window command, or an
        // address), an exact router match, and an app that exists. No site fallback.
        let old_runnable = routed.action.clone().filter(|_| routed.success).filter(|a| steps_of(a).iter().all(|(k, v)| !k.eq_ignore_ascii_case("LaunchApp") || exists(v)));
        let polite_window = phrase.single_sentence && phrase.words <= 8 && super::super::window_cmd::plan(&phrase.command).is_some();
        let may = !phrase.command.is_empty() && (phrase.addressed || (!phrase.carrier && phrase.single_sentence && phrase.words <= 6) || polite_window);
        let old = may && old_runnable.is_some() && (phrase.addressed || routed.score >= 0.95);
        (verdict.command, old, verdict)
    }

    /// Real phrases from the owner's recordings (DictationHistory and security.log, 2026-11-16), then
    /// made-up ones. Labels are what the speaker meant with the cursor in a text field.
    const SET: &[(&str, Label)] = &[
        // -- real: run as commands (security.log), or typed by mistake (dictation-trace.log)
        ("Mm-hmm Open notepad.", C),
        ("Mm-hmm Close Task Manager", C),
        ("Close this.", C),
        ("Open Notepad.", C),
        ("Close notepad.", C),
        ("Mm-hmm Open VS Code.", C),
        ("Move notepad to my second screen.", C),
        ("Next.", C),
        ("Close Apple Music", C),
        ("Open Apple music.", C),
        ("Open Apple Music on my second screen.", C),
        ("Search Desert Ant", C),
        ("Open chat GPT", C),
        ("Open chat GPT.", C),
        ("Open cloud code.", C),
        ("Search for Rom Back in the black in the Apple Music.", U),
        ("Open the kind.", U),
        ("Open incognito", U),
        ("Open tangibility.", U),
        ("Play back in the black.", U),
        ("Very fun Close this?", U),
        ("Research the fray open new tab.", U),
        ("Open Breo browser Open Chrome, open Spotify, close Apple Music.", U),
        // -- real: dictation
        ("Hey, can you throw the trash in the dustbin?", T),
        ("So basically we are working on a startup.", T),
        ("Clawed code is clawed by default.", T),
        ("Okay, cloud code is able to direct things.", T),
        ("Hey, I want to test open cloud code and everything is working well.", T),
        ("Open Cloud Code in your screen and go to the next menu.", T),
        ("Next, go to the batch three", T),
        ("Next go to patch 3.", T),
        ("Continue with the work, deploying next five agents, we will start running.", T),
        ("Suggest a better approach and go with it.", T),
        ("Deploy opus for harder tasks, deploy sonnet for smaller task and complete these tasks.", T),
        ("Complete batch 4 with the remaining things, and we will test everything together.", T),
        ("Please use ESP32. Actually NRF fifty-two.", T),
        ("Use ESP dictator. No actually use NRF dictator.", T),
        ("Let us meet on Sunday. No, basically let's meet on Saturday.", T),
        ("Hey, let's meet on Saturday. No, Monday. Actually Sunday.", T),
        ("Colour blue.", T),
        ("Colorado Springs.", T),
        ("Lightning McQueen.", T),
        ("Dust bin", T),
        ("Whisper Flow can hear your whisper.", T),
        ("That's the winner.", T),
        ("Mm mm. I think the board is fine.", T),
        ("Cool.", T),
        ("Code", T),
        ("Subaru.", T),
        ("Hinge back", T),
        ("Hey, does this application have the concept of functional keys? Function f plus f one function plus f two.", T),
        ("It is able to detect even if I whisper very close to my mic.", T),
        ("Hey, why the words is not coming sometimes so?", T),
        ("Who is the best? Is it Project Ories or this after all.", T),
        ("She does still make mistakes but the improvements are rigorously awesome.", T),
        ("Call a Rodu.", T),
        ("Please use", T),
        ("One more issue is even if user put the cursor on notepad and asked to open another application, it would be able to differentiate it as a command.", T),
        ("Deploy opus for harder tasks, deploy sonnet for smaller task and complete these tasks. So that uh today we will be able to ship this.", T),
        ("Claude code", U),
        // -- made up: commands
        ("Open Chrome.", C),
        ("Open Chrome please.", C),
        ("Please open Spotify.", C),
        ("Launch Spotify.", C),
        ("Close WhatsApp.", C),
        ("Maximize chrome.", C),
        ("Minimize this window.", C),
        ("Snap chrome to the left.", C),
        ("Volume up.", C),
        ("Mute.", C),
        ("Set volume to forty percent.", C),
        ("Next track.", C),
        ("Pause.", C),
        ("Open new tab in Chrome.", C),
        ("Open YouTube in Brave.", C),
        ("Press control shift t.", C),
        ("Open Chrome then press control t.", C),
        ("Open bluetooth settings.", C),
        ("Open YouTube.", C),
        ("Open Gmail.", C),
        ("Hey Oasis, open Notepad.", C),
        ("Oasis, can you open Spotify", C),
        ("Open Visual Studio Code.", C),
        ("Launch Calculator.", C),
        ("Open File Explorer.", C),
        ("Quit Spotify.", C),
        ("Close the window.", C),
        ("Can you close the window?", C),
        ("Open Claude.", C),
        ("Open KiCad.", C),
        ("Start Notepad.", C),
        ("Bring up my browser.", C),
        ("Open the Chrome window.", C),
        ("Uh, open Notepad.", C),
        ("Hey, open Chrome.", C),
        ("Open. Chrome.", C),
        ("Search for best laptops.", U),
        ("Can you launch Chrome?", U),
        ("Play Numb on Spotify.", U),
        ("Type hello.", U),
        // -- made up: dictation
        ("Open the door and then we walk.", T),
        ("I want to open Chrome later.", T),
        ("I think you should open Chrome.", T),
        ("Open the door.", T),
        ("Open the door for me please.", T),
        ("Open the pull request. Then tell me what the tests say.", T),
        ("Go to the store and buy some milk on your way home.", T),
        ("Play with the kids later.", T),
        ("Start of the meeting is at nine.", T),
        ("Close the deal by Friday.", T),
        ("Open your mind to new ideas.", T),
        ("Open parenthesis", T),
    ];

    /// More made-up dictation that starts like a command.
    const MORE_TEXT: &[&str] = &[
        "Write an email explaining the project to the team.",
        "Search for a better way to explain this to the client tomorrow.",
        "Next time we should start earlier.",
        "Open source is great.",
        "Close enough.",
        "Open Chrome and check whether the login works on staging before we ship.",
        "Can you open the attachment I sent you?",
        "We need to open a new office in Pune.",
        "Launch the product next week.",
        "Play the long game.",
        "Start the meeting without me.",
        "Run with it.",
        "Close the gap between the two teams.",
        "I think we should open the office earlier on Monday.",
        "Open the file I sent you yesterday and check the totals.",
    ];

    fn labelled() -> Vec<(&'static str, Label)> {
        SET.iter().copied().chain(MORE_TEXT.iter().map(|s| (*s, T))).collect()
    }

    /// (true positives, false positives, false negatives, true negatives) over the sure labels.
    fn counts(calls: &[(Label, bool)]) -> (usize, usize, usize, usize) {
        let n = |label: Label, said_command: bool| calls.iter().filter(|(l, c)| *l == label && *c == said_command).count();
        (n(C, true), n(T, true), n(C, false), n(T, false))
    }

    #[test]
    fn in_a_text_field_commands_are_told_from_typing_on_the_labelled_set() {
        let set = labelled();
        assert!(set.len() >= 120, "{}", set.len());
        let (mut new_calls, mut old_calls, mut wrong) = (Vec::new(), Vec::new(), Vec::new());
        for (said, label) in &set {
            let (now, before, verdict) = decide(said, true);
            new_calls.push((*label, now));
            old_calls.push((*label, before));
            if (*label == C && !now) || (*label == T && now) {
                wrong.push(format!("{said:?} ({label:?}) p={:.2}", verdict.p_command));
            }
            if (*label == C && !before) || (*label == T && before) {
                println!("old rule wrong: {said:?} ({label:?})");
            }
            if *label == U {
                println!("unsure {said:?}: {} p={:.2}", if now { "command" } else { "typed" }, verdict.p_command);
            }
        }
        let report = |name: &str, calls: &[(Label, bool)]| {
            let (tp, fp, fn_, tn) = counts(calls);
            let precision = tp as f64 / (tp + fp).max(1) as f64;
            let recall = tp as f64 / (tp + fn_).max(1) as f64;
            println!("{name}: tp {tp} fp {fp} fn {fn_} tn {tn}  precision {precision:.3} recall {recall:.3}");
            (precision, recall)
        };
        let _ = report("old rule", &old_calls);
        let (precision, recall) = report("scorer", &new_calls);
        println!("wrong: {wrong:#?}");
        assert!(precision >= 0.97, "typing run as a command: {wrong:#?}");
        assert!(recall >= 0.95, "commands typed: {wrong:#?}");
    }

    #[test]
    fn the_reported_bug_open_another_app_from_notepad_is_a_command() {
        for said in ["Open Chrome please.", "Open chat GPT.", "Hey, open Chrome.", "Open. Chrome.", "Open YouTube."] {
            let (command, _, verdict) = decide(said, true);
            assert!(command, "{said}: p={:.2}", verdict.p_command);
        }
    }

    #[test]
    fn talk_length_and_missing_targets_keep_a_phrase_text() {
        for said in ["I want to open Chrome later.", "Open the door and then we walk.", "Open Chrome and check whether the login works on staging before we ship."] {
            let (command, _, verdict) = decide(said, true);
            assert!(!command, "{said}: p={:.2}", verdict.p_command);
        }
    }

    #[test]
    fn with_no_text_field_the_bar_is_lower() {
        let (command, _, verdict) = decide("Can you launch Chrome?", false);
        assert!(command && verdict.margin > 0.0, "{verdict:?}");
        let (typed, _, _) = decide("Can you launch Chrome?", true);
        assert!(!typed, "unsure in a text field: typed");
    }

    #[test]
    fn a_site_stands_in_for_an_app_that_is_not_there_only() {
        let launch = |v: &str| ButtonAction { kind: "LaunchApp".into(), value: v.into(), steps: vec![] };
        assert_eq!(site_for_missing_app(launch("chat gpt"), &|_| false).value, "https://chatgpt.com");
        assert_eq!(site_for_missing_app(launch("chat gpt"), &|_| true).kind, "LaunchApp", "the installed app wins");
        assert_eq!(site_for_missing_app(launch("the door"), &|_| false).kind, "LaunchApp");
    }

    #[test]
    fn an_editing_phrase_said_alone_is_a_command_in_a_text_field() {
        for said in ["Copy.", "Paste.", "Paste it.", "Select all.", "Undo.", "Redo", "Ctrl Z.", "Control-Z", "Save it.", "Press enter.", "Enter.", "Escape", "Go back.", "Zoom in.", "Select word.", "Go to the end of the line.", "Hey Oasis, copy."] {
            let (command, _, verdict) = decide(said, true);
            assert!(command && verdict.p_command >= 0.99, "{said}: p={:.2}", verdict.p_command);
        }
        for said in ["Copy the file.", "I want to paste this later.", "Save the date please.", "Select all the things we discussed.", "I need to copy the file."] {
            let (command, _, verdict) = decide(said, true);
            assert!(!command, "{said}: p={:.2}", verdict.p_command);
        }
    }

    #[test]
    fn the_editing_rule_needs_the_setting_and_the_whole_phrase() {
        let phrase = prepare("Copy.");
        let copy = |edit: bool| judge("Copy.", &phrase, Found::Known(1.0), SafetyTier::Safe, true, edit);
        assert!(copy(true).command && copy(true).p_command >= 0.99);
        assert!(!copy(false).command, "off: the old scoring, which types a bare \"copy\"");
        let polite = prepare("I want to copy.");
        assert!(polite.carrier && polite.command == "copy");
        assert!(judge("I want to copy.", &polite, Found::Known(1.0), SafetyTier::Safe, true, true).p_command < 0.99, "said to someone, not on its own");
    }

    #[test]
    fn closing_still_counts_as_caution() {
        let close = ButtonAction { kind: "CloseApp".into(), value: "WhatsApp".into(), steps: vec![] };
        assert_eq!(tier(&close), SafetyTier::Caution);
    }
}
