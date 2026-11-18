//! One button: what it does (an action, or the steps of an automation), checked before it is saved.

use super::*;

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
pub(super) fn step_json(step: &ActionStep) -> Value {
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
pub(super) fn validate_step(number: usize, step: &ActionStep, existing: &[ActionStep]) -> Result<(), String> {
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
}

impl Button {
    /// A button with nothing bound, as the C# app creates a new profile's buttons.
    pub fn blank() -> Button {
        Button { kind: ActionKind::Shortcut.as_str().into(), steps: json!([]), ..Button::default() }
    }

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
    use crate::edit::test_settings::*;

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
