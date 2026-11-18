//! Port of `Services/ProfileExtensions.cs` (the read-only parts) and the button → action
//! conversion the executor needs.

use oasis_core::actions::{ActionKind, ActionStep, ButtonAction, Next};
use oasis_core::bridge::ConfigWhen;
use serde_json::Value;

use crate::model::{Button, ConfigProfile, Profile};

impl ConfigProfile {
    /// The config that applies when nothing more specific does: its match is exactly `*`.
    pub fn is_primary(&self) -> bool {
        self.context_match.trim() == "*"
    }

    /// `When` in its canonical form: `""` (always), `youtube.playing`, `youtube.ended`,
    /// `youtube.shorts`, or the lower-cased text for anything else.
    pub fn normalized_when(&self) -> String {
        let when = self.when.trim();
        let is = |names: &[&str]| names.iter().any(|n| when.eq_ignore_ascii_case(n));
        if when.is_empty() || is(&["always"]) {
            String::new()
        } else if is(&["youtube.playing", "playing"]) {
            ConfigWhen::PLAYING.to_string()
        } else if is(&["youtube.ended", "ended"]) {
            ConfigWhen::ENDED.to_string()
        } else if is(&["youtube.shorts", "youtube.short", "shorts", "short"]) {
            ConfigWhen::SHORTS.to_string()
        } else {
            when.to_lowercase()
        }
    }

    pub fn has_media_when(&self) -> bool {
        !self.normalized_when().is_empty()
    }
}

impl Profile {
    /// The index of the primary config: the first one matching `*`, else the first config.
    /// `None` for a profile with no configs.
    pub fn primary_config_index(&self) -> Option<usize> {
        self.config_profiles.iter().position(ConfigProfile::is_primary).or(
            (!self.config_profiles.is_empty()).then_some(0),
        )
    }
}

impl Button {
    /// What this button does, for `oasis_core::actions::run_action`.
    pub fn to_action(&self) -> ButtonAction {
        ButtonAction {
            kind: self.kind.clone(),
            value: self.value.clone(),
            steps: self.automation_steps(),
        }
    }

    /// The steps of an `Automation` button. `Steps` is `""` or `[]` for every other kind; an
    /// entry that is not an object with text fields is skipped, with `Delay` / `1000` defaults
    /// for a missing field as in C#.
    pub fn automation_steps(&self) -> Vec<ActionStep> {
        let Value::Array(items) = &self.steps else { return Vec::new() };
        items
            .iter()
            .filter_map(Value::as_object)
            .map(|step| {
                let text = |key: &str, default: &str| {
                    step.get(key).and_then(Value::as_str).unwrap_or(default).to_string()
                };
                let int = |key: &str| step.get(key).and_then(Value::as_i64);
                ActionStep {
                    kind: text("Type", ActionKind::Delay.as_str()),
                    value: text("Value", "1000"),
                    on_ok: Next::from_code(int("Next")),
                    on_fail: Next::from_code(int("OnFail")),
                    pos: int("X").zip(int("Y")).map(|(x, y)| (x as i32, y as i32)),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(context_match: &str, when: &str) -> ConfigProfile {
        ConfigProfile { context_match: context_match.into(), when: when.into(), ..Default::default() }
    }

    #[test]
    fn only_an_exact_star_is_the_primary_config() {
        assert!(config("*", "").is_primary());
        assert!(config(" * ", "").is_primary());
        assert!(!config("", "").is_primary());
        assert!(!config("chrome", "").is_primary());
    }

    #[test]
    fn when_is_normalised_to_the_tokens_the_bridge_uses() {
        let when = |w| config("x", w).normalized_when();
        assert_eq!(when(""), "");
        assert_eq!(when("Always"), "");
        assert_eq!(when("Playing"), ConfigWhen::PLAYING);
        assert_eq!(when("YouTube.Ended"), ConfigWhen::ENDED);
        assert_eq!(when("short"), ConfigWhen::SHORTS);
        assert_eq!(when("Custom.Thing"), "custom.thing");
        assert!(config("x", "playing").has_media_when());
        assert!(!config("x", "always").has_media_when());
    }

    #[test]
    fn the_primary_config_is_the_star_one_else_the_first() {
        let profile = |configs: Vec<ConfigProfile>| Profile { config_profiles: configs, ..Default::default() };
        assert_eq!(profile(vec![]).primary_config_index(), None);
        assert_eq!(profile(vec![config("a", ""), config("*", "")]).primary_config_index(), Some(1));
        assert_eq!(profile(vec![config("a", ""), config("b", "")]).primary_config_index(), Some(0));
    }

    #[test]
    fn a_button_becomes_an_action_with_its_steps() {
        let button = Button {
            kind: "Automation".into(),
            steps: json!([
                { "Type": "Shortcut", "Value": "Win+D" },
                { "Type": "Delay" },
                { "Value": "5" },
                "not an object"
            ]),
            ..Default::default()
        };
        let action = button.to_action();
        assert_eq!(action.kind, "Automation");
        assert_eq!(
            action.steps,
            [
                ActionStep { kind: "Shortcut".into(), value: "Win+D".into(), ..Default::default() },
                ActionStep { kind: "Delay".into(), value: "1000".into(), ..Default::default() },
                ActionStep { kind: "Delay".into(), value: "5".into(), ..Default::default() },
            ]
        );
    }

    #[test]
    fn a_plain_button_has_no_steps() {
        for steps in [json!(""), json!([]), Value::Null] {
            let button = Button { kind: "Shortcut".into(), value: "A".into(), steps, ..Default::default() };
            assert!(button.to_action().steps.is_empty());
        }
    }
}
