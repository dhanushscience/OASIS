//! Timer expiry: hold-2×, show-desktop, voice PTT and AI hold deadlines.

use super::*;

// ---------------------------------------------------------------- timers

pub(super) fn on_tick(
    state: &PadState,
    at: Instant,
    config: &ArbiterConfig,
    context: &ArbiterContext,
    claims: Claims<'_>,
) -> Transition {
    let mut effects = Vec::new();

    match state {
        PadState::AwaitingSecondTap {
            button,
            deadline,
            reveal_only,
        } => {
            if !at.is_at_or_after(*deadline) {
                return Transition::to(state.clone());
            }
            effects.push(ArbiterEffect::CancelTimer(DOUBLE_CLICK_TIMER));
            if !reveal_only {
                try_dispatch_bound_tap(*button, context, claims, state, &mut effects);
            }
            return Transition::with(PadState::Idle, effects);
        }
        PadState::Claimed {
            claim,
            held,
            cont: Some(cont),
        } => {
            if due(at, cont.ai_hold_deadline)
                && allows(claims, GestureClaimId::AiHold, state, context)
                && config.ai_trigger_held(*held)
            {
                if *claim == GestureClaimId::Hold2x || cont.hold2x_engaged {
                    effects.push(ArbiterEffect::SendToDevice(HOLD2X_RESTORE_COMMAND));
                }
                effects.push(ArbiterEffect::CancelTimer(AI_HOLD_TIMER));
                effects.push(ArbiterEffect::CancelTimer(HOLD2X_TIMER));
                effects.push(dispatch(primary_button(*held), GestureClaimId::AiHold));
                return Transition::with(PadState::claimed(GestureClaimId::AiHold, *held), effects);
            }
            return Transition::to(state.clone());
        }
        PadState::Pressed { .. } => {}
        _ => return Transition::to(state.clone()),
    }

    let PadState::Pressed {
        held,
        since,
        last,
        arm,
    } = state
    else {
        return Transition::to(state.clone());
    };
    let (held, since, last, mut arm) = (*held, *since, *last, *arm);

    if due(at, arm.hold2x_deadline) && config.hold2x.is_some_and(|b| held.contains(b)) {
        effects.push(ArbiterEffect::SendToDevice(HOLD2X_ENGAGE_COMMAND));
        effects.push(ArbiterEffect::CancelTimer(HOLD2X_TIMER));
        arm.hold2x_deadline = None;
        arm.hold2x_engaged = true;
        arm.hold2x_tracked = true;
        if arm.voice_ptt_deadline.is_none() && arm.ai_hold_deadline.is_none() {
            return Transition::with(
                PadState::Claimed {
                    claim: GestureClaimId::Hold2x,
                    held,
                    cont: Some(arm),
                },
                effects,
            );
        }
        return Transition::with(
            PadState::Pressed {
                held,
                since,
                last,
                arm,
            },
            effects,
        );
    }

    if let Some(desktop) = config.show_desktop {
        if due(at, arm.show_desktop_deadline)
            && held == desktop.mask()
            && context.execution_allowed
            && allows(claims, GestureClaimId::ShowDesktop, state, context)
        {
            effects.push(ArbiterEffect::CancelTimer(SHOW_DESKTOP_TIMER));
            effects.push(ArbiterEffect::CancelTimer(DOUBLE_CLICK_TIMER));
            effects.push(dispatch(desktop, GestureClaimId::ShowDesktop));
            return Transition::with(
                PadState::claimed(GestureClaimId::ShowDesktop, held),
                effects,
            );
        }
    }

    if due(at, arm.voice_ptt_deadline)
        && held == config.push_to_talk_mask
        && allows(claims, GestureClaimId::VoicePtt, state, context)
    {
        effects.push(ArbiterEffect::StartVoiceCapture);
        effects.push(ArbiterEffect::CancelTimer(VOICE_PTT_TIMER));
        arm.voice_ptt_deadline = None;
        arm.voice_capturing = true;
        if !arm.hold2x_engaged && arm.ai_hold_deadline.is_none() {
            return Transition::with(PadState::claimed(GestureClaimId::VoicePtt, held), effects);
        }
        return Transition::with(
            PadState::Pressed {
                held,
                since,
                last,
                arm,
            },
            effects,
        );
    }

    if due(at, arm.ai_hold_deadline)
        && config.ai_trigger_held(held)
        && allows(claims, GestureClaimId::AiHold, state, context)
    {
        if arm.hold2x_engaged {
            effects.push(ArbiterEffect::SendToDevice(HOLD2X_RESTORE_COMMAND));
        }
        effects.push(ArbiterEffect::CancelTimer(AI_HOLD_TIMER));
        effects.push(ArbiterEffect::CancelTimer(HOLD2X_TIMER));
        effects.push(ArbiterEffect::CancelTimer(DOUBLE_CLICK_TIMER));
        effects.push(dispatch(primary_button(held), GestureClaimId::AiHold));
        return Transition::with(PadState::claimed(GestureClaimId::AiHold, held), effects);
    }

    Transition::with(
        PadState::Pressed {
            held,
            since,
            last,
            arm,
        },
        effects,
    )
}
