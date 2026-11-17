//! Port of `Composition/CapabilityRegistry.cs`.
//!
//! Adapters write what they found (`set`); the UI and planners only read (`get`). Anything
//! never probed reads as Missing("Not probed yet."), so the UI shows an honest status instead
//! of assuming a feature works.

use std::collections::BTreeMap;
use std::sync::Mutex;

use oasis_core::capability::{Capability, CapabilityState};

type Listener = Box<dyn Fn(Capability, &CapabilityState) + Send + Sync>;

#[derive(Default)]
pub struct CapabilityRegistry {
    states: Mutex<BTreeMap<Capability, CapabilityState>>,
    listeners: Mutex<Vec<Listener>>,
}

impl CapabilityRegistry {
    pub fn new() -> CapabilityRegistry {
        CapabilityRegistry::default()
    }

    pub fn get(&self, capability: Capability) -> CapabilityState {
        self.states
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&capability)
            .cloned()
            .unwrap_or_else(|| CapabilityState::missing("Not probed yet."))
    }

    /// Available capabilities in declaration order.
    pub fn available(&self) -> Vec<Capability> {
        self.states
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|(_, s)| s.is_available())
            .map(|(c, _)| *c)
            .collect()
    }

    /// Record a state. Listeners hear about it only when it actually changed (the C# registry
    /// publishes `CapabilityChanged` on the event bus under the same rule).
    pub fn set(&self, capability: Capability, state: CapabilityState) -> bool {
        {
            let mut states = self.states.lock().unwrap_or_else(|e| e.into_inner());
            if states.get(&capability) == Some(&state) {
                return false;
            }
            states.insert(capability, state.clone());
        }
        for listener in self
            .listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
        {
            listener(capability, &state);
        }
        true
    }

    pub fn subscribe(
        &self,
        listener: impl Fn(Capability, &CapabilityState) + Send + Sync + 'static,
    ) {
        self.listeners
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Box::new(listener));
    }
}
