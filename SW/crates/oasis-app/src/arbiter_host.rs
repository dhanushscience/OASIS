//! Port of `Arbitration/ArbiterHost.cs`: the single owner of the pad reducer's state.
//!
//! - `own_timers = true` (normal): the host keeps the reducer's timers in a [`TimerQueue`]
//!   and turns each expiry into `PadEvent::Tick(now)`.
//! - `own_timers = false` (shadow mode): timers are ignored, the caller posts ticks itself.
//!   Kept because the C# app used it to run the reducer beside the old arbiter and compare.
//!
//! `SetTimer` / `CancelTimer` are consumed here; every other effect goes to the sink.

use std::sync::mpsc::{channel, Receiver, Sender};

use oasis_core::input::{
    default_claims, reduce, ArbiterConfig, ArbiterContext, ArbiterEffect, GestureClaim, PadEvent,
    PadState, Transition,
};
use oasis_core::Instant;

use crate::timers::TimerQueue;

/// An event queued from another thread, optionally replacing config/context (C# `Queued`).
#[derive(Debug, Clone)]
pub struct QueuedPadEvent {
    pub event: PadEvent,
    pub config: Option<ArbiterConfig>,
    pub context: Option<ArbiterContext>,
}

impl From<PadEvent> for QueuedPadEvent {
    fn from(event: PadEvent) -> Self {
        QueuedPadEvent {
            event,
            config: None,
            context: None,
        }
    }
}

pub struct ArbiterHost {
    state: PadState,
    last: Transition,
    config: ArbiterConfig,
    context: ArbiterContext,
    claims: Vec<Box<dyn GestureClaim>>,
    own_timers: bool,
    timers: TimerQueue,
    tx: Sender<QueuedPadEvent>,
    rx: Receiver<QueuedPadEvent>,
}

impl ArbiterHost {
    pub fn new(config: ArbiterConfig, own_timers: bool) -> ArbiterHost {
        ArbiterHost::with_claims(config, default_claims(), own_timers)
    }

    pub fn with_claims(
        config: ArbiterConfig,
        claims: Vec<Box<dyn GestureClaim>>,
        own_timers: bool,
    ) -> ArbiterHost {
        let (tx, rx) = channel();
        ArbiterHost {
            state: PadState::Idle,
            last: Transition::to(PadState::Idle),
            config,
            context: ArbiterContext::desktop(),
            claims,
            own_timers,
            timers: TimerQueue::new(),
            tx,
            rx,
        }
    }

    pub fn state(&self) -> &PadState {
        &self.state
    }

    pub fn last_transition(&self) -> &Transition {
        &self.last
    }

    pub fn timers(&self) -> &TimerQueue {
        &self.timers
    }

    pub fn set_context(&mut self, context: ArbiterContext) {
        self.context = context;
    }

    pub fn set_config(&mut self, config: ArbiterConfig) {
        self.config = config;
    }

    /// A handle other threads use to queue events (BLE reader, UI).
    pub fn sender(&self) -> Sender<QueuedPadEvent> {
        self.tx.clone()
    }

    pub fn post(&self, event: impl Into<QueuedPadEvent>) {
        // The receiver lives as long as `self`, so this cannot fail.
        let _ = self.tx.send(event.into());
    }

    /// Apply one event now. `now` is used to schedule any timers the reducer asks for.
    pub fn apply(
        &mut self,
        event: PadEvent,
        now: Instant,
        config: Option<ArbiterConfig>,
        context: Option<ArbiterContext>,
        sink: &mut dyn FnMut(&ArbiterEffect),
    ) -> &Transition {
        if let Some(c) = config {
            self.config = c;
        }
        if let Some(c) = context {
            self.context = c;
        }
        let transition = reduce(
            &self.state,
            event,
            &self.config,
            &self.context,
            &self.claims,
        );
        self.state = transition.next.clone();
        self.interpret(&transition, now, sink);
        self.last = transition;
        &self.last
    }

    /// Apply everything queued so far, in order.
    pub fn drain(&mut self, now: Instant, sink: &mut dyn FnMut(&ArbiterEffect)) {
        while let Ok(q) = self.rx.try_recv() {
            self.apply(q.event, now, q.config, q.context, sink);
        }
    }

    /// Fire every owned timer due at `now` as a `Tick(now)`, one tick per timer, like the C#
    /// host does when each scheduled callback runs.
    pub fn fire_due(&mut self, now: Instant, sink: &mut dyn FnMut(&ArbiterEffect)) {
        if !self.own_timers {
            return;
        }
        for _ in self.timers.take_due(now) {
            self.apply(PadEvent::Tick(now), now, None, None, sink);
        }
    }

    pub fn shutdown(&mut self) {
        self.timers.cancel_all();
    }

    fn interpret(
        &mut self,
        transition: &Transition,
        now: Instant,
        sink: &mut dyn FnMut(&ArbiterEffect),
    ) {
        for effect in &transition.effects {
            match effect {
                ArbiterEffect::SetTimer { name, delay } => {
                    if self.own_timers {
                        self.timers.set(name, now, *delay);
                    }
                }
                ArbiterEffect::CancelTimer(name) => {
                    if self.own_timers {
                        self.timers.cancel(name);
                    }
                }
                other => sink(other),
            }
        }
    }
}
