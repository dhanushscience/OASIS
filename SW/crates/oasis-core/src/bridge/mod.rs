//! Browser-extension bridge rules. Port of `Oasis.Core/Bridge/*` and the pure part of
//! `Services/BrowserBridge/YouTubeWatchState.cs`.
//!
//! The socket server itself lives in `oasis-platform` / `oasis-app`; this module only holds
//! the decisions: who may connect, which messages are understood, and what YouTube is doing.

pub mod policy;
pub mod youtube_watch;

pub use policy::BrowserBridgePolicy;
pub use youtube_watch::{ConfigWhen, YouTubePlaybackPhase, YouTubeWatchEvent, YouTubeWatchState};
