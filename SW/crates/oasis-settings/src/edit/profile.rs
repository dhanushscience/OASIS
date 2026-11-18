//! Profiles: adding, renaming, re-targeting, deleting and moving one; its look and icon; the overlay position.

use super::*;

impl Settings {
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
}

/// What an application profile matches: the process name, so `C:\Apps\Tool.exe` matches `Tool`.
pub(super) fn application_match(target: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::test_settings::*;

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
}
