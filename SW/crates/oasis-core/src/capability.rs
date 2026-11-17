//! Optional features and whether they are usable right now. Port of `Capabilities/Capability.cs`.
//!
//! "Offline is first-class": the pad, every model, every API key and every cloud service is
//! optional. A missing one is reported as [`CapabilityState::Missing`] with a reason the UI can
//! show — never as a swallowed error.

/// Things OASIS can do only when something optional is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Capability {
    Pad,
    SpeechToText,
    LocalPlanner,
    CloudPlanner,
    TextToSpeech,
    Spotify,
    BrowserBridge,
    Calendar,
    Marketplace,
    ScreenCapture,
}

impl Capability {
    pub const ALL: [Capability; 10] = [
        Capability::Pad,
        Capability::SpeechToText,
        Capability::LocalPlanner,
        Capability::CloudPlanner,
        Capability::TextToSpeech,
        Capability::Spotify,
        Capability::BrowserBridge,
        Capability::Calendar,
        Capability::Marketplace,
        Capability::ScreenCapture,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityState {
    Available,
    /// Not usable. `remedy_path` is where the user should put the missing thing, if anywhere
    /// (for a model: the models folder).
    Missing {
        reason: String,
        remedy_path: Option<String>,
    },
    /// Usable but worse than normal (e.g. the planner fell back to CPU).
    Degraded {
        reason: String,
    },
}

impl CapabilityState {
    pub fn missing(reason: impl Into<String>) -> CapabilityState {
        CapabilityState::Missing {
            reason: reason.into(),
            remedy_path: None,
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, CapabilityState::Available)
    }
}
