//! Port of `VoiceReducer.cs`, `VoiceState.cs`, `VoiceEvent.cs`, `VoiceEffect.cs`,
//! `VoiceSessionConfig.cs`, `VoiceTransition.cs`.
//!
//! Pure. The C# `VoiceSessionService` kept this as a handful of booleans (`_capturing`,
//! `_busy`, `_dictateThisUtterance`, …); here it is one enum so impossible combinations cannot
//! be represented.

use std::time::Duration;

use crate::primitives::Instant;

pub const MAX_CAPTURE_TIMER: &str = "voice-max-capture";
/// The longest one recording may run.
pub const MAX_UTTERANCE: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceSessionConfig {
    /// A session busy for longer than this is assumed stuck and is cleared on the next begin.
    pub stuck_busy_timeout: Duration,
    pub command_max_utterance: Duration,
    pub dictate_max_utterance: Duration,
    pub duck_media_during_listen: bool,
    pub hide_overlay_delay: Duration,
}

impl VoiceSessionConfig {
    pub fn product_defaults() -> VoiceSessionConfig {
        VoiceSessionConfig {
            stuck_busy_timeout: Duration::from_secs(45),
            // Long dictation (a locked, hands-free talk) ran into a 5-minute cap at 4:59. The audio is
            // transcribed in pieces as it comes, so a long recording costs no more memory or wait at
            // the end than a short one; the cap is only a guard against a recording left on by mistake.
            // A command is as long: with no text field found, a long talk is still taken as one.
            command_max_utterance: MAX_UTTERANCE,
            dictate_max_utterance: MAX_UTTERANCE,
            duck_media_during_listen: true,
            hide_overlay_delay: Duration::from_millis(3200),
        }
    }

    fn max_utterance(&self, dictate: bool) -> Duration {
        if dictate {
            self.dictate_max_utterance
        } else {
            self.command_max_utterance
        }
    }
}

