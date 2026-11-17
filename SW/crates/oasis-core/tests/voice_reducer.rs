//! Port of `SW/tests/Oasis.Architecture.Tests/VoiceReducerTests.cs`, same test names.

use std::time::Duration;

use oasis_core::voice::*;
use oasis_core::Instant;

fn at(ms: i64) -> Instant {
    Instant::from_millis(ms)
}

fn step(state: VoiceState, evt: VoiceEvent) -> VoiceTransition {
    reduce(state, evt, &VoiceSessionConfig::product_defaults())
}

fn begin(ms: i64) -> VoiceEvent {
    begin_with(ms, true, true, false)
}

fn begin_with(ms: i64, enabled: bool, model_ready: bool, dictate: bool) -> VoiceEvent {
    VoiceEvent::BeginRequested {
        at: at(ms),
        enabled,
        model_ready,
        dictate,
    }
}

fn timer_delays(t: &VoiceTransition) -> Vec<Duration> {
    t.effects
        .iter()
        .filter_map(|e| match e {
            VoiceEffect::SetTimer { name, delay } if *name == MAX_CAPTURE_TIMER => Some(*delay),
            _ => None,
        })
        .collect()
}

const IDLE: VoiceState = VoiceState::Idle;

fn busy(dictate: bool) -> VoiceState {
    VoiceState::Busy {
        since: at(0),
        dictate,
    }
}

#[test]
fn begin_from_idle_starts_mic_and_listening() {
    let t = step(IDLE, begin(0));
    assert_eq!(
        t.next,
        VoiceState::Capturing {
            since: at(0),
            dictate: false,
            command_takeover: false
        }
    );
    assert!(t.has(&VoiceEffect::StartMic));
    assert!(t.has(&VoiceEffect::ShowListening));
    assert!(t.has(&VoiceEffect::DuckMedia));
    assert_eq!(timer_delays(&t), vec![Duration::from_secs(30 * 60)]);
}

#[test]
fn begin_dictate_shows_dictating_and_uses_the_thirty_minute_cap() {
    let t = step(IDLE, begin_with(0, true, true, true));
    assert!(matches!(
        t.next,
        VoiceState::Capturing { dictate: true, .. }
    ));
    assert!(t.has(&VoiceEffect::ShowDictating));
    assert!(!t.has(&VoiceEffect::ShowListening));
    assert_eq!(timer_delays(&t), vec![Duration::from_secs(30 * 60)]);
}

#[test]
fn begin_disabled_or_without_model_does_not_start_mic() {
    assert!(!step(IDLE, begin_with(0, false, true, false)).starts_mic());
    assert!(!step(IDLE, begin_with(0, true, false, false)).starts_mic());
    assert_eq!(step(IDLE, begin_with(0, false, true, false)).next, IDLE);
}

#[test]
fn begin_while_capturing_does_not_start_a_second_mic() {
    let open = step(IDLE, begin(0));
    let again = step(open.next, begin(50));
    assert_eq!(again.next, open.next);
    assert!(!again.starts_mic());
}

#[test]
fn begin_while_busy_within_watchdog_is_rejected() {
    let t = step(busy(false), begin(10_000));
    assert_eq!(t.next, busy(false));
    assert!(!t.starts_mic());
    assert!(!t.has(&VoiceEffect::ClearStuckBusy));
}

#[test]
fn begin_while_busy_after_45s_clears_watchdog_and_starts_capture() {
    let t = step(busy(false), begin(45_001));
    assert!(matches!(t.next, VoiceState::Capturing { .. }));
    assert!(t.has(&VoiceEffect::ClearStuckBusy));
    assert!(t.starts_mic());
}

#[test]
fn begin_after_stuck_busy_when_disabled_clears_flag_without_mic() {
    let t = step(busy(false), begin_with(45_001, false, true, false));
    assert_eq!(t.next, IDLE);
    assert!(t.has(&VoiceEffect::ClearStuckBusy));
    assert!(!t.starts_mic());
}

#[test]
fn cancel_from_capturing_stops_mic_and_returns_idle() {
    let open = step(IDLE, begin(0));
    let t = step(open.next, VoiceEvent::CancelRequested);
    assert_eq!(t.next, IDLE);
    assert!(t.has(&VoiceEffect::StopMic));
    assert!(t.has(&VoiceEffect::HideOverlay));
    assert!(t.has(&VoiceEffect::RestoreMedia));
}

#[test]
fn cancel_from_idle_hides_chrome_without_starting_or_stopping_mic() {
    let t = step(IDLE, VoiceEvent::CancelRequested);
    assert_eq!(t.next, IDLE);
    assert!(t.has(&VoiceEffect::HideOverlay));
    assert!(t.has(&VoiceEffect::RestoreMedia));
    assert!(!t.starts_mic());
    assert!(!t.has(&VoiceEffect::StopMic));
}

#[test]
fn cancel_from_busy_leaves_busy_latched() {
    let t = step(busy(false), VoiceEvent::CancelRequested);
    assert_eq!(t.next, busy(false));
    assert!(t.has(&VoiceEffect::HideOverlay));
    assert!(!t.has(&VoiceEffect::StopMic));
}

