//! Type it or run it: the whole decision for one spoken phrase (`docs/VOICE_DECISIONS.md` V3, V4,
//! V5). The engine gathers the facts (text field in front, settings, which apps exist, whether a
//! launch starts a shell) and carries out the [`Decision`]; every message and security-log line
//! is chosen here so the table can be tested without a PC.

use std::collections::HashSet;

use crate::actions::{ActionKind, ButtonAction};
use crate::speech::GlossaryEntry;

use super::classifier::looks_like_command;
use super::gate;
use super::intent;
use super::router::{find_running, RouteResult};
use super::safety::{fast_path, steps_of, ApprovalMode, FastPath, SafetyTier};
use super::text::is_usable_command;
use super::RunningApp;

/// What is known about a phrase and the PC when it was heard.
pub struct Facts<'a> {
    pub transcript: &'a str,
    /// A text field had focus.
    pub in_text_field: bool,
    /// The user's fragments ("my LinkedIn" → the link).
    pub fragments: &'a [(String, String)],
    /// The user's dictionary, applied to commands as to dictation.
    pub glossary: &'a [GlossaryEntry],
    /// "Type text by voice" is on.
    pub allow_type_text: bool,
    /// "copy", "undo" said on their own press their keys.
    pub edit_commands: bool,
    /// profiles.json matched its signature.
    pub verified: bool,
    /// Actions are on (not a dry run).
    pub live: bool,
    pub trusted: &'a HashSet<String>,
    pub approval: ApprovalMode,
}

/// What to do with the phrase.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// A fragment said on its own: paste `value`, show `shown`.
    PasteFragment { shown: String, value: String },
    /// Dictation: type what was said.
    Type,
    /// Nothing runs; say so (`outcome` is "ignored", "blocked" or "preview") and maybe log it.
    Report { outcome: &'static str, label: String, detail: String, log: Option<String> },
    /// Run it now.
    Run { action: ButtonAction, label: String },
    /// Put it on the pad: button 1 runs it, button 2 cancels.
    Ask { action: ButtonAction, label: String, summary: String },
    /// Nothing matched: report it, then either type it (it reads as dictation) or put it on the
    /// clipboard in case it was meant as text.
    NoMatch { detail: String, log: String, then_type: bool, copy_to_clipboard: bool },
}

/// A decision and the notes to show before it ("There is no app called …").
#[derive(Debug, Clone, PartialEq)]
pub struct Decided {
    pub notes: Vec<String>,
    pub decision: Decision,
}

/// Whether the program a `LaunchApp` names is really there: running now, installed, or a browser
/// with a page (`chrome|youtube.com`). A spoken "open the door" names nothing.
pub fn launch_target_exists(value: &str, running: &[RunningApp], installed: &dyn Fn(&str) -> bool) -> bool {
    value.contains('|') || find_running(value, running).is_some() || installed(value)
}

fn report(outcome: &'static str, label: &str, detail: &str, log: Option<String>) -> Decision {
    Decision::Report { outcome, label: label.into(), detail: detail.into(), log }
}

