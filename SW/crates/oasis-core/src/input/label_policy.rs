//! Port of `Input/LabelPolicy.cs`.
//!
//! Whether a tap first reveals the button labels (second tap runs the action), decided per
//! profile. The general overlay setting is the default; a profile can override it either way,
//! so an app whose buttons the user already knows (YouTube) runs on the first tap while another
//! still shows what each button does.

pub struct LabelPolicy;

impl LabelPolicy {
    /// Follow the general overlay setting. The stored value for every profile saved before
    /// this option existed.
    pub const INHERIT: &'static str = "";
    /// First tap shows the labels, second tap runs the action.
    pub const SHOW: &'static str = "show";
    /// Taps run immediately.
    pub const HIDE: &'static str = "hide";

    /// The mode to keep for a stored value: anything unrecognised follows the general setting.
    pub fn normalize(mode: Option<&str>) -> &'static str {
        match mode.unwrap_or("").trim().to_lowercase().as_str() {
            Self::SHOW => Self::SHOW,
            Self::HIDE => Self::HIDE,
            _ => Self::INHERIT,
        }
    }

    /// Labels-on-first-tap for a profile, given the general setting.
    pub fn effective(general: bool, mode: Option<&str>) -> bool {
        match Self::normalize(mode) {
            Self::SHOW => true,
            Self::HIDE => false,
            _ => general,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LabelPolicy;

    #[test]
    fn a_profile_choice_beats_the_general_setting_and_anything_else_follows_it() {
        let cases: [(bool, Option<&str>, bool); 8] = [
            (true, Some(""), true),
            (false, Some(""), false),
            (true, Some("hide"), false),
            (false, Some("show"), true),
            (false, Some("SHOW"), true),
            (true, Some(" Hide "), false),
            (true, Some("nonsense"), true),
            (false, None, false),
        ];
        for (general, mode, expected) in cases {
            assert_eq!(
                LabelPolicy::effective(general, mode),
                expected,
                "{general} {mode:?}"
            );
        }
    }
}
