//! Port of `Voice/VoiceActionPolicy.cs`.
//!
//! Invariant 14: a `RunScript` action never runs from voice, even when scripts are enabled in
//! settings. A misheard phrase must not be able to run arbitrary code.

use crate::actions::ActionKind;

pub struct VoiceActionPolicy;

impl VoiceActionPolicy {
    pub fn is_forbidden(action_type: Option<&str>) -> bool {
        // An AutoHotkey script is code too: the same rule.
        action_type.is_some_and(|t| matches!(ActionKind::of_ignore_case(t), ActionKind::RunScript | ActionKind::AutoHotkey))
    }
}
