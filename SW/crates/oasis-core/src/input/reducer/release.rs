//! Button up: ending holds, resolving taps, and the double-click window.

use std::time::Duration;

use super::*;

// ---------------------------------------------------------------- release

pub(super) fn on_up(
    state: &PadState,
    button: ButtonId,
    at: Instant,
    config: &ArbiterConfig,
    context: &ArbiterContext,
    claims: Claims<'_>,
) -> Transition {
    let mut effects = Vec::new();

    if matches!(state, PadState::Idle | PadState::AwaitingSecondTap { .. } | PadState::VoiceLocked) {
        return Transition::to(state.clone());
    }

    if context.confirmation_pending && allows(claims, GestureClaimId::Confirmation, state, context)
    {
        effects.push(dispatch(button, GestureClaimId::Confirmation));
        return Transition::with(PadState::Idle, effects);
    }

    let (held_before, since, last, mut arm) = match state {
        PadState::Claimed { claim, held, .. } => {
            return on_claimed_up(state, *claim, *held, button, config, effects);
        }
        PadState::Pressed {
            held,
            since,
            last,
            arm,
        } => (*held, *since, *last, *arm),
        _ => return Transition::to(state.clone()),
    };

    if !held_before.contains(button) {
        return Transition::to(state.clone());
    }

    let held = held_before.without(button);
    let remaining = |arm: PressArm| {
        if held.is_empty() {
            PadState::Idle
        } else {
            PadState::Pressed {
                held,
                since,
                last,
                arm,
            }
        }
    };

    if arm.voice_capturing {
        if !config.push_to_talk_mask.contains(button) {
            return Transition::to(state.clone());
        }
        effects.push(ArbiterEffect::EndVoiceCapture);
        effects.push(ArbiterEffect::CancelTimer(VOICE_PTT_TIMER));
        return Transition::with(PadState::Idle, effects);
    }

    if arm.hold2x_engaged && config.is_hold2x_button(button) {
        effects.push(ArbiterEffect::SendToDevice(HOLD2X_RESTORE_COMMAND));
        effects.push(ArbiterEffect::CancelTimer(HOLD2X_TIMER));
        return Transition::with(PadState::Idle, effects);
    }

    let cancelled_ptt =
        arm.voice_ptt_deadline.is_some() && config.push_to_talk_mask.contains(button);
    if cancelled_ptt {
        effects.push(ArbiterEffect::CancelTimer(VOICE_PTT_TIMER));
        arm.voice_ptt_deadline = None;
    }

    if arm.hold2x_deadline.is_some() && config.is_hold2x_button(button) {
        effects.push(ArbiterEffect::CancelTimer(HOLD2X_TIMER));
        arm.hold2x_deadline = None;
        arm.hold2x_tracked = true;
    }

    if arm.ai_hold_deadline.is_some() {
        effects.push(ArbiterEffect::CancelTimer(AI_HOLD_TIMER));
        arm.ai_hold_deadline = None;
    }

    let desktop_tracked = arm.show_desktop_tracked && config.is_show_desktop_button(button);
    if desktop_tracked && arm.show_desktop_deadline.is_some() {
        effects.push(ArbiterEffect::CancelTimer(SHOW_DESKTOP_TIMER));
        arm.show_desktop_deadline = None;
    }

    let volume_hidden = is_hidden_volume(button, config, context, claims, state)
        || arm.volume_candidate
        || arm.completing_volume;
    let hold2x_tracked = arm.hold2x_tracked || config.is_hold2x_button(button);
    let resolve_deferred = cancelled_ptt
        || hold2x_tracked
        || desktop_tracked
        || config.ai_owns_press_path()
        || volume_hidden
        || arm.completing_volume;

    if !resolve_deferred {
        if !arm.dispatched_on_press
            && context.execution_allowed
            && allows(claims, GestureClaimId::BoundTap, state, context)
        {
            effects.push(dispatch(button, GestureClaimId::BoundTap));
        }
        return Transition::with(remaining(arm), effects);
    }

    if cancelled_ptt && !volume_hidden && !arm.completing_volume {
        if held.is_empty() && is_ptt_lock_button(button, config) {
            // Wait for a second tap that locks the mic on; the bound tap runs if none comes.
            let delay = config.double_click_delay();
            effects.push(set_timer(DOUBLE_CLICK_TIMER, delay));
            return Transition::with(
                PadState::AwaitingSecondTap { button, deadline: at.plus(delay), reveal_only: false },
                effects,
            );
        }
        if context.execution_allowed && allows(claims, GestureClaimId::BoundTap, state, context) {
            effects.push(dispatch(button, GestureClaimId::BoundTap));
        }
        return Transition::with(remaining(arm), effects);
    }

    resolve_double_click_or_tap(
        button, at, held, arm, config, context, claims, state, effects,
    )
}

