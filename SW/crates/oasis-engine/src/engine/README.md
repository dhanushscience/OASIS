# oasis-engine/src/engine

## Overview

**What:** the engine split by feature. `../engine.rs` keeps the loop (`run_loop`) and its small helpers; everything the loop calls lives here, one file per feature.

**Why split:** One file for everything was hard to read. Each file now holds one idea, and all of them share the loop's imports through `use super::*;`.

**When:** compiled as child modules of `engine`. Only the items re-exported by `../engine.rs` (`start`, `EngineHandle`, `EngineParts`, `EngineConfig`, `BridgeConfig`, `BackendContext`, `BackendFactory`, `ForegroundProbe`, `EngineError`, `ChangeError`) and `dictation_style` (crate-wide) leave the module. Everything else is `pub(super)`.

**How:**
- `parts.rs`: what the world hands the engine (pad link, foreground probe, backend factory, voice, buttons, Quick Launch) and the two error types.
- `handle.rs`: `Shared`, the state the loop, the front thread and both action workers share (settings, store, status, live flag, stop flag, generation counter, the inbox sender). `EngineHandle` is the caller's grip on it. `start` loads `profiles.json`, creates the two job channels, spawns the two action workers (`oasis-actions`, `oasis-actions-free`) and the loop thread (`oasis-engine`). The loop itself spawns the pad and front threads.
- `dispatch.rs`: a recognised gesture becomes a label peek or an action (`peek_labels`, `dispatch`, `plan`).
- `worker.rs`: `Job`, `Jobs` (the two lanes) and `spawn_worker`. A job with `prepare` runs that first and always takes the ordered lane.
- `spoken.rs`: `handle_heard` decides whether a phrase is typed, run, asked on the pad, or refused. Also spoken alarms, timers and reminders, and the 30 s confirmation timeout.
- `dictation.rs`: `type_dictation` queues a job whose `prepare` does cleanup, S1-mini polish, history and the trace log. Also the talk cues (`cue`) and `paste_typed`.
- `bridge.rs`: starts, moves, updates or stops the browser-extension socket from the settings; pushes the preferred browser to the action backend.
- `layouts.rs`: "save this layout" and "restore layout" as a Quick Launch setup named "<name> layout".

## Interface

| File | Items |
|---|---|
| `parts.rs` | `ForegroundProbe`, `BackendContext`, `BackendFactory`, `EngineParts`, `EngineConfig`, `BridgeConfig`, `EngineError`, `ChangeError` (all public) |
| `handle.rs` | `Shared`, `Emit`, `EngineHandle`, `start`, `load_settings` |
| `dispatch.rs` | `is_runnable`, `peek_labels`, `labels_possible`, `Gesture`, `dispatch`, `plan` |
| `worker.rs` | `Job`, `Jobs`, `spawn_worker`, `run_job_action`, `JoinAware`, `join_meeting_now`, `run_spotify`, `run_quick_launch`, `report_failure` |
| `spoken.rs` | `Pending`, `CONFIRM_TIMEOUT`, `SpokenContext`, `handle_heard`, `is_oasis`, `starts_a_shell`, `local_now`, `handle_reminder_action`, `fire_due_reminders`, `run_voice_action`, `close_target` |
| `dictation.rs` | `dictation_style`, `type_dictation`, `dictation_text`, `write_dictation_trace`, `cue`, `VOICE_IN`, `VOICE_OUT`, `VOICE_WAKE`, `paste_typed`, `show_typed`, `clean_dictation` |
| `bridge.rs` | `is_youtube_gesture_button`, `sync_preferences`, `sync_bridge` |
| `layouts.rs` | `layout_name`, `run_layout` |

## Invariants
- Lock order is always `settings` then `store`.
- The loop thread is the only caller of `dispatch`, `handle_heard` and `sync_bridge`. Actions run only on the two worker threads, never on the loop.
- The ordered lane runs one job at a time, in the order sent. The focus-free lane takes only jobs that do not depend on which window is in front and have no `prepare`.
- `Job.prepare` runs on the worker, in the job's place in the queue. If it returns `None`, nothing is pasted and the job ends.
- Both workers build the backend from the same factory; its mutex is held only for that call.
- A bad settings signature (`integrity_ok` false) forces `live` off; dictation is then shown, not typed.

## Side Effects
- `dictation.rs`: writes dictation history (when on), `dictation-trace.log` (when on), the security log, and may put text on the clipboard (`oasis_win::clipboard::set_text`) when no text field was found.
- `worker.rs`: runs actions through the backend, calls Spotify and Quick Launch, writes the security log.
- `bridge.rs`: starts or stops a socket on `127.0.0.1`, publishes the browser extension folder.
- `handle.rs`: reads and writes `profiles.json` through the store; emits events through the callback given to `start`.

## Verification
- Run: `cargo test -p oasis-engine` (unit tests in `layouts.rs` and `spoken.rs`; end-to-end tests in `../../tests/engine.rs`).
