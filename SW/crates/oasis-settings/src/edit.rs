//! Changing the settings from a settings window, safely.
//!
//! Every edit is checked with the same rules the engine uses to run an action, so a button
//! cannot be saved in a form that would silently fail at the first press: a shortcut that does
//! not parse, a link the allowlist refuses, a mouse position that is not `x,y`. Fields this crate
//! does not model are kept: an edit changes only what it names.
//!
//! This is new in Rust: the C# app validated inside its WPF editors.

use std::fmt;

use oasis_core::actions::{try_allow, ActionKind, ActionStep, MouseClick, Next, UrlScope};
use oasis_core::display_label::{fit as fit_label, DEFAULT_MAX_LENGTH};
use oasis_core::matching::profile_targets::{infer_kind, website_match, APP_KIND, WEBSITE_KIND};
use oasis_core::shortcut::Chord;
use serde_json::{json, Value};

use crate::model::{Button, ConfigProfile, Profile, Settings};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    NoSuchProfile(usize),
    NoSuchConfig { profile: usize, config: usize },
    NoSuchButton { profile: usize, config: usize, button: usize },
    /// The change is not allowed; the text says why, for the person editing.
    Invalid(String),
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::NoSuchProfile(i) => write!(f, "There is no profile number {}.", i + 1),
            EditError::NoSuchConfig { profile, config } => {
                write!(f, "Profile {} has no configuration number {}.", profile + 1, config + 1)
            }
            EditError::NoSuchButton { button, .. } => write!(f, "There is no button {}.", button + 1),
            EditError::Invalid(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for EditError {}

fn invalid<T>(why: impl Into<String>) -> Result<T, EditError> {
    Err(EditError::Invalid(why.into()))
}

/// What a button should become. `kind` is the action type text (`Shortcut`, `LaunchApp`,
/// `OpenUrl`, `MouseClick`, `PasteText`, `RunScript`, `JoinMeeting`, `Automation`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ButtonEdit {
    pub kind: String,
    pub value: String,
    pub label: String,
    /// Only used when `kind` is `Automation`.
    pub steps: Vec<ActionStep>,
}

/// The action types a button can have, in the order a menu shows them.
pub const ACTION_TYPES: [&str; 8] = [
    ActionKind::Shortcut.as_str(), ActionKind::LaunchApp.as_str(), ActionKind::OpenUrl.as_str(), ActionKind::MouseClick.as_str(), ActionKind::PasteText.as_str(), ActionKind::RunScript.as_str(), ActionKind::JoinMeeting.as_str(), ActionKind::Automation.as_str(),
];

/// Whether `kind` + `value` can be saved. A blank value means "nothing bound" and is always
/// fine; it is what a new button has.
pub fn validate_action(kind: &str, value: &str) -> Result<(), String> {
    let blank = value.trim().is_empty();
    match ActionKind::of(kind) {
        ActionKind::Shortcut if !blank => Chord::parse(value).map(|_| ()),
        ActionKind::OpenUrl if !blank => try_allow(value, UrlScope::Automation).map(|_| ()).ok_or_else(|| {
            "That address cannot be opened. Only https, http and spotify links are allowed.".to_string()
        }),
        ActionKind::MouseClick if !blank => MouseClick::parse(value)
            .map(|_| ())
            .ok_or_else(|| "Use the screen position as x,y (for example 120,340), or x,y,right.".to_string()),
        ActionKind::AutoHotkey if value.len() > 64 * 1024 => Err("That script is too long (64 KB at most).".to_string()),
        ActionKind::Shortcut
        | ActionKind::OpenUrl
        | ActionKind::MouseClick
        | ActionKind::LaunchApp
        | ActionKind::PasteText
        | ActionKind::RunScript
        | ActionKind::JoinMeeting
        | ActionKind::Automation
        | ActionKind::AutoHotkey => Ok(()),
        _ => Err(format!("Unknown action type '{kind}'.")),
    }
}

/// The step types the step menu offers, most used first (Quick Launch leads, as on the home page).
/// "Wait for Release" was removed (an old one is kept and skipped when run); a fixed "Delay" is no longer
/// offered, since each step now waits until the one before it is ready (an old one still runs).
pub const STEP_TYPES: [&str; 8] =
    [ActionKind::QuickLaunch.as_str(), ActionKind::LaunchApp.as_str(), ActionKind::OpenUrl.as_str(), ActionKind::Shortcut.as_str(), ActionKind::AutoHotkey.as_str(), ActionKind::PlaySound.as_str(), ActionKind::MouseClick.as_str(), ActionKind::JoinMeeting.as_str()];

/// Step types that may be written. Beyond the editor's own, a script step (how the C# script mode
/// stores a script) and a paste step are accepted. A step of any other kind is kept, not editable.
pub const WRITABLE_STEP_TYPES: [&str; 12] = [
    ActionKind::QuickLaunch.as_str(),
    ActionKind::LaunchApp.as_str(),
    "Launch App", // the old spelling, still accepted
    ActionKind::OpenUrl.as_str(),
    ActionKind::Shortcut.as_str(),
    ActionKind::AutoHotkey.as_str(),
    ActionKind::Delay.as_str(),
    ActionKind::MouseClick.as_str(),
    ActionKind::JoinMeeting.as_str(),
    ActionKind::RunScript.as_str(),
    ActionKind::PasteText.as_str(),
    ActionKind::PlaySound.as_str(),
];

/// A step as saved. The links and the place in the builder are written only when set, so a plain
/// list keeps the `{ Type, Value }` shape the C# app reads.
fn step_json(step: &ActionStep) -> Value {
    let mut out = json!({ "Type": step.kind, "Value": step.value });
    for (key, code) in [("Next", step.on_ok.code()), ("OnFail", step.on_fail.code())] {
        if let Some(code) = code {
            out[key] = json!(code);
        }
    }
    if let Some((x, y)) = step.pos {
        out["X"] = json!(x);
        out["Y"] = json!(y);
    }
    out
}

/// A step inside an automation. Unlike a lone button, a step with nothing to do is a mistake.
/// `existing` are the button's current steps: one of a kind this editor cannot create (an AI plan's
/// "focus window") may stay if it is left exactly as it is.
fn validate_step(number: usize, step: &ActionStep, existing: &[ActionStep]) -> Result<(), String> {
    let prefix = format!("Step {number}: ");
    // Moving or rewiring a kept step in the builder does not change what it is.
    let kept = existing.iter().any(|e| e.kind == step.kind && e.value == step.value);
    if !WRITABLE_STEP_TYPES.contains(&step.kind.as_str()) && !kept {
        return Err(format!("{prefix}'{}' steps cannot be created here.", step.kind));
    }
    if !WRITABLE_STEP_TYPES.contains(&step.kind.as_str()) {
        return Ok(()); // kept as it was
    }
    match step.kind() {
        ActionKind::PlaySound if oasis_core::actions::sounds::find(&step.value).is_none() => Err(format!("{prefix}pick a sound.")),
        ActionKind::PlaySound => Ok(()),
        ActionKind::Delay => step
            .value
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|ms| *ms >= 0)
            .map(|_| ())
            .ok_or_else(|| format!("{prefix}a delay is a number of milliseconds, 0 or more.")),
        ActionKind::JoinMeeting => Ok(()),
        // The value is a Quick Launch setup's id; `set_button` checks that it exists.
        ActionKind::QuickLaunch if step.value.trim().is_empty() => Err(format!("{prefix}pick the Quick Launch setup to open.")),
        ActionKind::QuickLaunch => Ok(()),
        _ => {
            let kind = step.kind.as_str();
            if step.value.trim().is_empty() {
                return Err(format!("{prefix}it needs a value."));
            }
            validate_action(kind, &step.value).map_err(|why| format!("{prefix}{why}"))
        }
    }
}