impl Default for VoiceSessionConfig {
    fn default() -> Self {
        VoiceSessionConfig::product_defaults()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    Idle,
    /// Mic open. `dictate`: this utterance is dictation, not a command. `command_takeover`: live
    /// dictation heard a command mid-hold and will run it as one.
    Capturing {
        since: Instant,
        dictate: bool,
        command_takeover: bool,
    },
    /// Transcribing / planning / executing the utterance.
    Busy {
        since: Instant,
        dictate: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceEvent {
    /// PTT begin, after the pad reducer matched the exact push-to-talk mask.
    BeginRequested {
        at: Instant,
        enabled: bool,
        model_ready: bool,
        dictate: bool,
    },
    CancelRequested,
    EndRequested {
        at: Instant,
    },
    ProcessingFinished,
    /// Live dictation classified a phrase as a command mid-hold.
    DictationEscalated,
    /// Explicit stuck-busy watchdog timeout.
    StuckBusyTimeout,
    Tick {
        at: Instant,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceEffect {
    StartMic,
    StopMic,
    SetTimer {
        name: &'static str,
        delay: Duration,
    },
    CancelTimer(&'static str),
    DuckMedia,
    RestoreMedia,
    ShowListening,
    ShowDictating,
    ShowThinking,
    ShowUnderstanding,
    HideOverlay,
    ScheduleHideOverlay(Duration),
    ClearStuckBusy,
    BeginProcessing {
        dictate: bool,
        command_takeover: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceTransition {
    pub next: VoiceState,
    pub effects: Vec<VoiceEffect>,
}

impl VoiceTransition {
    fn to(next: VoiceState) -> VoiceTransition {
        VoiceTransition {
            next,
            effects: Vec::new(),
        }
    }

    fn with(next: VoiceState, effects: Vec<VoiceEffect>) -> VoiceTransition {
        VoiceTransition { next, effects }
    }

    pub fn has(&self, effect: &VoiceEffect) -> bool {
        self.effects.contains(effect)
    }

    /// Invariant 15: the mic starts only on a successful PTT begin.
    pub fn starts_mic(&self) -> bool {
        self.has(&VoiceEffect::StartMic)
    }
}

pub fn reduce(state: VoiceState, evt: VoiceEvent, config: &VoiceSessionConfig) -> VoiceTransition {
    match evt {
        VoiceEvent::BeginRequested {
            at,
            enabled,
            model_ready,
            dictate,
        } => on_begin(state, at, enabled, model_ready, dictate, config),
        VoiceEvent::CancelRequested => on_cancel(state),
        VoiceEvent::EndRequested { at } => on_end(state, at),
        VoiceEvent::ProcessingFinished => on_finished(state, config),
        VoiceEvent::DictationEscalated => on_escalated(state),
        VoiceEvent::StuckBusyTimeout => on_stuck_timeout(state),
        VoiceEvent::Tick { at } => on_tick(state, at, config),
    }
}

fn on_begin(
    state: VoiceState,
    at: Instant,
    enabled: bool,
    model_ready: bool,
    dictate: bool,
    config: &VoiceSessionConfig,
) -> VoiceTransition {
    match state {
        VoiceState::Capturing { .. } => VoiceTransition::to(state),
        VoiceState::Busy { since, .. } => {
            if at.since(since) <= config.stuck_busy_timeout {
                return VoiceTransition::to(state);
            }
            let recovered =
                begin_from_idle(VoiceState::Idle, at, enabled, model_ready, dictate, config);
            let mut effects = vec![VoiceEffect::ClearStuckBusy];
            effects.extend(recovered.effects);
            VoiceTransition::with(recovered.next, effects)
        }
        VoiceState::Idle => begin_from_idle(state, at, enabled, model_ready, dictate, config),
    }
}

fn begin_from_idle(
    _state: VoiceState,
    at: Instant,
    enabled: bool,
    model_ready: bool,
    dictate: bool,
    config: &VoiceSessionConfig,
) -> VoiceTransition {
    if !enabled || !model_ready {
        return VoiceTransition::to(VoiceState::Idle);
    }

    let mut effects = vec![
        VoiceEffect::StartMic,
        VoiceEffect::SetTimer {
            name: MAX_CAPTURE_TIMER,
            delay: config.max_utterance(dictate),
        },
    ];
    if config.duck_media_during_listen {
        effects.push(VoiceEffect::DuckMedia);
    }
    effects.push(if dictate {
        VoiceEffect::ShowDictating
    } else {
        VoiceEffect::ShowListening
    });

    VoiceTransition::with(
        VoiceState::Capturing {
            since: at,
            dictate,
            command_takeover: false,
        },
        effects,
    )
}

fn on_cancel(state: VoiceState) -> VoiceTransition {
    if let VoiceState::Capturing { .. } = state {
        return VoiceTransition::with(
            VoiceState::Idle,
            vec![
                VoiceEffect::CancelTimer(MAX_CAPTURE_TIMER),
                VoiceEffect::StopMic,
                VoiceEffect::RestoreMedia,
                VoiceEffect::HideOverlay,
            ],
        );
    }
    // Not capturing: hide the chrome, leave busy latched (VoiceSessionService.CancelCapture).
    VoiceTransition::with(
        state,
        vec![VoiceEffect::RestoreMedia, VoiceEffect::HideOverlay],
    )
}

fn on_end(state: VoiceState, at: Instant) -> VoiceTransition {
    let VoiceState::Capturing {
        dictate,
        command_takeover,
        ..
    } = state
    else {
        return VoiceTransition::to(state);
    };

    let mut effects = vec![VoiceEffect::CancelTimer(MAX_CAPTURE_TIMER)];
    if !dictate {
        effects.push(VoiceEffect::StopMic);
        effects.push(VoiceEffect::ShowThinking);
    }
    effects.push(VoiceEffect::BeginProcessing {
        dictate,
        command_takeover,
    });
    VoiceTransition::with(VoiceState::Busy { since: at, dictate }, effects)
}

fn on_finished(state: VoiceState, config: &VoiceSessionConfig) -> VoiceTransition {
    let VoiceState::Busy { dictate, .. } = state else {
        return VoiceTransition::to(state);
    };

    let mut effects = Vec::new();
    if dictate {
        // Dictation keeps the mic open through processing (live transcript); close it now.
        effects.push(VoiceEffect::StopMic);
    }
    effects.push(VoiceEffect::RestoreMedia);
    effects.push(VoiceEffect::ScheduleHideOverlay(config.hide_overlay_delay));
    VoiceTransition::with(VoiceState::Idle, effects)
}

fn on_escalated(state: VoiceState) -> VoiceTransition {
    match state {
        VoiceState::Capturing {
            since,
            dictate: true,
            ..
        } => VoiceTransition::with(
            VoiceState::Capturing {
                since,
                dictate: true,
                command_takeover: true,
            },
            vec![VoiceEffect::ShowUnderstanding, VoiceEffect::RestoreMedia],
        ),
        _ => VoiceTransition::to(state),
    }
}

fn on_stuck_timeout(state: VoiceState) -> VoiceTransition {
    match state {
        VoiceState::Busy { .. } => {
            VoiceTransition::with(VoiceState::Idle, vec![VoiceEffect::ClearStuckBusy])
        }
        _ => VoiceTransition::to(state),
    }
}

fn on_tick(state: VoiceState, at: Instant, config: &VoiceSessionConfig) -> VoiceTransition {
    if let VoiceState::Capturing { since, dictate, .. } = state {
        if at.is_at_or_after(since.plus(config.max_utterance(dictate))) {
            return on_end(state, at);
        }
    }
    // The watchdog does not fire on Tick — only on the next begin or an explicit timeout.
    VoiceTransition::to(state)
}