#[test]
fn end_command_capture_goes_busy_and_stops_mic() {
    let open = step(IDLE, begin(0));
    let t = step(open.next, VoiceEvent::EndRequested { at: at(400) });
    assert_eq!(
        t.next,
        VoiceState::Busy {
            since: at(400),
            dictate: false
        }
    );
    assert!(t.has(&VoiceEffect::StopMic));
    assert!(t.has(&VoiceEffect::ShowThinking));
    assert!(t.has(&VoiceEffect::BeginProcessing {
        dictate: false,
        command_takeover: false
    }));
}

#[test]
fn end_dictate_capture_goes_busy_without_stopping_mic_yet() {
    let open = step(IDLE, begin_with(0, true, true, true));
    let t = step(open.next, VoiceEvent::EndRequested { at: at(400) });
    assert!(matches!(t.next, VoiceState::Busy { dictate: true, .. }));
    assert!(!t.has(&VoiceEffect::StopMic));
    assert!(!t.has(&VoiceEffect::ShowThinking));
    assert!(t
        .effects
        .iter()
        .any(|e| matches!(e, VoiceEffect::BeginProcessing { .. })));
}

#[test]
fn end_when_not_capturing_is_a_noop() {
    let t = step(IDLE, VoiceEvent::EndRequested { at: at(0) });
    assert_eq!(t.next, IDLE);
    assert!(t.effects.is_empty());
}

#[test]
fn processing_finished_returns_idle_and_schedules_hide() {
    let t = step(busy(false), VoiceEvent::ProcessingFinished);
    assert_eq!(t.next, IDLE);
    assert!(t.has(&VoiceEffect::RestoreMedia));
    assert!(
        t.has(&VoiceEffect::ScheduleHideOverlay(Duration::from_millis(
            3200
        )))
    );
    assert!(!t.has(&VoiceEffect::StopMic));
}

#[test]
fn processing_finished_stops_mic_after_dictate() {
    let t = step(busy(true), VoiceEvent::ProcessingFinished);
    assert!(t.has(&VoiceEffect::StopMic));
    assert_eq!(t.next, IDLE);
}

#[test]
fn dictation_escalate_sets_takeover_and_restores_media() {
    let open = step(IDLE, begin_with(0, true, true, true));
    let t = step(open.next, VoiceEvent::DictationEscalated);
    assert_eq!(
        t.next,
        VoiceState::Capturing {
            since: at(0),
            dictate: true,
            command_takeover: true
        }
    );
    assert!(t.has(&VoiceEffect::ShowUnderstanding));
    assert!(t.has(&VoiceEffect::RestoreMedia));
}

#[test]
fn dictation_escalate_ignored_in_command_mode() {
    let open = step(IDLE, begin(0));
    let t = step(open.next, VoiceEvent::DictationEscalated);
    assert_eq!(t.next, open.next);
    assert!(!t.has(&VoiceEffect::ShowUnderstanding));
}

#[test]
fn tick_at_max_utterance_ends_capture() {
    let open = step(IDLE, begin(0));
    let still = step(open.next, VoiceEvent::Tick { at: at(5 * 60_000) });
    assert!(matches!(still.next, VoiceState::Capturing { .. }), "five minutes is no longer the end");
    let t = step(open.next, VoiceEvent::Tick { at: at(30 * 60_000) });
    assert!(matches!(t.next, VoiceState::Busy { .. }));
    assert!(t
        .effects
        .iter()
        .any(|e| matches!(e, VoiceEffect::BeginProcessing { .. })));
}

#[test]
fn tick_does_not_clear_stuck_busy() {
    let t = step(busy(false), VoiceEvent::Tick { at: at(60_000) });
    assert_eq!(t.next, busy(false));
    assert!(!t.has(&VoiceEffect::ClearStuckBusy));
}

#[test]
fn stuck_busy_timeout_is_an_explicit_idle_transition() {
    let t = step(busy(false), VoiceEvent::StuckBusyTimeout);
    assert_eq!(t.next, IDLE);
    assert!(t.has(&VoiceEffect::ClearStuckBusy));
    assert!(!t.starts_mic());
}

#[test]
fn invariant_15_start_mic_only_on_explicit_ptt_begin() {
    let not_begin = [
        VoiceEvent::CancelRequested,
        VoiceEvent::EndRequested { at: at(0) },
        VoiceEvent::ProcessingFinished,
        VoiceEvent::DictationEscalated,
        VoiceEvent::StuckBusyTimeout,
        VoiceEvent::Tick { at: at(0) },
    ];
    for evt in not_begin {
        assert!(!step(IDLE, evt).starts_mic(), "{evt:?}");
    }
    let capturing = step(IDLE, begin(0)).next;
    for evt in not_begin {
        assert!(!step(capturing, evt).starts_mic(), "{evt:?}");
    }
}

#[test]
fn duck_media_follows_config() {
    let quiet = VoiceSessionConfig {
        duck_media_during_listen: false,
        ..VoiceSessionConfig::product_defaults()
    };
    assert!(!reduce(IDLE, begin(0), &quiet).has(&VoiceEffect::DuckMedia));
}

#[test]
fn invariant_14_runscript_is_forbidden_from_voice() {
    assert!(VoiceActionPolicy::is_forbidden(Some("RunScript")));
    assert!(VoiceActionPolicy::is_forbidden(Some("runscript")));
    assert!(!VoiceActionPolicy::is_forbidden(Some("LaunchApp")));
    assert!(!VoiceActionPolicy::is_forbidden(None));
}
