//! Pad gesture arbitration. Port of `Oasis.Core/Input/*`.
//!
//! Three buttons have to carry taps, double-taps, holds, chords, push-to-talk, hold-for-2×
//! speed, AI hold and show-desktop. [`reduce`] decides, for every press/release/tick, which of
//! those the gesture is. See `README.md` in this folder for the rules in plain words.

mod claims;
mod config;
mod keybinds;
mod label_policy;
mod reducer;
mod state;

pub use claims::{default_claims, GestureClaim, GestureClaimId, PriorityClaim};
pub use config::ArbiterConfig;
pub use keybinds::{Binding, KeyMatcher, KeyVerdict};
pub use label_policy::LabelPolicy;
pub use reducer::{
    reduce, AI_HOLD_TIMER, DOUBLE_CLICK_TIMER, HOLD2X_ENGAGE_COMMAND, HOLD2X_RESTORE_COMMAND,
    HOLD2X_TIMER, SHOW_DESKTOP_TIMER, VOICE_PTT_TIMER,
};
pub use state::{ArbiterContext, ArbiterEffect, PadEvent, PadState, PressArm, Transition};