/// Decide what one phrase does. `route` routes the (glossary-fixed) command text; it is only
/// called once the phrase is worth routing. `exists` says whether a launch target is on this PC
/// ([`launch_target_exists`]); `starts_a_shell` whether a launch really starts a shell.
pub fn decide(
    f: &Facts<'_>,
    route: &mut dyn FnMut(&str) -> RouteResult,
    exists: &dyn Fn(&str) -> bool,
    starts_a_shell: &dyn Fn(&ButtonAction) -> bool,
) -> Decided {
    let done = |decision| Decided { notes: Vec::new(), decision };
    let (transcript, in_text_field) = (f.transcript, f.in_text_field);
    // A fragment said on its own ("insert my LinkedIn") types its text exactly (V5), in a text
    // field or not. Without one it is typing by voice, so it follows that setting.
    if let Some(text) = crate::speech::fragments::whole_phrase(transcript, f.fragments) {
        if in_text_field || f.allow_type_text {
            let value = if text.ends_with(char::is_whitespace) { text.to_string() } else { format!("{text} ") };
            return done(Decision::PasteFragment { shown: text.to_string(), value });
        }
        return done(report("blocked", "FRAGMENT", "Typing text by voice is off in Settings.", None));
    }
    let phrase = gate::prepare(transcript);
    if !in_text_field && gate::is_prose_about_writing(&phrase) {
        return done(Decision::Type);
    }
    if !is_usable_command(transcript) || phrase.command.is_empty() {
        if in_text_field {
            return done(Decision::Type);
        }
        return done(report("ignored", "", "Heard nothing usable.", Some(format!("Voice ignored (empty/noise): \"{transcript}\""))));
    }
    // "search this" with nothing after it (or cut off): no browser, no question, not typed (W15).
    if super::web_search::is_empty_search(&phrase.command) {
        return done(report("ignored", "", "A search with nothing to search for.", Some(format!("Voice ignored (empty search): \"{transcript}\""))));
    }
    let text = crate::speech::glossary::apply(&phrase.command, f.glossary).0;
    let routed = route(&text);
    // An app that is not on this PC is no command: "open the door" stays words, "open chatgpt" opens
    // the site. In "open X on … and open Y on …" the apps that are there still open.
    let (found, missing) = match routed.action.clone().filter(|_| routed.success) {
        Some(action) => without_missing_apps(intent::site_for_missing_app(action, exists), exists),
        None => (None, Vec::new()),
    };
    // With a text field in front, the transcript and its target must make a clear command; an
    // unsure phrase is typed into the field (intent::judge). Without one, the flow below decides.
    let evidence = intent::found(routed.success, routed.score, found.as_ref());
    let tier = found.as_ref().map_or(SafetyTier::Safe, intent::tier);
    let verdict = intent::judge(transcript, &phrase, evidence, tier, in_text_field, f.edit_commands);
    let Some(action) = found.filter(|_| !in_text_field || verdict.command) else {
        return done(no_command(f, &phrase, &routed, exists));
    };
    let notes = missing.iter().map(|app| format!("There is no app called \"{app}\" on this PC; the rest still runs.")).collect();
    let label = routed.label;
    let decision = if !f.verified {
        report("blocked", &label, "Voice actions are off: profiles.json did not match its signature.", None)
    } else if starts_a_shell(&action) {
        let log = format!("Voice launch refused, it starts a shell: \"{text}\" → {}", action.value);
        report("blocked", &label, "Blocked for safety.", Some(log))
    } else {
        match fast_path(&action, f.trusted, f.approval) {
            FastPath::Refuse => {
                let log = format!("Voice fast path refused: \"{text}\" → {} / {}", action.kind, action.value);
                report("blocked", &label, "Blocked for safety.", Some(log))
            }
            FastPath::Run => Decision::Run { action, label },
            FastPath::Ask if !f.live => report("preview", &label, "It would ask first. Actions are off, so nothing happened.", None),
            FastPath::Ask => {
                let summary = format!("Run “{label}”?  Button 1: run · Button 2: cancel");
                Decision::Ask { action, label, summary }
            }
        }
    };
    Decided { notes, decision }
}

/// Nothing sure to run. In a text field it was meant to be typed; with no field found it is typed
/// too when it reads as prose ("I want to write an email…"), so a dictation is not lost because
/// the field was never clicked. Otherwise: if one thing came close, it is safe and it exists, ask
/// about it on the pad ("Did you mean …?"); never run it unasked. Else say it was not understood.
fn no_command(f: &Facts<'_>, phrase: &gate::Phrase, routed: &RouteResult, exists: &dyn Fn(&str) -> bool) -> Decision {
    let prose = !gate::reads_as_command(phrase);
    if (f.in_text_field || prose) && !phrase.addressed {
        return Decision::Type;
    }
    let is_launch = |kind: &str| ActionKind::of_ignore_case(kind) == ActionKind::LaunchApp;
    let offer = routed
        .suggestion
        .clone()
        .filter(|(a, _)| f.verified && fast_path(a, f.trusted, f.approval) != FastPath::Refuse)
        .filter(|(a, _)| !is_launch(&a.kind) || exists(&a.value));
    if let Some((action, label)) = offer {
        if !f.live {
            return report("preview", &label, "It would ask whether you meant this. Actions are off, so nothing happened.", None);
        }
        let summary = format!("Did you mean “{label}”?  Button 1: run · Button 2: cancel");
        return Decision::Ask { action, label, summary };
    }
    // The app that is missing, also when it is one step of "open X on … and open Y on …".
    let missing = routed.action.as_ref().and_then(|a| steps_of(a).into_iter().find(|(kind, value)| is_launch(kind) && !exists(value)));
    let detail = match missing {
        Some((_, app)) => format!("There is no app called \"{app}\" on this PC."),
        None => routed.message.clone().unwrap_or_else(|| "No match.".into()),
    };
    // Never lose a transcript (1.7): a miss that is long or does not start like a command may be
    // dictation into a field that was not found (or not readable): type it, and keep it on the
    // clipboard. A short command-like miss is reported and copied.
    let then_type = !phrase.addressed && (phrase.words > 4 || !looks_like_command(&phrase.command));
    Decision::NoMatch {
        log: format!("Voice no_match: \"{}\" ({detail})", f.transcript),
        detail,
        then_type,
        copy_to_clipboard: !then_type && f.verified && f.live,
    }
}

