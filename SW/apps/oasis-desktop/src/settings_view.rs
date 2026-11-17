//! What the settings screen shows: a plain, serialisable view of the profiles. The page never
//! sees the settings types, only this, so a field the page does not need cannot leak into it.

use oasis_settings::edit::{
    ACTION_TYPES, MAX_SETS_PER_PROFILE, PANEL_PRESETS, STEP_TYPES, WHEN_OPTIONS,
    WRITABLE_STEP_TYPES,
};
use oasis_settings::Settings;
use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct StepView {
    pub kind: String,
    pub value: String,
    /// Where it goes once it worked / when it failed: null the plain-list rule, -1 stop, else a step.
    pub next: Option<i64>,
    pub on_fail: Option<i64>,
    /// Its place in the automation builder, if it was drawn there.
    pub x: Option<i32>,
    pub y: Option<i32>,
}

impl From<oasis_core::actions::ActionStep> for StepView {
    fn from(s: oasis_core::actions::ActionStep) -> StepView {
        StepView {
            kind: s.kind,
            value: s.value,
            next: s.on_ok.code(),
            on_fail: s.on_fail.code(),
            x: s.pos.map(|p| p.0),
            y: s.pos.map(|p| p.1),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ButtonView {
    pub kind: String,
    pub value: String,
    pub label: String,
    /// The steps of an `Automation` button; empty for any other.
    pub steps: Vec<StepView>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ConfigView {
    pub index: usize,
    pub name: String,
    pub context_match: String,
    /// The main set: always matches `*`, has no condition, and cannot be deleted.
    pub is_primary: bool,
    pub can_delete: bool,
    /// `""`, `youtube.playing`, `youtube.shorts` or `youtube.ended`.
    pub when: String,
    pub panel_id: String,
    pub panel_label: String,
    /// `""` follows the profile, `show` reveals the labels first, `hide` runs at once.
    pub label_mode: String,
    pub buttons: Vec<ButtonView>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Choice {
    pub value: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ProfileView {
    pub index: usize,
    pub name: String,
    pub context_match: String,
    pub kind: String,
    pub configs: Vec<ConfigView>,
    /// The config shown first: the one that applies when nothing more specific does.
    pub primary_config: usize,
    /// Whether the profile uses its extra sets of buttons (they are kept either way).
    pub multi: bool,
    /// `""` follows the general setting, `show` reveals the labels first, `hide` runs at once.
    pub label_mode: String,
    pub icon_emoji: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SettingsView {
    pub profiles: Vec<ProfileView>,
    /// The action types a button can be set to in this window.
    pub action_types: Vec<&'static str>,
    /// The step types the step menu offers.
    pub step_types: Vec<&'static str>,
    /// Every step type that can be edited. A step of another kind (an AI plan's "focus window")
    /// is shown but locked, and kept as it is.
    pub writable_step_types: Vec<&'static str>,
    /// Panels a set of buttons can be tied to.
    pub panel_presets: Vec<Choice>,
    /// When a set applies: always, or a YouTube state.
    pub when_options: Vec<Choice>,
    pub max_sets: usize,
    /// Which ways of pressing the three buttons are on.
    pub input: InputView,
}

/// The input methods, as the page shows them.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct InputView {
    /// The OASIS pad.
    pub hardware: bool,
    /// The three buttons from the keyboard.
    pub keyboard: bool,
    /// The shortcut for each button.
    pub keys: [String; 3],
    /// Keep a chosen shortcut from the program in front.
    pub exclusive: bool,
}

pub fn view(settings: &Settings) -> SettingsView {
    let profiles = settings
        .profiles
        .iter()
        .enumerate()
        .map(|(index, profile)| ProfileView {
            index,
            name: profile.name.clone(),
            context_match: profile.context_match.clone(),
            kind: profile.kind.clone(),
            primary_config: profile.primary_config_index().unwrap_or(0),
            multi: profile.multi_config_enabled,
            label_mode: oasis_core::input::LabelPolicy::normalize(Some(&profile.label_mode))
                .to_string(),
            icon_emoji: profile.icon_emoji.clone(),
            configs: profile
                .config_profiles
                .iter()
                .enumerate()
                .map(|(index, config)| ConfigView {
                    index,
                    name: config.name.clone(),
                    context_match: config.context_match.clone(),
                    is_primary: config.is_primary(),
                    can_delete: !config.is_primary() && profile.config_profiles.len() > 1,
                    when: config.normalized_when(),
                    panel_id: config.panel_id.clone(),
                    panel_label: config.panel_label.clone(),
                    label_mode: oasis_core::input::LabelPolicy::normalize(Some(&config.label_mode))
                        .to_string(),
                    buttons: config
                        .buttons
                        .iter()
                        .map(|b| ButtonView {
                            kind: b.kind.clone(),
                            value: b.value.clone(),
                            label: b.label.clone(),
                            steps: b.automation_steps().into_iter().map(StepView::from).collect(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    SettingsView {
        profiles,
        action_types: ACTION_TYPES.to_vec(),
        step_types: STEP_TYPES.to_vec(),
        writable_step_types: WRITABLE_STEP_TYPES.to_vec(),
        panel_presets: PANEL_PRESETS
            .iter()
            .map(|&(value, label)| Choice { value, label })
            .collect(),
        when_options: WHEN_OPTIONS
            .iter()
            .map(|&(value, label)| Choice { value, label })
            .collect(),
        max_sets: MAX_SETS_PER_PROFILE,
        input: {
            let input = oasis_settings::input::InputSettings::from(settings);
            InputView {
                hardware: input.hardware,
                keyboard: input.keyboard,
                keys: input.keys,
                exclusive: input.exclusive,
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oasis_settings::{Button, ConfigProfile, Profile};
    use serde_json::json;

    fn settings() -> Settings {
        let automation = Button {
            kind: "Automation".into(),
            steps: json!([{ "Type": "Delay", "Value": "5" }, { "Type": "Shortcut", "Value": "A" }]),
            ..Default::default()
        };
        let plain = Button {
            kind: "Shortcut".into(),
            value: "Win+D".into(),
            label: "DESKTOP".into(),
            ..Default::default()
        };
        Settings {
            profiles: vec![Profile {
                name: "Default".into(),
                context_match: "*".into(),
                config_profiles: vec![
                    ConfigProfile {
                        name: "Docs".into(),
                        context_match: "docs".into(),
                        ..Default::default()
                    },
                    ConfigProfile {
                        name: "Default".into(),
                        context_match: "*".into(),
                        buttons: vec![plain, automation],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn profiles_configs_and_buttons_are_listed_with_their_indexes() {
        let v = view(&settings());
        assert_eq!(v.profiles.len(), 1);
        let p = &v.profiles[0];
        assert_eq!(
            (p.index, p.name.as_str(), p.context_match.as_str()),
            (0, "Default", "*")
        );
        assert_eq!(p.configs.len(), 2);
        assert_eq!(p.configs[1].index, 1);
        assert_eq!(p.configs[1].buttons[0].value, "Win+D");
    }

    #[test]
    fn the_main_configuration_is_the_one_shown_first() {
        assert_eq!(view(&settings()).profiles[0].primary_config, 1);
    }

    #[test]
    fn an_automation_button_lists_its_steps_in_order() {
        let buttons = &view(&settings()).profiles[0].configs[1].buttons;
        assert!(buttons[0].steps.is_empty());
        assert_eq!(
            buttons[1].steps,
            [
                StepView { kind: "Delay".into(), value: "5".into(), next: None, on_fail: None, x: None, y: None },
                StepView { kind: "Shortcut".into(), value: "A".into(), next: None, on_fail: None, x: None, y: None },
            ]
        );
    }

    #[test]
    fn the_menus_offer_every_action_type_and_the_editor_step_types() {
        let v = view(&settings());
        assert!(v.action_types.contains(&"Shortcut") && v.action_types.contains(&"Automation"));
        assert_eq!(v.step_types.len(), 8);
        assert_eq!(v.step_types[0], "QuickLaunch");
        assert!(!v.step_types.contains(&"Wait for Release"));
        // More step types can be edited than the menu offers (a script step stays editable).
        assert!(
            v.writable_step_types.contains(&"RunScript") && !v.step_types.contains(&"RunScript")
        );
        assert!(!v.writable_step_types.contains(&"FocusWindow"));
    }

    #[test]
    fn each_set_says_whether_it_is_the_main_one_and_whether_it_can_go() {
        let configs = &view(&settings()).profiles[0].configs;
        assert!(!configs[0].is_primary && configs[0].can_delete);
        assert!(configs[1].is_primary && !configs[1].can_delete);
    }

    #[test]
    fn the_menus_for_a_set_are_the_csharp_lists_and_the_limit_is_six() {
        let v = view(&settings());
        assert_eq!(v.panel_presets.len(), 8);
        assert_eq!(
            v.panel_presets[0],
            Choice {
                value: "editor",
                label: "Code Editor"
            }
        );
        let whens: Vec<&str> = v.when_options.iter().map(|c| c.value).collect();
        assert_eq!(
            whens,
            ["", "youtube.playing", "youtube.shorts", "youtube.ended"]
        );
        assert_eq!(v.max_sets, 6);
        assert!(v.input.hardware && !v.input.keyboard && v.input.exclusive);
        assert_eq!(v.input.keys[0], "F1");
    }

    #[test]
    fn the_when_of_a_set_is_shown_in_its_canonical_form() {
        let mut s = settings();
        s.profiles[0].config_profiles[0].when = "Playing".into();
        s.profiles[0].multi_config_enabled = true;
        let p = &view(&s).profiles[0];
        assert_eq!(p.configs[0].when, "youtube.playing");
        assert!(p.multi);
    }

    #[test]
    fn nothing_but_the_view_fields_reach_the_page() {
        let json = serde_json::to_string(&view(&settings())).unwrap();
        assert!(!json.contains("extra"));
    }
}
