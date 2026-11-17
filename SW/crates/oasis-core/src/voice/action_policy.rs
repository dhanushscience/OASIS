//! Port of `Voice/VoiceActionPolicy.cs`.
//!
//! Invariant 14: a `RunScript` action never runs from voice, even when scripts are enabled in
//! settings. A misheard phrase must not be able to run arbitrary code.

pub struct VoiceActionPolicy;

impl VoiceActionPolicy {
    pub const RUN_SCRIPT_TYPE: &'static str = "RunScript";

    pub fn is_forbidden(action_type: Option<&str>) -> bool {
        // An AutoHotkey script is code too: the same rule.
        action_type.is_some_and(|t| t.eq_ignore_ascii_case(Self::RUN_SCRIPT_TYPE) || t.eq_ignore_ascii_case("AutoHotkey"))
    }
}
