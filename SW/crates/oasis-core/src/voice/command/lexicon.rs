//! The closed vocabulary spoken commands are matched against. Port of `VoiceLexicon` in
//! `Services/Voice/VoiceLexicon.cs`. Pure: the caller passes in the profile data and the list of
//! running apps, so the rules can be tested without a desktop.

use crate::actions::ButtonAction;

use super::text::normalize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexiconKind {
    /// A button's label: saying it runs that button.
    ButtonLabel,
    ProfileName,
    KnownApp,
    /// "volume up", "mute", …
    BuiltIn,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexiconEntry {
    pub phrase: String,
    pub normalized: String,
    pub kind: LexiconKind,
    pub action: Option<ButtonAction>,
    pub label: String,
    pub profile_name: Option<String>,
    pub app_alias: Option<String>,
    /// The button belongs to the active profile: wins a close tie.
    pub prefer_active: bool,
}

/// A labelled button of a profile.
#[derive(Debug, Clone, PartialEq)]
pub struct LabeledAction {
    pub label: String,
    pub action: ButtonAction,
}

/// A profile as the lexicon needs it: its name and the buttons of all its sets.
#[derive(Debug, Clone, PartialEq)]
pub struct LexiconProfile {
    pub name: String,
    pub buttons: Vec<LabeledAction>,
}

/// A running program: process name and its window title.
#[derive(Debug, Clone, PartialEq)]
pub struct RunningApp {
    pub process: String,
    pub title: String,
}

pub struct LexiconInputs<'a> {
    /// The profile in front, with the buttons of the set in use.
    pub active: Option<(&'a str, &'a [LabeledAction])>,
    pub profiles: &'a [LexiconProfile],
    pub running: &'a [RunningApp],
}

const KNOWN_APPS: [&str; 14] = [
    "cursor", "chrome", "edge", "firefox", "notepad", "calculator", "code", "vscode", "vs code",
    "visual studio code", "explorer", "kicad", "spotify", "oasis",
];

/// At most this many running programs are added.
const MAX_RUNNING: usize = 15;

pub fn build_lexicon(inputs: &LexiconInputs<'_>) -> Vec<LexiconEntry> {
    let mut list = Vec::with_capacity(64);

    if let Some((name, buttons)) = inputs.active {
        add_buttons(&mut list, buttons, true);
        add(&mut list, name, LexiconKind::ProfileName, None, Some(name), None, false);
    }
    for profile in inputs.profiles {
        add(&mut list, &profile.name, LexiconKind::ProfileName, None, Some(&profile.name), None, false);
        add_buttons(&mut list, &profile.buttons, false);
    }
    for alias in KNOWN_APPS {
        add(&mut list, alias, LexiconKind::KnownApp, None, None, Some(alias), false);
    }

    // Running programs and a short form of their window titles.
    let mut taken = 0;
    for app in inputs.running {
        if taken >= MAX_RUNNING {
            break;
        }
        if app.process.trim().is_empty() || app.process.eq_ignore_ascii_case("OASIS") {
            continue;
        }
        add(&mut list, &app.process, LexiconKind::KnownApp, None, None, Some(&app.process), false);
        taken += 1;
        if let Some(short) = short_title(&app.title) {
            if short.chars().count() >= 3 && !short.eq_ignore_ascii_case(&app.process) {
                add(&mut list, &short, LexiconKind::KnownApp, None, None, Some(&app.process), false);
            }
        }
    }

    for (phrase, value, label) in [
        ("volume up", "VolumeUp", "VOLUME UP"),
        ("volume down", "VolumeDown", "VOLUME DOWN"),
        ("mute", "VolumeMute", "MUTE"),
        ("volume mute", "VolumeMute", "MUTE"),
        ("play pause", "MediaPlayPause", "PLAY PAUSE"),
        ("next track", "MediaNextTrack", "NEXT"),
        ("previous track", "MediaPreviousTrack", "PREV"),
    ] {
        let action = ButtonAction { kind: "Shortcut".into(), value: value.into(), steps: vec![] };
        add(&mut list, phrase, LexiconKind::BuiltIn, Some((action, label.to_string())), None, None, false);
    }
    list
}

/// The part of a window title before a dash or bar, at most 24 characters.
fn short_title(title: &str) -> Option<String> {
    let first = title.split(['-', '—', '|', '·']).next()?.trim();
    let cut: String = first.chars().take(24).collect();
    let cut = cut.trim().to_string();
    (!cut.is_empty()).then_some(cut)
}

