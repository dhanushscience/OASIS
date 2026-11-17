//! Voice session rules. Port of `Oasis.Core/Voice/*`.
//!
//! [`reducer`] is the push-to-talk lifecycle (idle → capturing → busy → idle) as a pure state
//! machine. [`media_duck`] decides whether opening the mic may pause YouTube.
//! [`action_policy`] holds the voice safety invariants.

pub mod action_policy;
pub mod command;
pub mod media_duck;
pub mod models;
pub mod placement;
pub mod reducer;
pub mod self_check;

pub use action_policy::VoiceActionPolicy;
pub use media_duck::{MediaDuckPolicy, MediaPlayback};
pub use reducer::{
    reduce, VoiceEffect, VoiceEvent, VoiceSessionConfig, VoiceState, VoiceTransition,
    MAX_CAPTURE_TIMER, MAX_UTTERANCE,
};
