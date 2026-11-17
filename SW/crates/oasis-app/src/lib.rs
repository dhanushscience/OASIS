//! # oasis-app
//!
//! Port of `SW/Oasis.Application`. Reducers in `oasis-core` only *ask* for timers and actions;
//! the hosts here own the state, keep the timers and hand every other effect to a sink.
//!
//! | Module | C# origin | Role |
//! |---|---|---|
//! | [`timers`] | `ITimerScheduler`, `ManualTimerScheduler` | Named deadlines the host polls |
//! | [`arbiter_host`] | `Arbitration/ArbiterHost.cs` | Runs the pad reducer |
//! | [`voice_host`] | `Voice/VoiceSessionHost.cs` | Runs the voice reducer |
//! | [`capabilities`] | `Composition/CapabilityRegistry.cs` | What optional features are usable |
//! | [`pad_link`] | glue from `BleManager` | Turns protocol messages into pad events + telemetry |
//!
//! Threading model: each host is a single-writer object. Other threads send it events through
//! the `Sender` returned by `sender()`; the owning thread calls `drain` and `fire_due`. This is
//! the C# `Channel` + `IWorkMarshal` design without an async runtime, which keeps this crate
//! dependency-free and lets the UI layer pick its own event loop later.

pub mod arbiter_host;
pub mod capabilities;
pub mod pad_link;
pub mod timers;
pub mod voice_host;

pub use arbiter_host::ArbiterHost;
pub use capabilities::CapabilityRegistry;
pub use pad_link::{PadLink, PadTelemetry};
pub use timers::TimerQueue;
pub use voice_host::VoiceSessionHost;