impl Settings {
    /// Change one button. Everything about it that the edit does not name (fields we do not
    /// model) is kept.
    pub fn set_button(
        &mut self,
        profile: usize,
        config: usize,
        index: usize,
        edit: ButtonEdit,
    ) -> Result<(), EditError> {
        validate_action(&edit.kind, &edit.value).or_else(invalid)?;
        let existing = self
            .profiles
            .get(profile)
            .and_then(|p| p.config_profiles.get(config))
            .and_then(|c| c.buttons.get(index))
            .map(Button::automation_steps)
            .unwrap_or_default();
        if ActionKind::of(&edit.kind) == ActionKind::Automation && edit.steps.is_empty() {
            return invalid("Add at least one step, or choose what the button does instead.");
        }
        for (n, step) in edit.steps.iter().enumerate() {
            validate_step(n + 1, step, &existing).or_else(invalid)?;
            if step.kind() == ActionKind::QuickLaunch && crate::quick_launch::find(self, &step.value).is_none() {
                return invalid(format!("Step {}: that Quick Launch setup does not exist.", n + 1));
            }
            if [step.on_ok, step.on_fail].iter().any(|l| matches!(l, Next::Step(t) if *t >= edit.steps.len())) {
                return invalid(format!("Step {}: it is joined to a step that is not there.", n + 1));
            }
        }

        let owner = self.profiles.get_mut(profile).ok_or(EditError::NoSuchProfile(profile))?;
        let cfg = owner
            .config_profiles
            .get_mut(config)
            .ok_or(EditError::NoSuchConfig { profile, config })?;
        let button = cfg
            .buttons
            .get_mut(index)
            .ok_or(EditError::NoSuchButton { profile, config, button: index })?;

        let was_automation = ActionKind::of(&button.kind) == ActionKind::Automation;
        button.kind = edit.kind.clone();
        button.value = if ActionKind::of(&edit.kind) == ActionKind::Automation { String::new() } else { edit.value };
        // Shown on the pad in capitals and shortened to fit, as in C#. A blank label stays blank.
        button.label = if edit.label.trim().is_empty() {
            String::new()
        } else {
            fit_label(edit.label.trim(), DEFAULT_MAX_LENGTH)
        };
        button.steps = if ActionKind::of(&edit.kind) == ActionKind::Automation {
            Value::Array(
                edit.steps.iter().map(step_json).collect(),
            )
        } else if was_automation || !matches!(button.steps, Value::String(_) | Value::Array(_)) {
            // Leaving automation, or an odd stored value: the plain empty form.
            json!([])
        } else {
            std::mem::take(&mut button.steps)
        };
        Ok(())
    }

    /// Rename a profile or change what it matches. A profile that matches everything (`*` or
    /// blank) has no kind; any other is an application or a website, guessed from the match text
    /// when not given. The profile's main configuration keeps the profile's name, as the C# app
    /// shows them.
    pub fn update_profile(
        &mut self,
        index: usize,
        name: &str,
        context_match: &str,
        kind: &str,
    ) -> Result<(), EditError> {
        let name = name.trim();
        if name.is_empty() {
            return invalid("A profile needs a name.");
        }
        check_length(name, MAX_PROFILE_NAME, "A profile name")?;
        check_length(context_match.trim(), MAX_MATCH, "What a profile applies to")?;
        let matches_all = context_match.trim().is_empty() || context_match.trim() == "*";
        let kind = match (matches_all, kind.trim().to_lowercase().as_str()) {
            (true, _) => String::new(),
            (false, k @ (APP_KIND | WEBSITE_KIND)) => k.to_string(),
            (false, "") => infer_kind(Some(context_match)).to_string(),
            (false, other) => return invalid(format!("A profile is for an app or a website, not '{other}'.")),
        };

        let profile = self.profiles.get_mut(index).ok_or(EditError::NoSuchProfile(index))?;
        if let Some(primary) = profile.primary_config_index() {
            let config = &mut profile.config_profiles[primary];
            if config.name == profile.name {
                config.name = name.to_string();
            }
        }
        profile.name = name.to_string();
        profile.context_match = if matches_all { "*".into() } else { context_match.trim().to_string() };
        profile.kind = kind;
        Ok(())
    }
}

/// How many sets of buttons one profile can have (the C# window has six slots).
pub const MAX_SETS_PER_PROFILE: usize = 6;
/// Longest names and conditions, in characters. The window stops typing at these; this refuses the rest.
pub const MAX_PROFILE_NAME: usize = 40;
pub const MAX_SET_NAME: usize = 24;
pub const MAX_MATCH: usize = 200;

/// A name or condition no longer than `max` characters, or the reason it is refused.
fn check_length(text: &str, max: usize, what: &str) -> Result<(), EditError> {
    if text.chars().count() > max {
        return invalid(&format!("{what} can be at most {max} characters."));
    }
    Ok(())
}

/// Panels a set of buttons can be tied to, as the C# editor's built-in list. (The C# app can also
/// offer the panels its app catalog knows for a code editor; those come with panel detection.)
pub const PANEL_PRESETS: [(&str, &str); 8] = [
    ("editor", "Code Editor"),
    ("chat", "AI Chat / Composer"),
    ("agent", "AI Agent"),
    ("terminal", "Integrated Terminal"),
    ("output", "Output Panel"),
    ("explorer", "File Explorer / Sidebar"),
    ("debug-console", "Debug Console"),
    ("settings", "Settings / Preferences"),
];

/// When a set of buttons applies, as the C# window words it: `(value, label)`.
pub const WHEN_OPTIONS: [(&str, &str); 4] = [
    ("", "Always"),
    ("youtube.playing", "Video playing"),
    ("youtube.shorts", "YouTube Short"),
    ("youtube.ended", "Video ended"),
];

/// What a set of buttons should become. Its buttons and the panel hints this window cannot set
/// (`FocusClassName`, `AncestorHint`) are not part of the edit and are kept.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SetEdit {
    pub name: String,
    /// Part of the window title or address this set applies to. Ignored for the main set.
    pub context_match: String,
    /// `""`, `youtube.playing`, `youtube.shorts` or `youtube.ended` (the older spellings are read).
    pub when: String,
    /// One of [`PANEL_PRESETS`], or empty.
    pub panel_id: String,
    /// How the overlay shows for this set: `""` (follow the profile), `show` or `hide`.
    pub label_mode: String,
}

