//! Port of `Input/GestureClaimId.cs`, `IGestureClaim.cs`, `GestureClaims.cs`.
//!
//! A *claim* is a gesture that may take ownership of a press. When two claims share an id the
//! higher priority one decides. Priorities match the C# `OverlayService` press sequence:
//! meeting reminders and live sessions outrank holds, hidden YouTube gestures outrank taps.

use super::state::{ArbiterContext, PadState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GestureClaimId {
    BoundTap,
    DoubleClickVolume,
    Hold2x,
    VoicePtt,
    AiHold,
    ShowDesktop,
    MeetingReminder,
    Confirmation,
    AiSession,
}

impl GestureClaimId {
    /// The same strings the C# app uses (logs, fingerprints).
    pub fn as_str(self) -> &'static str {
        match self {
            GestureClaimId::BoundTap => "bound-tap",
            GestureClaimId::DoubleClickVolume => "double-click-volume",
            GestureClaimId::Hold2x => "hold-2x",
            GestureClaimId::VoicePtt => "voice-ptt",
            GestureClaimId::AiHold => "ai-hold",
            GestureClaimId::ShowDesktop => "show-desktop",
            GestureClaimId::MeetingReminder => "meeting",
            GestureClaimId::Confirmation => "confirmation",
            GestureClaimId::AiSession => "ai-session",
        }
    }
}

impl std::fmt::Display for GestureClaimId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A gesture that may take a press (C# `IGestureClaim`).
pub trait GestureClaim: Send + Sync {
    fn id(&self) -> GestureClaimId;
    fn priority(&self) -> i32;
    fn can_claim(&self, state: &PadState, context: &ArbiterContext) -> bool;
}

/// The built-in claims. Each is a priority plus a rule over the context.
#[derive(Debug, Clone, Copy)]
pub struct PriorityClaim {
    id: GestureClaimId,
    priority: i32,
}

impl PriorityClaim {
    pub const fn new(id: GestureClaimId, priority: i32) -> PriorityClaim {
        PriorityClaim { id, priority }
    }
}

fn no_modal(c: &ArbiterContext) -> bool {
    !c.meeting_reminder_active && !c.ai_session_active && !c.confirmation_pending
}

impl GestureClaim for PriorityClaim {
    fn id(&self) -> GestureClaimId {
        self.id
    }

    fn priority(&self) -> i32 {
        self.priority
    }

    fn can_claim(&self, _state: &PadState, c: &ArbiterContext) -> bool {
        match self.id {
            GestureClaimId::MeetingReminder => c.meeting_reminder_active,
            GestureClaimId::AiSession => c.ai_session_active,
            GestureClaimId::Confirmation => c.confirmation_pending,
            // Hidden YouTube gestures exist only while YouTube is the foreground player.
            GestureClaimId::Hold2x | GestureClaimId::DoubleClickVolume => {
                c.youtube_gestures_active && no_modal(c)
            }
            // Show-desktop yields to voice, hold-2× and an AI hold on the same button; the
            // reducer arms it only when none of those took the press.
            GestureClaimId::VoicePtt
            | GestureClaimId::ShowDesktop
            | GestureClaimId::AiHold
            | GestureClaimId::BoundTap => no_modal(c),
        }
    }
}

/// The default claim list in descending priority — the order `OverlayService` checks them.
pub fn default_claims() -> Vec<Box<dyn GestureClaim>> {
    use GestureClaimId::*;
    [
        (MeetingReminder, 1000),
        (AiSession, 900),
        (Confirmation, 850),
        (VoicePtt, 80),
        (Hold2x, 70),
        (DoubleClickVolume, 60),
        (ShowDesktop, 55),
        (AiHold, 50),
        (BoundTap, 10),
    ]
    .into_iter()
    .map(|(id, p)| Box::new(PriorityClaim::new(id, p)) as Box<dyn GestureClaim>)
    .collect()
}