fn on_claimed_up(
    state: &PadState,
    claim: GestureClaimId,
    _held: ButtonMask,
    button: ButtonId,
    config: &ArbiterConfig,
    mut effects: Vec<ArbiterEffect>,
) -> Transition {
    match claim {
        GestureClaimId::MeetingReminder
        | GestureClaimId::AiSession
        | GestureClaimId::Confirmation => {
            effects.push(dispatch(button, claim));
            Transition::with(PadState::Idle, effects)
        }
        GestureClaimId::VoicePtt => {
            if !config.push_to_talk_mask.contains(button) {
                return Transition::to(state.clone());
            }
            effects.push(ArbiterEffect::EndVoiceCapture);
            effects.push(ArbiterEffect::CancelTimer(VOICE_PTT_TIMER));
            Transition::with(PadState::Idle, effects)
        }
        GestureClaimId::Hold2x => {
            if !config.is_hold2x_button(button) {
                return Transition::to(state.clone());
            }
            effects.push(ArbiterEffect::SendToDevice(HOLD2X_RESTORE_COMMAND));
            effects.push(ArbiterEffect::CancelTimer(HOLD2X_TIMER));
            Transition::with(PadState::Idle, effects)
        }
        GestureClaimId::AiHold => Transition::to(PadState::Idle),
        // The hold already showed the desktop; its release is not a tap.
        GestureClaimId::ShowDesktop => Transition::to(if config.is_show_desktop_button(button) {
            PadState::Idle
        } else {
            state.clone()
        }),
        GestureClaimId::BoundTap | GestureClaimId::DoubleClickVolume => {
            Transition::to(state.clone())
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_double_click_or_tap(
    button: ButtonId,
    at: Instant,
    held: ButtonMask,
    arm: PressArm,
    config: &ArbiterConfig,
    context: &ArbiterContext,
    claims: Claims<'_>,
    source: &PadState,
    mut effects: Vec<ArbiterEffect>,
) -> Transition {
    if arm.completing_volume {
        effects.push(ArbiterEffect::CancelTimer(DOUBLE_CLICK_TIMER));
        effects.push(dispatch(button, GestureClaimId::DoubleClickVolume));
        return Transition::with(PadState::Idle, effects);
    }

    if !is_hidden_volume(button, config, context, claims, source) && !arm.volume_candidate {
        // A tap already run on press (the hold-2× button outside YouTube) is not run again. The
        // flag is shared by every held button, so it only counts for the one pressed last.
        let ran_on_press = arm.dispatched_on_press
            && matches!(source, PadState::Pressed { last, .. } if *last == button);
        if !ran_on_press
            && context.execution_allowed
            && allows(claims, GestureClaimId::BoundTap, source, context)
        {
            effects.push(dispatch(button, GestureClaimId::BoundTap));
        }
        let next = if held.is_empty() {
            PadState::Idle
        } else {
            PadState::Pressed {
                held,
                since: at,
                last: button,
                arm,
            }
        };
        return Transition::with(next, effects);
    }

    let reveal_only = config.show_overlay_labels && !context.labels_on_screen;
    if reveal_only {
        effects.push(ArbiterEffect::ShowPeek(Duration::from_secs(4)));
    }

    let delay = config.double_click_delay();
    effects.push(set_timer(DOUBLE_CLICK_TIMER, delay));
    Transition::with(
        PadState::AwaitingSecondTap {
            button,
            deadline: at.plus(delay),
            reveal_only,
        },
        effects,
    )
}