impl Settings {
    /// Remember where the user put the overlay (screen units, not negative).
    pub fn set_overlay_position(&mut self, x: i32, y: i32) -> Result<(), EditError> {
        if x < 0 || y < 0 {
            return invalid("The overlay position cannot be negative.");
        }
        self.overlay_x = x.into();
        self.overlay_y = y.into();
        Ok(())
    }

    /// Set how a profile shows its labels (`""` follows the general setting, `show` reveals them
    /// on the first tap, `hide` runs a tap at once) and its emoji icon (at most 8 characters,
    /// nothing invisible). An emoji picked here replaces nothing else: a picture icon is kept.
    pub fn set_profile_look(&mut self, index: usize, label_mode: &str, icon_emoji: &str) -> Result<(), EditError> {
        let mode = match label_mode.trim().to_lowercase().as_str() {
            "" | "inherit" | "follow" => "",
            "show" => "show",
            "hide" => "hide",
            other => return invalid(format!("Label behaviour must be follow, show or hide, not '{other}'.")),
        };
        let emoji = icon_emoji.trim();
        if emoji.chars().count() > 8 {
            return invalid("An emoji icon is at most 8 characters.");
        }
        if emoji.chars().any(char::is_control) {
            return invalid("An emoji icon cannot contain control characters.");
        }
        let profile = self.profiles.get_mut(index).ok_or(EditError::NoSuchProfile(index))?;
        profile.label_mode = mode.to_string();
        profile.icon_emoji = emoji.to_string();
        Ok(())
    }

    /// Point a profile at a picture icon (a managed `profile_icons/<id>.png`), or at none (`""`).
    pub fn set_profile_icon_path(&mut self, index: usize, icon_path: &str) -> Result<(), EditError> {
        let profile = self.profiles.get_mut(index).ok_or(EditError::NoSuchProfile(index))?;
        profile.icon_path = icon_path.trim().to_string();
        Ok(())
    }

    /// Turn a profile's extra sets of buttons on or off. Off keeps them (only the main set is
    /// used); nothing is deleted.
    pub fn set_multi_config(&mut self, profile: usize, enabled: bool) -> Result<(), EditError> {
        self.profiles.get_mut(profile).ok_or(EditError::NoSuchProfile(profile))?.multi_config_enabled =
            enabled;
        Ok(())
    }

    /// Add a set of buttons to a profile (three blank buttons) and return its index. Turns the
    /// profile's extra sets on, since a set that is never used is not what anyone adds.
    pub fn add_config(&mut self, profile: usize, edit: SetEdit) -> Result<usize, EditError> {
        let owner = self.profiles.get_mut(profile).ok_or(EditError::NoSuchProfile(profile))?;
        if owner.config_profiles.len() >= MAX_SETS_PER_PROFILE {
            return invalid(format!("A profile can have up to {MAX_SETS_PER_PROFILE} sets of buttons."));
        }
        let mut config = ConfigProfile {
            buttons: vec![Button::blank(), Button::blank(), Button::blank()],
            ..ConfigProfile::default()
        };
        apply_set_edit(&mut config, edit, false)?;
        refuse_duplicate(&owner.config_profiles, None, &config)?;
        owner.config_profiles.push(config);
        owner.multi_config_enabled = true;
        Ok(owner.config_profiles.len() - 1)
    }

    /// Change a set's name, condition and panel. The main set stays the main set: it always
    /// matches `*` and has no condition, whatever is asked.
    pub fn update_config(&mut self, profile: usize, config: usize, edit: SetEdit) -> Result<(), EditError> {
        let owner = self.profiles.get_mut(profile).ok_or(EditError::NoSuchProfile(profile))?;
        let mut draft = owner
            .config_profiles
            .get(config)
            .ok_or(EditError::NoSuchConfig { profile, config })?
            .clone();
        let primary = draft.is_primary();
        apply_set_edit(&mut draft, edit, primary)?;
        // A set saved twice before this check existed can still be renamed or have its buttons edited:
        // only a changed condition has to be unique.
        let before = &owner.config_profiles[config];
        let unchanged = before.context_match.trim().eq_ignore_ascii_case(draft.context_match.trim())
            && before.panel_id.eq_ignore_ascii_case(&draft.panel_id)
            && before.normalized_when() == draft.normalized_when();
        if !unchanged {
            refuse_duplicate(&owner.config_profiles, Some(config), &draft)?;
        }
        owner.config_profiles[config] = draft;
        Ok(())
    }

    /// Remove a set and return it. The main set, and the last set, cannot be removed.
    pub fn delete_config(&mut self, profile: usize, config: usize) -> Result<ConfigProfile, EditError> {
        let owner = self.profiles.get_mut(profile).ok_or(EditError::NoSuchProfile(profile))?;
        let target = owner
            .config_profiles
            .get(config)
            .ok_or(EditError::NoSuchConfig { profile, config })?;
        if target.is_primary() {
            return invalid("The main set of buttons cannot be deleted.");
        }
        if owner.config_profiles.len() <= 1 {
            return invalid("A profile needs at least one set of buttons.");
        }
        Ok(owner.config_profiles.remove(config))
    }
}

/// A second set that would apply to exactly what another set already applies to is never wanted.
fn refuse_duplicate(sets: &[ConfigProfile], skip: Option<usize>, new: &ConfigProfile) -> Result<(), EditError> {
    let same = |o: &ConfigProfile| {
        o.context_match.trim().eq_ignore_ascii_case(new.context_match.trim())
            && o.panel_id.eq_ignore_ascii_case(&new.panel_id)
            && o.normalized_when() == new.normalized_when()
    };
    match sets.iter().enumerate().find(|(i, o)| Some(*i) != skip && same(o)) {
        Some((_, o)) => invalid(format!("“{}” already applies to this. Edit that set instead.", o.name)),
        None => Ok(()),
    }
}