/// `action` without the apps that are not on this PC, and their names. One app: no action at all.
/// Several ("open X on … and open Y on …"): the missing app's launch and the move of its window
/// go, and what is left runs, as long as it still opens something.
fn without_missing_apps(action: ButtonAction, exists: &dyn Fn(&str) -> bool) -> (Option<ButtonAction>, Vec<String>) {
    let is_launch = |kind: &str| ActionKind::of_ignore_case(kind) == ActionKind::LaunchApp;
    if ActionKind::of_ignore_case(&action.kind) != ActionKind::Automation {
        if is_launch(&action.kind) && !exists(&action.value) {
            return (None, vec![action.value]);
        }
        return (Some(action), Vec::new());
    }
    let (mut missing, mut steps, mut skip_window) = (Vec::new(), Vec::new(), false);
    for step in action.steps {
        if is_launch(&step.kind) && !exists(&step.value) {
            missing.push(step.value);
            skip_window = true;
            continue;
        }
        if std::mem::take(&mut skip_window) && ActionKind::of_ignore_case(&step.kind) == ActionKind::WindowCommand {
            continue;
        }
        steps.push(step);
    }
    if !missing.is_empty() && !steps.iter().any(|s| is_launch(&s.kind)) {
        return (None, missing);
    }
    (Some(ButtonAction { kind: action.kind, value: action.value, steps }), missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::ActionStep;

    fn act(kind: &str, value: &str) -> ButtonAction {
        ButtonAction { kind: kind.into(), value: value.into(), steps: vec![] }
    }

    fn step(kind: &str, value: &str) -> ActionStep {
        ActionStep { kind: kind.into(), value: value.into(), ..Default::default() }
    }

    fn matched(action: ButtonAction, label: &str) -> RouteResult {
        RouteResult { success: true, transcript: String::new(), message: None, action: Some(action), label: label.into(), score: 1.0, suggestion: None }
    }

    fn missed() -> RouteResult {
        RouteResult { success: false, transcript: String::new(), message: None, action: None, label: String::new(), score: 0.0, suggestion: None }
    }

    /// Settings as on a fresh PC: signed profiles, actions on, nothing trusted yet.
    struct Pc {
        fragments: Vec<(String, String)>,
        trusted: HashSet<String>,
        in_text_field: bool,
        verified: bool,
        allow_type_text: bool,
        shell: bool,
    }

    fn pc() -> Pc {
        Pc { fragments: vec![], trusted: HashSet::new(), in_text_field: false, verified: true, allow_type_text: true, shell: false }
    }

    impl Pc {
        fn hear(&self, transcript: &str, routed: RouteResult) -> Decided {
            let facts = Facts {
                transcript,
                in_text_field: self.in_text_field,
                fragments: &self.fragments,
                glossary: &[],
                allow_type_text: self.allow_type_text,
                edit_commands: false,
                verified: self.verified,
                live: true,
                trusted: &self.trusted,
                approval: ApprovalMode::FirstUse,
            };
            let mut routed = Some(routed);
            let mut route = |_: &str| routed.take().expect("routed once");
            decide(&facts, &mut route, &|app: &str| app != "nowhere", &|_: &ButtonAction| self.shell)
        }
    }

    fn not_routed() -> RouteResult {
        panic!("this phrase must be decided before routing")
    }

    #[test]
    fn a_fragment_said_alone_is_pasted_or_blocked_by_the_setting() {
        let mut p = pc();
        p.fragments = vec![("my linkedin".into(), "https://in/me".into())];
        let d = p.hear("My LinkedIn.", missed());
        assert_eq!(d.decision, Decision::PasteFragment { shown: "https://in/me".into(), value: "https://in/me ".into() });
        p.allow_type_text = false;
        assert!(matches!(p.hear("my linkedin", missed()).decision, Decision::Report { outcome: "blocked", ref label, .. } if label == "FRAGMENT"));
    }

    #[test]
    fn prose_about_writing_with_no_field_is_typed_unrouted() {
        let mut route = |_: &str| not_routed();
        let p = pc();
        let facts = Facts {
            transcript: "write an email explaining the project to the team",
            in_text_field: false,
            fragments: &[],
            glossary: &[],
            allow_type_text: true,
            edit_commands: false,
            verified: true,
            live: true,
            trusted: &p.trusted,
            approval: ApprovalMode::FirstUse,
        };
        assert_eq!(decide(&facts, &mut route, &|_: &str| true, &|_: &ButtonAction| false).decision, Decision::Type);
    }

    #[test]
    fn an_unsure_phrase_in_a_text_field_is_typed() {
        let mut p = pc();
        p.in_text_field = true;
        let d = p.hear("I think we should open chrome later because it was slow", matched(act("LaunchApp", "chrome"), "Chrome"));
        assert_eq!(d.decision, Decision::Type);
    }

    #[test]
    fn a_clear_trusted_command_runs_and_an_untrusted_one_asks() {
        let mut p = pc();
        let d = p.hear("open chrome", matched(act("LaunchApp", "chrome"), "Chrome"));
        assert_eq!(
            d.decision,
            Decision::Ask { action: act("LaunchApp", "chrome"), label: "Chrome".into(), summary: "Run “Chrome”?  Button 1: run · Button 2: cancel".into() }
        );
        p.trusted.insert("launchapp:chrome".into());
        let d = p.hear("open chrome", matched(act("LaunchApp", "chrome"), "Chrome"));
        assert_eq!(d.decision, Decision::Run { action: act("LaunchApp", "chrome"), label: "Chrome".into() });
    }

    #[test]
    fn a_shell_or_a_bad_signature_is_blocked() {
        let mut p = pc();
        p.shell = true;
        let d = p.hear("open command", matched(act("LaunchApp", "command"), "Command"));
        assert_eq!(
            d.decision,
            report("blocked", "Command", "Blocked for safety.", Some("Voice launch refused, it starts a shell: \"open command\" → command".into()))
        );
        let mut p = pc();
        p.verified = false;
        let d = p.hear("open chrome", matched(act("LaunchApp", "chrome"), "Chrome"));
        assert_eq!(d.decision, report("blocked", "Chrome", "Voice actions are off: profiles.json did not match its signature.", None));
    }

    #[test]
    fn a_near_miss_asks_did_you_mean() {
        let routed = RouteResult { suggestion: Some((act("LaunchApp", "chrome"), "Chrome".into())), ..missed() };
        let d = pc().hear("open chrom", routed);
        assert_eq!(
            d.decision,
            Decision::Ask { action: act("LaunchApp", "chrome"), label: "Chrome".into(), summary: "Did you mean “Chrome”?  Button 1: run · Button 2: cancel".into() }
        );
    }

    #[test]
    fn a_long_miss_is_typed_and_a_short_command_like_one_is_copied() {
        let Decision::NoMatch { then_type, copy_to_clipboard, .. } = pc().hear("open the big blue door right now", missed()).decision else {
            panic!("no match expected")
        };
        assert!(then_type && !copy_to_clipboard);
        let d = pc().hear("open the dor", missed());
        assert_eq!(
            d.decision,
            Decision::NoMatch { detail: "No match.".into(), log: "Voice no_match: \"open the dor\" (No match.)".into(), then_type: false, copy_to_clipboard: true }
        );
    }

    #[test]
    fn a_missing_app_is_dropped_with_a_note_and_the_rest_runs() {
        let plan = ButtonAction {
            kind: "Automation".into(),
            value: String::new(),
            steps: vec![step("LaunchApp", "chrome"), step("WindowCommand", "left"), step("LaunchApp", "nowhere"), step("WindowCommand", "right")],
        };
        let mut p = pc();
        p.trusted.insert("launchapp:chrome".into());
        let d = p.hear("open chrome on the left and open nowhere on the right", matched(plan, "Two apps"));
        assert_eq!(d.notes, vec!["There is no app called \"nowhere\" on this PC; the rest still runs.".to_string()]);
        let Decision::Run { action, .. } = d.decision else { panic!("should run") };
        assert_eq!(action.steps, vec![step("LaunchApp", "chrome"), step("WindowCommand", "left")]);
    }

    #[test]
    fn noise_is_ignored_with_no_field_and_typed_in_one() {
        let mut p = pc();
        assert!(matches!(p.hear("um", missed()).decision, Decision::Report { outcome: "ignored", .. }));
        p.in_text_field = true;
        assert_eq!(p.hear("um", missed()).decision, Decision::Type);
    }
}
