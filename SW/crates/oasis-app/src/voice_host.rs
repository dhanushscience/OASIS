//! Port of `Voice/VoiceSessionHost.cs`: the single owner of the voice reducer's state.
//! Same shape as [`crate::ArbiterHost`]: timers kept here, other effects go to the sink.

use std::sync::mpsc::{channel, Receiver, Sender};

use oasis_core::voice::{
    reduce, VoiceEffect, VoiceEvent, VoiceSessionConfig, VoiceState, VoiceTransition,
};
use oasis_core::Instant;

use crate::timers::TimerQueue;

pub struct VoiceSessionHost {
    state: VoiceState,
    last: Option<VoiceTransition>,
    config: VoiceSessionConfig,
    own_timers: bool,
    timers: TimerQueue,
    tx: Sender<VoiceEvent>,
    rx: Receiver<VoiceEvent>,
}

impl VoiceSessionHost {
    pub fn new(config: VoiceSessionConfig, own_timers: bool) -> VoiceSessionHost {
        let (tx, rx) = channel();
        VoiceSessionHost {
            state: VoiceState::Idle,
            last: None,
            config,
            own_timers,
            timers: TimerQueue::new(),
            tx,
            rx,
        }
    }

    pub fn state(&self) -> VoiceState {
        self.state
    }

    pub fn last_transition(&self) -> Option<&VoiceTransition> {
        self.last.as_ref()
    }

    pub fn sender(&self) -> Sender<VoiceEvent> {
        self.tx.clone()
    }

    pub fn post(&self, evt: VoiceEvent) {
        let _ = self.tx.send(evt);
    }

    pub fn apply(
        &mut self,
        evt: VoiceEvent,
        now: Instant,
        sink: &mut dyn FnMut(&VoiceEffect),
    ) -> &VoiceTransition {
        let t = reduce(self.state, evt, &self.config);
        self.state = t.next;
        for effect in &t.effects {
            match effect {
                VoiceEffect::SetTimer { name, delay } => {
                    if self.own_timers {
                        self.timers.set(name, now, *delay);
                    }
                }
                VoiceEffect::CancelTimer(name) => {
                    if self.own_timers {
                        self.timers.cancel(name);
                    }
                }
                other => sink(other),
            }
        }
        self.last.insert(t)
    }

    pub fn drain(&mut self, now: Instant, sink: &mut dyn FnMut(&VoiceEffect)) {
        while let Ok(evt) = self.rx.try_recv() {
            self.apply(evt, now, sink);
        }
    }

    pub fn fire_due(&mut self, now: Instant, sink: &mut dyn FnMut(&VoiceEffect)) {
        if !self.own_timers {
            return;
        }
        for _ in self.timers.take_due(now) {
            self.apply(VoiceEvent::Tick { at: now }, now, sink);
        }
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.timers.next_deadline()
    }
}