/// Apply an edit to a set. `primary` is the main set, which keeps `*` and no condition.
fn apply_set_edit(config: &mut ConfigProfile, edit: SetEdit, primary: bool) -> Result<(), EditError> {
    let name = edit.name.trim();
    if name.is_empty() {
        return invalid("A set of buttons needs a name.");
    }
    check_length(name, MAX_SET_NAME, "A set name")?;
    check_length(edit.context_match.trim(), MAX_MATCH, "A window title or address")?;
    config.label_mode = match edit.label_mode.trim().to_lowercase().as_str() {
        "" | "inherit" | "follow" => "",
        "show" => "show",
        "hide" => "hide",
        other => return invalid(format!("Overlay must be follow, show or hide, not '{other}'.")),
    }
    .to_string();

    if primary {
        config.name = name.to_string();
        config.context_match = "*".into();
        config.when = String::new();
        return Ok(());
    }

    let when = ConfigProfile { when: edit.when.clone(), ..ConfigProfile::default() }.normalized_when();
    if !WHEN_OPTIONS.iter().any(|(value, _)| *value == when) {
        return invalid("Pick when this set applies: always, video playing, a YouTube Short, or video ended.");
    }

    let panel_id = edit.panel_id.trim();
    let panel_label = if panel_id.is_empty() {
        String::new()
    } else if let Some((_, label)) = PANEL_PRESETS.iter().find(|(id, _)| id.eq_ignore_ascii_case(panel_id)) {
        (*label).to_string()
    } else if panel_id == config.panel_id {
        config.panel_label.clone() // a panel the C# app detected: kept as it was
    } else {
        return invalid(format!("'{panel_id}' is not a panel this window knows."));
    };

    // A secondary set never matches "*": that would make it a second main set.
    let context_match = match edit.context_match.trim() {
        "*" => "",
        other => other,
    };
    if context_match.is_empty() && panel_id.is_empty() && when.is_empty() {
        return invalid(
            "Say when this set applies: part of the window title or address, a panel, or a YouTube state.",
        );
    }

    config.name = name.to_string();
    config.context_match = context_match.to_string();
    config.when = when;
    config.panel_id = panel_id.to_string();
    config.panel_label = panel_label;
    Ok(())
}

impl Button {
    /// A button with nothing bound, as the C# app creates a new profile's buttons.
    pub fn blank() -> Button {
        Button { kind: ActionKind::Shortcut.as_str().into(), steps: json!([]), ..Button::default() }
    }
}

/// What an application profile matches: the process name, so `C:\Apps\Tool.exe` matches `Tool`.
fn application_match(target: &str) -> String {
    let name = target.rsplit(['\\', '/']).next().unwrap_or(target);
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".exe") { name[..name.len() - 4].to_string() } else { target.trim().to_string() }
}

impl Settings {
    /// Add a profile at the end and return its index. `target` is what it applies to: a process
    /// name or `.exe` path for an app, a site name or address for a website, or nothing for a
    /// profile that applies to everything. `name` defaults to the target (or `Profile N`). The
    /// profile gets one main set of three blank buttons, as in the C# app.
    pub fn add_profile(&mut self, name: &str, target: &str, kind: &str) -> Result<usize, EditError> {
        let target = target.trim();
        let (context_match, kind) = if target.is_empty() {
            ("*".to_string(), String::new())
        } else {
            let kind = match kind.trim().to_lowercase().as_str() {
                "" => infer_kind(Some(target)).to_string(),
                k @ (APP_KIND | WEBSITE_KIND) => k.to_string(),
                other => return invalid(format!("A profile is for an app or a website, not '{other}'.")),
            };
            if kind == WEBSITE_KIND {
                (website_match(target), kind)
            } else {
                (application_match(target), APP_KIND.to_string())
            }
        };

        let name = match name.trim() {
            "" if target.is_empty() => format!("Profile {}", self.profiles.len() + 1),
            "" => context_match.clone(),
            given => given.to_string(),
        };
        if name.is_empty() {
            return invalid("A profile needs a name.");
        }
        check_length(&name, MAX_PROFILE_NAME, "A profile name")?;
        check_length(&context_match, MAX_MATCH, "What a profile applies to")?;

        self.profiles.push(Profile {
            name: name.clone(),
            context_match,
            kind,
            config_profiles: vec![ConfigProfile {
                name,
                context_match: "*".into(),
                buttons: vec![Button::blank(), Button::blank(), Button::blank()],
                ..ConfigProfile::default()
            }],
            ..Profile::default()
        });
        Ok(self.profiles.len() - 1)
    }

    /// Remove a profile and return it (the caller cleans up its icon file). At least one profile
    /// must remain.
    pub fn delete_profile(&mut self, index: usize) -> Result<Profile, EditError> {
        if index >= self.profiles.len() {
            return Err(EditError::NoSuchProfile(index));
        }
        if self.profiles.len() <= 1 {
            return invalid("At least one profile must remain.");
        }
        Ok(self.profiles.remove(index))
    }

    /// Move a profile to a new position. Order matters: the first profile that matches wins.
    pub fn move_profile(&mut self, from: usize, to: usize) -> Result<(), EditError> {
        let count = self.profiles.len();
        if from >= count {
            return Err(EditError::NoSuchProfile(from));
        }
        if to >= count {
            return Err(EditError::NoSuchProfile(to));
        }
        let moved = self.profiles.remove(from);
        self.profiles.insert(to, moved);
        Ok(())
    }
}

