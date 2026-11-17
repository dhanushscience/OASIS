//! Port of `PadReducer.cs` + `PadReducer.Transitions.cs`, line for line.
//!
//! Split like the C# partial class: `press.rs` (button down), `release.rs` (button up,
//! double-click resolution), `tick.rs` (timer expiry); entry point and shared helpers here.
//!
//! Pure: no I/O, no clock reads, no service calls. Timers are requested as effects and come
//! back as [`PadEvent::Tick`]. The precedence encoded here is the C# `OverlayService` press
//! sequence, pinned by the characterization tests in `tests/pad_reducer.rs`. Change behaviour
//! here only with a test that shows the new behaviour first.

use std::time::Duration;

use super::claims::{GestureClaim, GestureClaimId};
use super::config::ArbiterConfig;
use super::state::{ArbiterContext, ArbiterEffect, PadEvent, PadState, PressArm, Transition};
use crate::primitives::{ButtonId, ButtonMask, Instant};

pub const DOUBLE_CLICK_TIMER: &str = "double-click";
pub const HOLD2X_TIMER: &str = "hold-2x";
pub const VOICE_PTT_TIMER: &str = "voice-ptt";
pub const AI_HOLD_TIMER: &str = "ai-hold";
pub const SHOW_DESKTOP_TIMER: &str = "show-desktop";

/// Sent to the browser extension to engage / undo the hold-for-speed playback rate.
pub const HOLD2X_ENGAGE_COMMAND: &str = "playbackRate.hold2x";
pub const HOLD2X_RESTORE_COMMAND: &str = "playbackRate.restore";

pub(super) type Claims<'a> = &'a [Box<dyn GestureClaim>];

/// Apply one event. Returns the next state and the effects the host must carry out, in order.
pub fn reduce(
    state: &PadState,
    evt: PadEvent,
    config: &ArbiterConfig,
    context: &ArbiterContext,
    claims: Claims<'_>,
) -> Transition {
    match evt {
        PadEvent::Disconnected => on_disconnected(state),
        PadEvent::Tick(at) => on_tick(state, at, config, context, claims),
        PadEvent::Down(button, at) if button.is_pad() => {
            on_down(state.clone(), button, at, config, context, claims)
        }
        PadEvent::Up(button, at) if button.is_pad() => {
            on_up(state, button, at, config, context, claims)
        }
        _ => Transition::to(state.clone()),
    }
}

fn on_disconnected(state: &PadState) -> Transition {
    let mut effects = Vec::new();
    cancel_all_timers(&mut effects);
    if is_voice_capturing(state) {
        effects.push(ArbiterEffect::EndVoiceCapture);
    }
    if is_hold2x_engaged(state) {
        effects.push(ArbiterEffect::SendToDevice(HOLD2X_RESTORE_COMMAND));
    }
    Transition::with(PadState::Idle, effects)
}

mod press;
mod release;
mod tick;

use press::on_down;
use release::on_up;
use tick::on_tick;

// ---------------------------------------------------------------- helpers

pub(super) fn dispatch(button: ButtonId, claim: GestureClaimId) -> ArbiterEffect {
    ArbiterEffect::DispatchCommand { button, claim }
}

pub(super) fn set_timer(name: &'static str, delay: Duration) -> ArbiterEffect {
    ArbiterEffect::SetTimer { name, delay }
}

pub(super) fn try_dispatch_bound_tap(
    button: ButtonId,
    context: &ArbiterContext,
    claims: Claims<'_>,
    state: &PadState,
    effects: &mut Vec<ArbiterEffect>,
) {
    if context.execution_allowed && allows(claims, GestureClaimId::BoundTap, state, context) {
        effects.push(dispatch(button, GestureClaimId::BoundTap));
    }
}

pub(super) fn is_hidden_volume(
    button: ButtonId,
    config: &ArbiterConfig,
    context: &ArbiterContext,
    claims: Claims<'_>,
    state: &PadState,
) -> bool {
    context.youtube_gestures_active
        && config.is_volume_button(button)
        && allows(claims, GestureClaimId::DoubleClickVolume, state, context)
}

/// The highest-priority claim with this id decides; no such claim means "allowed".
pub(super) fn allows(
    claims: Claims<'_>,
    id: GestureClaimId,
    state: &PadState,
    context: &ArbiterContext,
) -> bool {
    let mut found: Option<&dyn GestureClaim> = None;
    for claim in claims.iter().map(|c| c.as_ref()) {
        if claim.id() != id {
            continue;
        }
        if found.is_none_or(|f| claim.priority() > f.priority()) {
            found = Some(claim);
        }
    }
    found.is_none_or(|c| c.can_claim(state, context))
}

pub(super) fn due(at: Instant, deadline: Option<Instant>) -> bool {
    deadline.is_some_and(|d| at.is_at_or_after(d))
}

pub(super) fn cancel_all_timers(effects: &mut Vec<ArbiterEffect>) {
    for name in [
        DOUBLE_CLICK_TIMER,
        HOLD2X_TIMER,
        VOICE_PTT_TIMER,
        AI_HOLD_TIMER,
        SHOW_DESKTOP_TIMER,
    ] {
        effects.push(ArbiterEffect::CancelTimer(name));
    }
}

/// Desktop hold takes a lone press of its button that nothing stronger armed: voice PTT and
/// hold-2× return before this point, and an AI hold on the same buttons keeps the press.
pub(super) fn arms_show_desktop(
    button: ButtonId,
    held_now: ButtonMask,
    arm: &PressArm,
    config: &ArbiterConfig,
    context: &ArbiterContext,
    claims: Claims<'_>,
    state: &PadState,
) -> bool {
    config.is_show_desktop_button(button)
        && held_now == button.mask()
        && arm.voice_ptt_deadline.is_none()
        && !config.ai_trigger_held(held_now)
        && allows(claims, GestureClaimId::ShowDesktop, state, context)
}

/// A talk button that is alone in the push-to-talk combination: double-clicking it locks the mic on.
pub(super) fn is_ptt_lock_button(button: ButtonId, config: &ArbiterConfig) -> bool {
    config.voice_enabled && config.push_to_talk_mask == button.mask()
}

pub(super) fn is_voice_capturing(state: &PadState) -> bool {
    match state {
        PadState::VoiceLocked => true,
        PadState::Claimed { claim, .. } => *claim == GestureClaimId::VoicePtt,
        PadState::Pressed { arm, .. } => arm.voice_capturing,
        _ => false,
    }
}

pub(super) fn is_hold2x_engaged(state: &PadState) -> bool {
    match state {
        PadState::Claimed { claim, .. } => *claim == GestureClaimId::Hold2x,
        PadState::Pressed { arm, .. } => arm.hold2x_engaged,
        _ => false,
    }
}

pub(super) fn primary_button(held: ButtonMask) -> ButtonId {
    if held.contains(ButtonId::ONE) {
        ButtonId::ONE
    } else if held.contains(ButtonId::TWO) {
        ButtonId::TWO
    } else {
        ButtonId::THREE
    }
}
