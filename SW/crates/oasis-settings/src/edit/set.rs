//! Sets of buttons inside a profile: adding, changing and deleting one, and when each applies.

use super::*;

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
pub(super) fn refuse_duplicate(sets: &[ConfigProfile], skip: Option<usize>, new: &ConfigProfile) -> Result<(), EditError> {
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
pub(super) fn apply_set_edit(config: &mut ConfigProfile, edit: SetEdit, primary: bool) -> Result<(), EditError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ConfigProfile;
    use crate::edit::test_settings::*;

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
}