fn add_buttons(list: &mut Vec<LexiconEntry>, buttons: &[LabeledAction], prefer_active: bool) {
    for button in buttons.iter().filter(|b| !b.label.trim().is_empty()) {
        add(
            list,
            &button.label,
            LexiconKind::ButtonLabel,
            Some((button.action.clone(), button.label.clone())),
            None,
            None,
            prefer_active,
        );
    }
}

fn add(
    list: &mut Vec<LexiconEntry>,
    phrase: &str,
    kind: LexiconKind,
    action: Option<(ButtonAction, String)>,
    profile_name: Option<&str>,
    app_alias: Option<&str>,
    prefer_active: bool,
) {
    let normalized = normalize(phrase);
    if normalized.is_empty() {
        return;
    }
    let (action, label) = action.map_or((None, String::new()), |(a, l)| (Some(a), l));
    // The same thing listed twice (the active profile is also one of all profiles) would tie with
    // itself and make every exact match "not confident". Keep one, remembering it is preferred.
    if let Some(same) = list.iter_mut().find(|e| {
        e.normalized == normalized && e.kind == kind && e.action == action
            && e.profile_name.as_deref() == profile_name && e.app_alias.as_deref() == app_alias
    }) {
        same.prefer_active |= prefer_active;
        return;
    }
    list.push(LexiconEntry {
        phrase: phrase.trim().to_string(),
        normalized,
        kind,
        action,
        label,
        profile_name: profile_name.map(str::to_string),
        app_alias: app_alias.map(str::to_string),
        prefer_active,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut(value: &str) -> ButtonAction {
        ButtonAction { kind: "Shortcut".into(), value: value.into(), steps: vec![] }
    }

    fn button(label: &str, value: &str) -> LabeledAction {
        LabeledAction { label: label.into(), action: shortcut(value) }
    }

    #[test]
    fn button_labels_profile_names_apps_and_builtins_are_all_in() {
        let active = vec![button("NEW TAB", "Ctrl+T")];
        let profiles = vec![LexiconProfile { name: "Chrome".into(), buttons: vec![button("BACK", "Alt+Left"), button("", "x")] }];
        let lexicon = build_lexicon(&LexiconInputs { active: Some(("Chrome", &active)), profiles: &profiles, running: &[] });
        let has = |phrase: &str, kind| lexicon.iter().any(|e| e.normalized == phrase && e.kind == kind);
        assert!(has("new tab", LexiconKind::ButtonLabel));
        assert!(has("back", LexiconKind::ButtonLabel));
        assert!(has("chrome", LexiconKind::ProfileName));
        assert!(has("spotify", LexiconKind::KnownApp));
        assert!(has("volume up", LexiconKind::BuiltIn));
        assert!(lexicon.iter().all(|e| !e.normalized.is_empty()), "a blank label adds nothing");
        assert!(lexicon.iter().find(|e| e.normalized == "new tab").unwrap().prefer_active);
        assert!(!lexicon.iter().find(|e| e.normalized == "back").unwrap().prefer_active);
    }

    #[test]
    fn running_programs_and_short_window_titles_are_added_but_not_oasis_itself() {
        let running = vec![
            RunningApp { process: "slack".into(), title: "general - Acme - Slack".into() },
            RunningApp { process: "OASIS".into(), title: "OASIS".into() },
            RunningApp { process: "figma".into(), title: "Figma".into() },
        ];
        let lexicon = build_lexicon(&LexiconInputs { active: None, profiles: &[], running: &running });
        let alias_of = |phrase: &str| lexicon.iter().find(|e| e.normalized == phrase).and_then(|e| e.app_alias.clone());
        assert_eq!(alias_of("slack").as_deref(), Some("slack"));
        assert_eq!(alias_of("general").as_deref(), Some("slack"), "the title before the first dash");
        assert_eq!(lexicon.iter().filter(|e| e.normalized == "figma").count(), 1, "a title equal to the process is not repeated");
    }

    #[test]
    fn only_the_first_fifteen_running_programs_are_used() {
        let running: Vec<RunningApp> = (0..30).map(|i| RunningApp { process: format!("app{i:02}"), title: String::new() }).collect();
        let lexicon = build_lexicon(&LexiconInputs { active: None, profiles: &[], running: &running });
        assert!(lexicon.iter().any(|e| e.normalized == "app14"));
        assert!(!lexicon.iter().any(|e| e.normalized == "app15"));
    }
}
