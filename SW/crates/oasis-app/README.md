# oasis-app

## Overview
The hosts that run the rules: the code that *drives* `oasis-core`. Port of `SW/Oasis.Application`. Reducers are pure and forgetful; this crate owns their state across events, feeds events in, keeps the timers the reducers ask for, and hands every other effect to a sink. Keeping this in one small layer (not in UI code, as `OverlayService` once did) keeps threading and timing in one place.

For the whole life of the app, the BLE thread posts pad events; the main loop drains them, fires due timers, and sleeps until `timers().next_deadline()`.

Not ported yet: `QuickLaunch/QuickLaunchRunner.cs`, `Voice/IncrementalTranscriber.cs`, `Composition/InProcessEventBus.cs`, `OasisHost.cs`.

## Interface
- `src/timers.rs`: `TimerQueue` (`set`, `cancel`, `cancel_all`, `is_set`, `is_empty`, `next_deadline`, `take_due`). C# `ITimerScheduler`, `ManualTimerScheduler`.
- `src/arbiter_host.rs`: `ArbiterHost` (`new`, `with_claims`, `state`, `last_transition`, `timers`, `set_context`, `set_config`, `sender`, `post`, `apply`, `drain`, `fire_due`, `shutdown`), `QueuedPadEvent`. C# `Arbitration/ArbiterHost.cs`.
- `src/voice_host.rs`: `VoiceSessionHost` (`new`, `state`, `last_transition`, `sender`, `post`, `apply`, `drain`, `fire_due`, `next_deadline`). C# `Voice/VoiceSessionHost.cs`.
- `src/capabilities.rs`: `CapabilityRegistry` (`new`, `get`, `available`, `set`, `subscribe`). C# `Composition/CapabilityRegistry.cs`.
- `src/pad_link.rs`: `PadLink` (`new`, `telemetry`, `on_notification`, `on_disconnected`), `PadTelemetry`. Parsing half of C# `BleManager.cs`.
- `src/lib.rs`: module declarations; re-exports `ArbiterHost`, `CapabilityRegistry`, `PadLink`, `PadTelemetry`, `TimerQueue`, `VoiceSessionHost`.

## Invariants
- Each host is **single-writer**: only the owning thread calls `apply`, `drain`, `fire_due`. Other threads send events through `sender()` (std `mpsc`). This is the C# `Channel` + `IWorkMarshal` design without an async runtime, so the UI layer picks its own loop.
- `own_timers = false` is *shadow mode*: the host ignores timers and the caller ticks it.
- Effects reach the outside world through a closure `&mut dyn FnMut(&Effect)` (the sink); `SetTimer` / `CancelTimer` are consumed by the host.
- Setting a timer name that is already set replaces its deadline.
- A capability never probed reads as Missing ("Not probed yet.").
- `PadLink` does not care where bytes come from (real pad, recorded log, `oasis-cli` simulator).

## Side Effects
- None of its own beyond in-memory state and `mpsc` channels; no OS APIs. Depends only on `oasis-core` and `oasis-protocol`. Transport lives in `oasis-platform`.

## Verification
- Run: `cargo test -p oasis-app` (`tests/hosts.rs`: ported `ArbiterHostTests`, voice host, pad link, capability tests)
