//! Matching helpers. Port of `Oasis.Core/Matching/*`.
//!
//! Each of these used to be copied in several places and the copies drifted apart (one list of
//! browsers knew Arc, another did not; one YouTube check knew "yt music", another did not).
//! Now there is one definition of each idea.

pub mod address_bar;
pub mod browser_processes;
pub mod context;
pub mod edit_distance;
pub mod panel;
pub mod profile_targets;
pub mod text;
pub mod url_host;
pub mod web_catalog;
pub mod youtube_hosts;

pub use browser_processes::BrowserProcesses;
pub use context::ForegroundContext;
pub use edit_distance::{levenshtein, levenshtein_bounded, similarity};
pub use youtube_hosts::YouTubeHosts;
