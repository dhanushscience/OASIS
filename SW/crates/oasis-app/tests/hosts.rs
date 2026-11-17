//! Port of `SW/tests/Oasis.Application.Tests/ArbiterHostTests.cs`, plus voice-host and
//! pad-link checks.

use std::time::Duration;

use oasis_app::{ArbiterHost, CapabilityRegistry, PadLink, VoiceSessionHost};
use oasis_core::capability::{Capability, CapabilityState};
use oasis_core::input::{
    ArbiterConfig, ArbiterContext, ArbiterEffect, GestureClaimId, PadEvent, PadState,
};
use oasis_core::voice::{VoiceEffect, VoiceEvent, VoiceSessionConfig, VoiceState};
use oasis_core::{ButtonId, Instant};

fn at(ms: i64) -> Instant {
    Instant::from_millis(ms)
}

fn dispatches(effects: &[ArbiterEffect]) -> Vec<GestureClaimId> {
    effects
        .iter()
        .filter_map(|e| match e {
            ArbiterEffect::DispatchCommand { claim, .. } => Some(*claim),
            _ => None,
        })
        .collect()
}

#[test]
fn desktop_press_dispatches_bound_tap() {
    let mut host = ArbiterHost::new(ArbiterConfig::product_defaults(), false);
    let mut seen = Vec::new();
    host.apply(
        PadEvent::Down(ButtonId::ONE, at(0)),
        at(0),
        None,
        None,
        &mut |e| seen.push(e.clone()),
    );
    assert!(host.last_transition().dispatched(GestureClaimId::BoundTap));
    assert_eq!(dispatches(&seen).len(), 1);
    assert!(matches!(host.state(), PadState::Pressed { .. }));
}

#[test]
fn owned_timers_fire_volume_window_as_bound_tap() {
    let config = ArbiterConfig {
        show_overlay_labels: false,
        ..ArbiterConfig::product_defaults()
    };
    let mut host = ArbiterHost::new(config, true);
    let ctx = Some(ArbiterContext::youtube());
    let mut seen = Vec::new();
    host.apply(
        PadEvent::Down(ButtonId::ONE, at(0)),
        at(0),
        None,
        ctx,
        &mut |e| seen.push(e.clone()),
    );
    host.apply(
        PadEvent::Up(ButtonId::ONE, at(40)),
        at(40),
        None,
        ctx,
        &mut |e| seen.push(e.clone()),
    );
    seen.clear();

    host.fire_due(at(740), &mut |e| seen.push(e.clone()));
    assert!(dispatches(&seen).contains(&GestureClaimId::BoundTap));
    assert_eq!(host.state(), &PadState::Idle);
}

#[test]
fn post_then_drain_applies_in_order() {
    let mut host = ArbiterHost::new(ArbiterConfig::product_defaults(), false);
    host.post(PadEvent::Down(ButtonId::ONE, at(0)));
    host.post(PadEvent::Up(ButtonId::ONE, at(50)));
    let mut seen = Vec::new();
    host.drain(at(50), &mut |e| seen.push(e.clone()));
    assert_eq!(host.state(), &PadState::Idle);
    assert_eq!(dispatches(&seen).len(), 1);
}

#[test]
fn posting_from_another_thread_works() {
    let mut host = ArbiterHost::new(ArbiterConfig::product_defaults(), false);
    let tx = host.sender();
    std::thread::spawn(move || {
        tx.send(PadEvent::Down(ButtonId::TWO, at(0)).into())
            .unwrap();
    })
    .join()
    .unwrap();
    let mut seen = Vec::new();
    host.drain(at(0), &mut |e| seen.push(e.clone()));
    assert_eq!(dispatches(&seen), vec![GestureClaimId::BoundTap]);
}

#[test]
fn shadow_mode_does_not_self_fire_timers() {
    let config = ArbiterConfig {
        show_overlay_labels: false,
        ..ArbiterConfig::product_defaults()
    };
    let mut host = ArbiterHost::new(config, false);
    let ctx = Some(ArbiterContext::youtube());
    let mut seen = Vec::new();
    host.apply(
        PadEvent::Down(ButtonId::ONE, at(0)),
        at(0),
        None,
        ctx,
        &mut |e| seen.push(e.clone()),
    );
    host.apply(
        PadEvent::Up(ButtonId::ONE, at(40)),
        at(40),
        None,
        ctx,
        &mut |e| seen.push(e.clone()),
    );
    seen.clear();
    host.fire_due(at(740), &mut |e| seen.push(e.clone()));
    assert!(seen.is_empty());
    assert!(matches!(host.state(), PadState::AwaitingSecondTap { .. }));
}

#[test]
fn voice_host_ends_capture_at_the_max_utterance_timer() {
    let mut host = VoiceSessionHost::new(VoiceSessionConfig::product_defaults(), true);
    let mut seen = Vec::new();
    host.apply(
        VoiceEvent::BeginRequested {
            at: at(0),
            enabled: true,
            model_ready: true,
            dictate: false,
        },
        at(0),
        &mut |e| seen.push(*e),
    );
    assert!(seen.contains(&VoiceEffect::StartMic));
    assert_eq!(
        host.next_deadline(),
        Some(at(0).plus(oasis_core::voice::MAX_UTTERANCE))
    );
    seen.clear();
    host.fire_due(at(30 * 60_000), &mut |e| seen.push(*e));
    assert!(matches!(host.state(), VoiceState::Busy { .. }));
    assert!(seen.contains(&VoiceEffect::StopMic));
}

#[test]
fn pad_link_turns_notifications_into_events_and_telemetry() {
    let mut link = PadLink::new();
    let (events, changed) = link.on_notification(b"BTN:0:D\nBAT:81:C\n", at(5));
    assert_eq!(events, vec![PadEvent::Down(ButtonId::ONE, at(5))]);
    assert!(changed);
    assert_eq!(link.telemetry().battery_percent, Some(81));
    assert!(link.telemetry().is_charging);
    // Same reading again is not a change (no 80/81 % flutter downstream).
    let (_, changed) = link.on_notification(b"BAT:81:C\n", at(6));
    assert!(!changed);
    let (events, _) = link.on_notification(b"PWR:x\nnonsense\nBTN:0:U", at(7));
    assert_eq!(events, vec![PadEvent::Up(ButtonId::ONE, at(7))]);
    assert_eq!(link.on_disconnected(), PadEvent::Disconnected);
}

#[test]
fn capability_registry_reports_honestly_and_notifies_on_change() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let reg = CapabilityRegistry::new();
    assert!(matches!(
        reg.get(Capability::Pad),
        CapabilityState::Missing { .. }
    ));
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    reg.subscribe(move |_, _| {
        h.fetch_add(1, Ordering::SeqCst);
    });
    assert!(reg.set(Capability::Pad, CapabilityState::Available));
    assert!(!reg.set(Capability::Pad, CapabilityState::Available));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(reg.available(), vec![Capability::Pad]);
}