impl Button {
    /// The edit that would leave this button as it is: the starting point for a form.
    pub fn to_edit(&self) -> ButtonEdit {
        ButtonEdit {
            kind: self.kind.clone(),
            value: self.value.clone(),
            label: self.label.clone(),
            steps: self.automation_steps(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ConfigProfile, Profile};

    fn settings() -> Settings {
        let buttons = (0..3).map(|i| Button { kind: "Shortcut".into(), value: format!("F{}", i + 1), label: format!("B{i}"), steps: json!(""), ..Default::default() }).collect();
        Settings {
            profiles: vec![
                Profile {
                    name: "Default".into(),
                    context_match: "*".into(),
                    config_profiles: vec![ConfigProfile { name: "Default".into(), context_match: "*".into(), buttons, ..Default::default() }],
                    ..Default::default()
                },
                Profile {
                    name: "Chrome".into(),
                    context_match: "chrome".into(),
                    kind: "app".into(),
                    config_profiles: vec![ConfigProfile { name: "Chrome".into(), context_match: "*".into(), buttons: vec![Button::default(); 3], ..Default::default() }],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn edit(kind: &str, value: &str) -> ButtonEdit {
        ButtonEdit { kind: kind.into(), value: value.into(), label: " Go ".into(), steps: vec![] }
    }

    fn step(kind: &str, value: &str) -> ActionStep {
        ActionStep { kind: kind.into(), value: value.into(), ..Default::default() }
    }

    #[test]
    fn a_valid_edit_changes_only_that_button_and_trims_the_label() {
        let mut s = settings();
        s.set_button(0, 0, 1, edit("Shortcut", "Ctrl+Shift+Z")).unwrap();
        let buttons = &s.profiles[0].config_profiles[0].buttons;
        assert_eq!((buttons[1].kind.as_str(), buttons[1].value.as_str(), buttons[1].label.as_str()), ("Shortcut", "Ctrl+Shift+Z", "GO"));
        assert_eq!(buttons[0].value, "F1");
        assert_eq!(buttons[2].value, "F3");
    }

    #[test]
    fn fields_we_do_not_model_survive_an_edit() {
        let mut s = settings();
        s.profiles[0].config_profiles[0].buttons[0].extra.insert("FutureField".into(), json!({ "a": 1 }));
        s.set_button(0, 0, 0, edit("LaunchApp", r"C:\Apps\tool.exe")).unwrap();
        assert_eq!(s.profiles[0].config_profiles[0].buttons[0].extra["FutureField"], json!({ "a": 1 }));
    }

    #[test]
    fn bad_values_are_refused_with_a_reason_and_change_nothing() {
        let mut s = settings();
        let before = s.clone();
        for (kind, value, needle) in [
            ("Shortcut", "Ctrl+Nope", "Unsupported key"),
            ("OpenUrl", "javascript:alert(1)", "cannot be opened"),
            ("MouseClick", "left side", "x,y"),
            ("Teleport", "x", "Unknown action type"),
        ] {
            let err = s.set_button(0, 0, 0, edit(kind, value)).unwrap_err();
            assert!(matches!(&err, EditError::Invalid(m) if m.contains(needle)), "{kind}: {err}");
        }
        assert_eq!(s, before);
    }

    #[test]
    fn a_blank_value_means_nothing_is_bound_and_is_allowed() {
        let mut s = settings();
        for kind in ["Shortcut", "OpenUrl", "MouseClick", "LaunchApp"] {
            s.set_button(0, 0, 0, edit(kind, "  ")).unwrap();
        }
    }

    #[test]
    fn a_missing_profile_config_or_button_is_an_error_not_a_panic() {
        let mut s = settings();
        assert_eq!(s.set_button(9, 0, 0, edit("Shortcut", "A")), Err(EditError::NoSuchProfile(9)));
        assert!(matches!(s.set_button(0, 5, 0, edit("Shortcut", "A")), Err(EditError::NoSuchConfig { .. })));
        assert!(matches!(s.set_button(0, 0, 7, edit("Shortcut", "A")), Err(EditError::NoSuchButton { .. })));
    }

    #[test]
    fn an_automation_keeps_its_steps_and_drops_the_plain_value() {
        let mut s = settings();
        let mut e = edit("Automation", "ignored");
        e.steps = vec![step("Shortcut", "Win+D"), step("Delay", "250"), step("OpenUrl", "example.com")];
        s.set_button(0, 0, 0, e).unwrap();
        let b = &s.profiles[0].config_profiles[0].buttons[0];
        assert_eq!(b.value, "");
        assert_eq!(b.to_action().steps, [step("Shortcut", "Win+D"), step("Delay", "250"), step("OpenUrl", "example.com")]);
    }

    #[test]
    fn a_bad_step_names_its_number() {
        let mut s = settings();
        let mut e = edit("Automation", "");
        e.steps = vec![step("Shortcut", "A"), step("Delay", "soon")];
        let err = s.set_button(0, 0, 0, e).unwrap_err();
        assert!(matches!(&err, EditError::Invalid(m) if m.starts_with("Step 2:")), "{err}");

        let mut e = edit("Automation", "");
        e.steps = vec![step("Shortcut", "  ")];
        assert!(matches!(s.set_button(0, 0, 0, e), Err(EditError::Invalid(m)) if m.contains("needs a value")));
    }

    #[test]
    fn leaving_automation_clears_the_steps() {
        let mut s = settings();
        let mut e = edit("Automation", "");
        e.steps = vec![step("Delay", "5")];
        s.set_button(0, 0, 0, e).unwrap();
        s.set_button(0, 0, 0, edit("Shortcut", "A")).unwrap();
        assert!(s.profiles[0].config_profiles[0].buttons[0].automation_steps().is_empty());
    }

    #[test]
    fn a_plain_button_keeps_the_steps_form_it_already_had() {
        let mut s = settings();
        s.set_button(0, 0, 0, edit("Shortcut", "A")).unwrap();
        assert_eq!(s.profiles[0].config_profiles[0].buttons[0].steps, json!(""));
    }

    #[test]
    fn a_form_round_trips_through_to_edit() {
        let s = settings();
        let b = &s.profiles[0].config_profiles[0].buttons[2];
        let mut again = s.clone();
        again.set_button(0, 0, 2, b.to_edit()).unwrap();
        assert_eq!(again, s);
    }

    #[test]
    fn renaming_a_profile_also_renames_its_main_configuration() {
        let mut s = settings();
        s.update_profile(1, "  Browser  ", "chrome", "app").unwrap();
        assert_eq!(s.profiles[1].name, "Browser");
        assert_eq!(s.profiles[1].config_profiles[0].name, "Browser");
    }

    #[test]
    fn a_configuration_with_its_own_name_is_not_renamed() {
        let mut s = settings();
        s.profiles[1].config_profiles[0].name = "Custom".into();
        s.update_profile(1, "Browser", "chrome", "app").unwrap();
        assert_eq!(s.profiles[1].config_profiles[0].name, "Custom");
    }

    #[test]
    fn the_kind_follows_what_the_profile_matches() {
        let mut s = settings();
        s.update_profile(1, "Netflix", "netflix.com", "").unwrap();
        assert_eq!((s.profiles[1].context_match.as_str(), s.profiles[1].kind.as_str()), ("netflix.com", "website"));
        s.update_profile(1, "Code", "Code", "").unwrap();
        assert_eq!(s.profiles[1].kind, "app");
        s.update_profile(1, "Claude", "Claude", "APP").unwrap();
        assert_eq!(s.profiles[1].kind, "app");
        // Matching everything has no kind, whatever was asked.
        s.update_profile(1, "Everything", " ", "website").unwrap();
        assert_eq!((s.profiles[1].context_match.as_str(), s.profiles[1].kind.as_str()), ("*", ""));
    }

    #[test]
    fn a_profile_needs_a_name_and_a_known_kind() {
        let mut s = settings();
        assert!(matches!(s.update_profile(1, "  ", "x", "app"), Err(EditError::Invalid(_))));
        assert!(matches!(s.update_profile(1, "N", "x", "device"), Err(EditError::Invalid(_))));
        assert_eq!(s.update_profile(8, "N", "x", "app"), Err(EditError::NoSuchProfile(8)));
    }

    #[test]
    fn a_new_website_profile_gets_the_host_a_name_and_three_blank_buttons() {
        let mut s = settings();
        let i = s.add_profile("", "https://www.netflix.com/browse", "website").unwrap();
        assert_eq!(i, 2);
        let p = &s.profiles[2];
        assert_eq!((p.name.as_str(), p.context_match.as_str(), p.kind.as_str()), ("netflix.com", "netflix.com", "website"));
        assert_eq!(p.config_profiles.len(), 1);
        let main = &p.config_profiles[0];
        assert_eq!((main.name.as_str(), main.context_match.as_str()), ("netflix.com", "*"));
        assert_eq!(main.buttons.len(), 3);
        assert!(main.buttons.iter().all(|b| b.kind == "Shortcut" && b.value.is_empty() && b.steps == json!([])));
        assert_eq!(p.primary_config_index(), Some(0));
    }

    #[test]
    fn a_site_name_is_looked_up_in_the_catalog() {
        let mut s = settings();
        s.add_profile("Mail", "proton mail", "website").unwrap();
        assert_eq!(s.profiles[2].context_match, "mail.proton.me");
        assert_eq!(s.profiles[2].name, "Mail");
    }

    #[test]
    fn a_new_app_profile_matches_the_process_name() {
        let mut s = settings();
        s.add_profile("", r"C:\Program Files\Tool\Tool.EXE", "app").unwrap();
        s.add_profile("Notes", "notepad", "app").unwrap();
        assert_eq!((s.profiles[2].name.as_str(), s.profiles[2].context_match.as_str(), s.profiles[2].kind.as_str()), ("Tool", "Tool", "app"));
        assert_eq!(s.profiles[3].context_match, "notepad");
    }

    #[test]
    fn a_profile_with_no_target_applies_to_everything_and_is_named_by_its_number() {
        let mut s = settings();
        s.add_profile("", "  ", "").unwrap();
        let p = &s.profiles[2];
        assert_eq!((p.name.as_str(), p.context_match.as_str(), p.kind.as_str()), ("Profile 3", "*", ""));
    }

    #[test]
    fn the_kind_is_guessed_when_not_given_and_unknown_kinds_are_refused() {
        let mut s = settings();
        s.add_profile("", "netflix.com", "").unwrap();
        s.add_profile("", "Code", "").unwrap();
        assert_eq!((s.profiles[2].kind.as_str(), s.profiles[3].kind.as_str()), ("website", "app"));
        assert!(matches!(s.add_profile("x", "y", "device"), Err(EditError::Invalid(_))));
        assert_eq!(s.profiles.len(), 4, "a refused add changes nothing");
    }

    #[test]
    fn deleting_returns_the_profile_and_the_last_one_cannot_be_deleted() {
        let mut s = settings();
        let removed = s.delete_profile(1).unwrap();
        assert_eq!(removed.name, "Chrome");
        assert_eq!(s.profiles.len(), 1);
        assert!(matches!(s.delete_profile(0), Err(EditError::Invalid(m)) if m.contains("At least one")));
        assert_eq!(s.delete_profile(5), Err(EditError::NoSuchProfile(5)));
        assert_eq!(s.profiles.len(), 1);
    }

    #[test]
    fn moving_reorders_and_a_bad_position_is_an_error() {
        let mut s = settings();
        s.add_profile("Third", "x", "app").unwrap();
        s.move_profile(2, 0).unwrap();
        let names: Vec<&str> = s.profiles.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Third", "Default", "Chrome"]);
        s.move_profile(1, 1).unwrap();
        assert_eq!(s.move_profile(0, 9), Err(EditError::NoSuchProfile(9)));
        assert_eq!(s.move_profile(9, 0), Err(EditError::NoSuchProfile(9)));
        assert_eq!(s.profiles[0].name, "Third");
    }

    #[test]
    fn a_new_profile_round_trips_through_json_in_the_shape_the_csharp_app_writes() {
        let mut s = settings();
        s.add_profile("Notes", "notepad", "app").unwrap();
        let text = crate::to_json(&s);
        let back = crate::load(&text).unwrap().settings;
        assert_eq!(back.profiles[2], s.profiles[2]);
        let value: Value = serde_json::from_str(&text).unwrap();
        let config = &value["Profiles"][2]["ConfigProfiles"][0];
        assert_eq!(config["Buttons"][0], json!({ "Type": "Shortcut", "Value": "", "Label": "", "Steps": [] }));
        assert_eq!(config["ContextMatch"], "*");
    }

    #[test]
    fn a_label_is_shown_in_capitals_and_fitted_to_the_pad() {
        let mut s = settings();
        let mut e = edit("Shortcut", "A");
        e.label = "documentation setup for the project".into();
        s.set_button(0, 0, 0, e.clone()).unwrap();
        assert_eq!(s.profiles[0].config_profiles[0].buttons[0].label, "DOCS PROJECT");
        e.label = "   ".into();
        s.set_button(0, 0, 0, e).unwrap();
        assert_eq!(s.profiles[0].config_profiles[0].buttons[0].label, "", "blank stays blank, not a dash");
    }

    #[test]
    fn an_automation_needs_at_least_one_step() {
        let mut s = settings();
        let err = s.set_button(0, 0, 0, edit("Automation", "")).unwrap_err();
        assert!(matches!(&err, EditError::Invalid(m) if m.contains("at least one step")), "{err}");
    }

    #[test]
    fn the_editor_step_types_are_offered_and_others_cannot_be_created() {
        assert_eq!(STEP_TYPES[0], "QuickLaunch", "most used first");
        assert!(!STEP_TYPES.contains(&"Wait for Release") && !WRITABLE_STEP_TYPES.contains(&"WaitRelease"));
        let mut s = settings();
        let mut e = edit("Automation", "");
        e.steps = vec![step("FocusWindow", "Notepad")];
        let err = s.set_button(0, 0, 0, e).unwrap_err();
        assert!(matches!(&err, EditError::Invalid(m) if m.contains("cannot be created")), "{err}");
    }

    #[test]
    fn a_step_of_a_kind_the_editor_cannot_make_survives_if_left_as_it_is() {
        let mut s = settings();
        s.profiles[0].config_profiles[0].buttons[0] = Button {
            kind: "Automation".into(),
            steps: json!([{ "Type": "FocusWindow", "Value": "Notepad" }, { "Type": "Delay", "Value": "5" }]),
            ..Default::default()
        };
        // Edit only the delay: the focus step is passed back unchanged and is kept.
        let mut e = s.profiles[0].config_profiles[0].buttons[0].to_edit();
        e.steps[1].value = "250".into();
        s.set_button(0, 0, 0, e.clone()).unwrap();
        assert_eq!(
            s.profiles[0].config_profiles[0].buttons[0].automation_steps(),
            [step("FocusWindow", "Notepad"), step("Delay", "250")]
        );
        // Changing it is not allowed.
        e.steps[0].value = "Calculator".into();
        assert!(s.set_button(0, 0, 0, e).is_err());
    }

    #[test]
    fn links_and_builder_places_are_saved_and_read_back() {
        let mut s = settings();
        let mut e = edit("Automation", "");
        e.steps = vec![
            ActionStep { on_ok: Next::Stop, on_fail: Next::Step(1), pos: Some((40, 80)), ..step("LaunchApp", "notepad") },
            step("Shortcut", "A"),
        ];
        s.set_button(0, 0, 0, e.clone()).unwrap();
        let button = &s.profiles[0].config_profiles[0].buttons[0];
        assert_eq!(button.steps[0], json!({ "Type": "LaunchApp", "Value": "notepad", "Next": -1, "OnFail": 1, "X": 40, "Y": 80 }));
        assert_eq!(button.steps[1], json!({ "Type": "Shortcut", "Value": "A" }), "a plain step keeps the C# shape");
        assert_eq!(button.automation_steps(), e.steps);

        e.steps[1].on_ok = Next::Step(5);
        assert!(s.set_button(0, 0, 0, e).is_err(), "joined to a step that is not there");
    }

    #[test]
    fn a_new_wait_for_release_step_cannot_be_made() {
        let mut s = settings();
        let mut e = edit("Automation", "");
        e.steps = vec![step("Shortcut", "Ctrl+C"), step("Wait for Release", "")];
        assert!(s.set_button(0, 0, 0, e).is_err());
    }

    #[test]
    fn a_saved_automation_has_the_json_shape_the_csharp_app_reads() {
        let mut s = settings();
        let mut e = edit("Automation", "");
        e.steps = vec![step("Delay", "250"), step("Shortcut", "Win+D")];
        s.set_button(0, 0, 0, e).unwrap();
        let value: Value = serde_json::from_str(&crate::to_json(&s)).unwrap();
        let button = &value["Profiles"][0]["ConfigProfiles"][0]["Buttons"][0];
        assert_eq!(button["Type"], "Automation");
        assert_eq!(button["Value"], "");
        assert_eq!(button["Steps"], json!([{ "Type": "Delay", "Value": "250" }, { "Type": "Shortcut", "Value": "Win+D" }]));
    }

    fn set(name: &str, context_match: &str, when: &str, panel: &str) -> SetEdit {
        SetEdit { name: name.into(), context_match: context_match.into(), when: when.into(), panel_id: panel.into(), ..SetEdit::default() }
    }

    #[test]
    fn a_duplicate_set_is_refused() {
        let mut s = Settings::default();
        let p = s.add_profile("Code", "code", "app").unwrap();
        s.add_config(p, set("Term", "", "", "terminal")).unwrap();
        assert!(s.add_config(p, set("Term 2", "", "", "TERMINAL")).is_err());
        let other = s.add_config(p, set("Docs", "docs", "", "")).unwrap();
        assert!(s.update_config(p, other, set("Docs", "docs", "", "")).is_ok());
        assert!(s.update_config(p, other, set("Docs", "", "", "terminal")).is_err());
        // an old duplicate (same condition already stored) can still be renamed
        s.profiles[p].config_profiles[other].context_match = String::new();
        s.profiles[p].config_profiles[other].panel_id = "terminal".into();
        assert!(s.update_config(p, other, set("Docs 2", "", "", "terminal")).is_ok());
    }

    #[test]
    fn a_profiles_picture_icon_path_is_set_and_cleared() {
        let mut s = settings();
        s.set_profile_icon_path(0, " profile_icons/1.png ").unwrap();
        assert_eq!(s.profiles[0].icon_path, "profile_icons/1.png");
        s.set_profile_icon_path(0, "").unwrap();
        assert_eq!(s.profiles[0].icon_path, "");
        assert!(matches!(s.set_profile_icon_path(99, "x"), Err(EditError::NoSuchProfile(99))));
    }

    #[test]
    fn a_profiles_label_behaviour_and_emoji_are_saved_and_checked() {
        let mut s = settings();
        s.set_profile_look(0, "Show", " 🎬 ").unwrap();
        assert_eq!((s.profiles[0].label_mode.as_str(), s.profiles[0].icon_emoji.as_str()), ("show", "🎬"));
        s.set_profile_look(0, "", "").unwrap();
        assert_eq!((s.profiles[0].label_mode.as_str(), s.profiles[0].icon_emoji.as_str()), ("", ""));
        let before = s.clone();
        assert!(matches!(s.set_profile_look(0, "sometimes", "x"), Err(EditError::Invalid(_))));
        assert!(matches!(s.set_profile_look(0, "", "123456789"), Err(EditError::Invalid(_))));
        assert!(matches!(s.set_profile_look(0, "", "a\u{7}"), Err(EditError::Invalid(_))));
        assert!(matches!(s.set_profile_look(99, "", ""), Err(EditError::NoSuchProfile(99))));
        assert_eq!(s, before, "a refusal changes nothing");
    }

    #[test]
    fn the_overlay_position_is_saved_and_cannot_be_negative() {
        let mut s = settings();
        s.set_overlay_position(0, 717).unwrap();
        assert_eq!(crate::overlay::overlay_position(&s), Some((0.0, 717.0)));
        assert!(matches!(s.set_overlay_position(-1, 5), Err(EditError::Invalid(_))));
        assert_eq!(crate::overlay::overlay_position(&s), Some((0.0, 717.0)), "a refusal changes nothing");
    }

    #[test]
    fn a_new_set_has_three_blank_buttons_and_turns_extra_sets_on() {
        let mut s = settings();
        assert!(!s.profiles[1].multi_config_enabled);
        let i = s.add_config(1, set("  Docs  ", "docs", "", "")).unwrap();
        assert_eq!(i, 1);
        let c = &s.profiles[1].config_profiles[1];
        assert_eq!((c.name.as_str(), c.context_match.as_str(), c.when.as_str()), ("Docs", "docs", ""));
        assert_eq!(c.buttons.len(), 3);
        assert!(c.buttons.iter().all(|b| b.kind == "Shortcut" && b.value.is_empty()));
        assert!(s.profiles[1].multi_config_enabled);
        assert!(!c.is_primary());
    }

    #[test]
    fn a_set_can_apply_to_a_panel_or_a_youtube_state_instead_of_a_title() {
        let mut s = settings();
        s.add_config(1, set("Terminal", "", "", "terminal")).unwrap();
        s.add_config(1, set("Playing", "", "Playing", "")).unwrap();
        let configs = &s.profiles[1].config_profiles;
        assert_eq!((configs[1].panel_id.as_str(), configs[1].panel_label.as_str()), ("terminal", "Integrated Terminal"));
        assert_eq!(configs[2].when, "youtube.playing", "older spellings are stored in the canonical form");
    }

    #[test]
    fn a_set_with_no_condition_is_refused_because_it_would_never_apply() {
        let mut s = settings();
        for blank in [set("Lonely", "", "", ""), set("Lonely", "*", "always", "  ")] {
            let err = s.add_config(1, blank).unwrap_err();
            assert!(matches!(&err, EditError::Invalid(m) if m.contains("Say when")), "{err}");
        }
        assert_eq!(s.profiles[1].config_profiles.len(), 1);
    }

    #[test]
    fn a_secondary_set_can_never_match_everything() {
        let mut s = settings();
        let i = s.add_config(1, set("Docs", "*", "youtube.ended", "")).unwrap();
        assert_eq!(s.profiles[1].config_profiles[i].context_match, "");
    }

    #[test]
    fn names_when_values_and_panels_are_checked() {
        let mut s = settings();
        assert!(matches!(s.add_config(1, set(" ", "x", "", "")), Err(EditError::Invalid(m)) if m.contains("name")));
        assert!(matches!(s.add_config(1, set("A", "x", "sometimes", "")), Err(EditError::Invalid(m)) if m.contains("Pick when")));
        assert!(matches!(s.add_config(1, set("A", "x", "", "lava")), Err(EditError::Invalid(m)) if m.contains("not a panel")));
        assert_eq!(s.profiles[1].config_profiles.len(), 1, "refusals change nothing");
    }

    #[test]
    fn names_and_conditions_have_a_length_limit_counted_in_characters() {
        let mut s = settings();
        let long_set = "é".repeat(MAX_SET_NAME + 1);
        assert!(matches!(s.add_config(1, set(&long_set, "x", "", "")), Err(EditError::Invalid(m)) if m.contains("at most 24")));
        assert!(s.add_config(1, set(&"é".repeat(MAX_SET_NAME), "x", "", "")).is_ok(), "exactly the limit is fine, accents count once");
        assert!(matches!(s.add_config(1, set("A", &"w".repeat(MAX_MATCH + 1), "", "")), Err(EditError::Invalid(_))));
        assert!(matches!(s.update_profile(1, &"n".repeat(MAX_PROFILE_NAME + 1), "x", "app"), Err(EditError::Invalid(m)) if m.contains("at most 40")));
        assert!(matches!(s.add_profile(&"n".repeat(MAX_PROFILE_NAME + 1), "notepad", "app"), Err(EditError::Invalid(_))));
        assert!(s.update_profile(1, &"n".repeat(MAX_PROFILE_NAME), "x", "app").is_ok());
    }

    #[test]
    fn a_profile_has_at_most_six_sets() {
        let mut s = settings();
        for n in 0..5 {
            s.add_config(1, set(&format!("S{n}"), &format!("m{n}"), "", "")).unwrap();
        }
        assert_eq!(s.profiles[1].config_profiles.len(), 6);
        assert!(matches!(s.add_config(1, set("S6", "m6", "", "")), Err(EditError::Invalid(m)) if m.contains("up to 6")));
    }

    #[test]
    fn the_main_set_keeps_star_and_no_condition_whatever_is_asked() {
        let mut s = settings();
        s.update_config(0, 0, set("Home", "docs", "youtube.playing", "terminal")).unwrap();
        let main = &s.profiles[0].config_profiles[0];
        assert_eq!((main.name.as_str(), main.context_match.as_str(), main.when.as_str(), main.panel_id.as_str()), ("Home", "*", "", ""));
    }

    #[test]
    fn updating_a_set_keeps_its_buttons_and_the_hints_this_window_cannot_set() {
        let mut s = settings();
        s.add_config(1, set("Docs", "docs", "", "")).unwrap();
        {
            let c = &mut s.profiles[1].config_profiles[1];
            c.focus_class_name = "Chrome_WidgetWin".into();
            c.ancestor_hint = "toolbar".into();
            c.buttons[0].value = "F5".into();
        }
        s.update_config(1, 1, set("Documents", "doc", "", "")).unwrap();
        let c = &s.profiles[1].config_profiles[1];
        assert_eq!((c.name.as_str(), c.context_match.as_str()), ("Documents", "doc"));
        assert_eq!((c.focus_class_name.as_str(), c.ancestor_hint.as_str(), c.buttons[0].value.as_str()), ("Chrome_WidgetWin", "toolbar", "F5"));
    }

    #[test]
    fn a_panel_the_csharp_app_detected_is_kept_when_the_set_is_edited() {
        let mut s = settings();
        s.add_config(1, set("Odd", "x", "", "")).unwrap();
        {
            let c = &mut s.profiles[1].config_profiles[1];
            c.panel_id = "problems".into();
            c.panel_label = "Problems".into();
        }
        s.update_config(1, 1, set("Odd", "", "", "problems")).unwrap();
        assert_eq!(s.profiles[1].config_profiles[1].panel_label, "Problems");
        assert!(s.update_config(1, 1, set("Odd", "", "", "another")).is_err());
    }

    #[test]
    fn deleting_a_set_returns_it_and_the_main_and_last_sets_stay() {
        let mut s = settings();
        s.add_config(1, set("Docs", "docs", "", "")).unwrap();
        let removed = s.delete_config(1, 1).unwrap();
        assert_eq!(removed.name, "Docs");
        assert_eq!(s.profiles[1].config_profiles.len(), 1);
        assert!(matches!(s.delete_config(1, 0), Err(EditError::Invalid(m)) if m.contains("main set")));
        assert_eq!(s.delete_config(1, 4), Err(EditError::NoSuchConfig { profile: 1, config: 4 }));
        assert_eq!(s.delete_config(9, 0), Err(EditError::NoSuchProfile(9)));
    }

    #[test]
    fn the_last_set_cannot_go_even_if_it_is_not_the_main_one() {
        let mut s = settings();
        s.profiles[1].config_profiles = vec![ConfigProfile { name: "Only".into(), context_match: "x".into(), ..Default::default() }];
        assert!(matches!(s.delete_config(1, 0), Err(EditError::Invalid(m)) if m.contains("at least one")));
    }

    #[test]
    fn turning_extra_sets_off_keeps_them() {
        let mut s = settings();
        s.add_config(1, set("Docs", "docs", "", "")).unwrap();
        s.set_multi_config(1, false).unwrap();
        assert!(!s.profiles[1].multi_config_enabled);
        assert_eq!(s.profiles[1].config_profiles.len(), 2);
        assert_eq!(s.set_multi_config(7, true), Err(EditError::NoSuchProfile(7)));
    }

    #[test]
    fn a_set_the_matcher_can_use_after_adding_one() {
        use crate::matcher::matching_config;
        use oasis_core::matching::ForegroundContext;
        let mut s = settings();
        s.add_config(1, set("Docs", "docs", "", "editor")).unwrap();
        let mut ctx = ForegroundContext::new("chrome", "my docs", "");
        ctx.panel_id = "editor".into();
        ctx.detection_confidence = "high".into();
        assert_eq!(matching_config(&s.profiles[1], &ctx, ""), Some(1));
        s.set_multi_config(1, false).unwrap();
        assert_eq!(matching_config(&s.profiles[1], &ctx, ""), Some(0), "off: only the main set");
    }

    #[test]
    fn every_listed_action_type_is_accepted_with_a_blank_value() {
        for kind in ACTION_TYPES {
            assert_eq!(validate_action(kind, ""), Ok(()), "{kind}");
        }
    }

    #[test]
    fn a_quick_launch_step_must_name_a_setup_that_exists() {
        let mut s = settings();
        let id = s
            .save_setup(crate::quick_launch::SetupEdit {
                id: String::new(),
                name: "Project Mode".into(),
                call_name: String::new(),
                start_with_oasis: false,
                items: vec![crate::quick_launch::ItemEdit { kind: "app".into(), target: "code".into(), open_with: String::new(), screen: String::new(), layout: 0, region: None, open_another: false }],
            })
            .unwrap();
        let mut e = edit("Automation", "");
        e.steps = vec![step("QuickLaunch", &id)];
        assert!(s.set_button(0, 0, 0, e).is_ok());
        let mut e = edit("Automation", "");
        e.steps = vec![step("QuickLaunch", "nope")];
        assert!(matches!(s.set_button(0, 0, 0, e).unwrap_err(), EditError::Invalid(m) if m.contains("does not exist")));
        let mut e = edit("Automation", "");
        e.steps = vec![step("QuickLaunch", " ")];
        assert!(matches!(s.set_button(0, 0, 0, e).unwrap_err(), EditError::Invalid(m) if m.contains("pick the Quick Launch")));
    }
}
