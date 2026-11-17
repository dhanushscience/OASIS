//! Port of `PadState.cs`, `PadEvent.cs`, `ArbiterContext.cs`, `ArbiterEffect.cs`, `Transition.cs`.

use std::time::Duration;

use super::claims::GestureClaimId;
use crate::primitives::{ButtonId, ButtonMask, CommandId, Instant};

/// What the pad reducer is doing between events.
#[derive(Debug, Clone, PartialEq)]
pub enum PadState {
    Idle,
    /// One or more buttons are down and the gesture is still undecided. `arm` carries every
    /// gesture that is still possible (PTT, hold-2×, AI hold, volume, show-desktop) at once.
    Pressed {
        held: ButtonMask,
        since: Instant,
        last: ButtonId,
        arm: PressArm,
    },
    /// A volume-button tap was released; waiting to see whether a second tap follows.
    AwaitingSecondTap {
        button: ButtonId,
        deadline: Instant,
        reveal_only: bool,
    },
    /// A gesture owns the press. `cont` keeps the arm alive when a later timer can still upgrade
    /// it (hold-2× engaged, AI hold still pending).
    Claimed {
        claim: GestureClaimId,
        held: ButtonMask,
        cont: Option<PressArm>,
    },
    Executing {
        command: CommandId,
        held: ButtonMask,
    },
    /// Push-to-talk was double-clicked: the mic stays open with no button held, until the
    /// push-to-talk button is pressed once more.
    VoiceLocked,
}

impl PadState {
    pub fn claimed(claim: GestureClaimId, held: ButtonMask) -> PadState {
        PadState::Claimed {
            claim,
            held,
            cont: None,
        }
    }
}

/// Timers and flags the C# `OverlayService` keeps as fields during a press.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PressArm {
    pub voice_ptt_deadline: Option<Instant>,
    pub hold2x_deadline: Option<Instant>,
    pub ai_hold_deadline: Option<Instant>,
    pub volume_candidate: bool,
    pub completing_volume: bool,
    pub dispatched_on_press: bool,
    pub hold2x_engaged: bool,
    pub voice_capturing: bool,
    pub hold2x_tracked: bool,
    pub show_desktop_deadline: Option<Instant>,
    pub show_desktop_tracked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadEvent {
    Down(ButtonId, Instant),
    Up(ButtonId, Instant),
    /// A timer the host set has fired (or the host is polling).
    Tick(Instant),
    /// BLE link lost: every gesture is cancelled.
    Disconnected,
}

/// One verdict for the whole gesture: the host probes the foreground app once (600 ms TTL in
/// the C# host) and passes the *same* context to press, release and double-click resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArbiterContext {
    pub youtube_gestures_active: bool,
    pub meeting_reminder_active: bool,
    pub ai_session_active: bool,
    pub confirmation_pending: bool,
    pub labels_on_screen: bool,
    /// False when the profile store failed its integrity check: presses are recorded but bound
    /// actions do not run.
    pub execution_allowed: bool,
}

impl ArbiterContext {
    pub fn new(youtube_gestures_active: bool) -> ArbiterContext {
        ArbiterContext {
            youtube_gestures_active,
            meeting_reminder_active: false,
            ai_session_active: false,
            confirmation_pending: false,
            labels_on_screen: false,
            execution_allowed: true,
        }
    }

    pub fn desktop() -> ArbiterContext {
        ArbiterContext::new(false)
    }

    pub fn youtube() -> ArbiterContext {
        ArbiterContext::new(true)
    }
}

impl Default for ArbiterContext {
    fn default() -> Self {
        ArbiterContext::desktop()
    }
}

/// What the reducer asks the host to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArbiterEffect {
    /// Run whatever this gesture means for this button in the active profile.
    DispatchCommand {
        button: ButtonId,
        claim: GestureClaimId,
    },
    StartVoiceCapture,
    EndVoiceCapture,
    /// Throw the recording away (button 2 while the mic is locked on).
    CancelVoiceCapture,
    /// Briefly show the button labels on the overlay.
    ShowPeek(Duration),
    SetTimer {
        name: &'static str,
        delay: Duration,
    },
    CancelTimer(&'static str),
    /// Send a command to the browser extension (hold-2× engage/restore).
    SendToDevice(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub next: PadState,
    pub effects: Vec<ArbiterEffect>,
}

impl Transition {
    pub fn to(next: PadState) -> Transition {
        Transition {
            next,
            effects: Vec::new(),
        }
    }

    pub fn with(next: PadState, effects: Vec<ArbiterEffect>) -> Transition {
        Transition { next, effects }
    }

    /// Every dispatched `(button, claim)` in order.
    pub fn dispatches(&self) -> impl Iterator<Item = (ButtonId, GestureClaimId)> + '_ {
        self.effects.iter().filter_map(|e| match e {
            ArbiterEffect::DispatchCommand { button, claim } => Some((*button, *claim)),
            _ => None,
        })
    }

    pub fn dispatched(&self, claim: GestureClaimId) -> bool {
        self.dispatches().any(|(_, c)| c == claim)
    }

    pub fn sets_timer(&self, name: &str) -> bool {
        self.effects
            .iter()
            .any(|e| matches!(e, ArbiterEffect::SetTimer { name: n, .. } if *n == name))
    }

    pub fn timer_delay(&self, name: &str) -> Option<Duration> {
        self.effects.iter().find_map(|e| match e {
            ArbiterEffect::SetTimer { name: n, delay } if *n == name => Some(*delay),
            _ => None,
        })
    }

    pub fn cancels_timer(&self, name: &str) -> bool {
        self.effects
            .iter()
            .any(|e| matches!(e, ArbiterEffect::CancelTimer(n) if *n == name))
    }

    pub fn sends(&self, command: &str) -> bool {
        self.effects
            .iter()
            .any(|e| matches!(e, ArbiterEffect::SendToDevice(c) if *c == command))
    }

    pub fn has(&self, effect: &ArbiterEffect) -> bool {
        self.effects.contains(effect)
    }
}
