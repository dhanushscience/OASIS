# oasis-engine

## Overview

**What:** the loop that makes the pad do things. One loop thread applies the gesture
rules to what the pad thread reads, works out which profile applies to the app in front, and
hands the button's action to an action worker thread. It has no window; it reports through events and a status snapshot.

**Why its own library:** the desktop app and the command line both need this exact loop. Keeping it
out of both means they cannot disagree, and it can be tested end to end with a pretend pad (the C#
app spread this wiring over `OverlayService` and `OasisHost`, which is why it could not be tested
that way).

**When:** started once at launch (`start`), stopped at quit (`EngineHandle::stop`, or just drop it).

**How:**
- `start(config, parts, on_event)` loads `profiles.json` (checking its signature), then runs.
- `EngineParts` is what the world supplies: the pad link, the foreground probe, and a factory for
  the action backend. `defaults::windows::parts()` gives the real Windows ones; tests give fakes.
- Events (`EngineEvent`) tell the watcher what happened: pad status, battery, a press and what it
  would do, a failed action, a note. `EngineHandle::status()` gives a snapshot for a status window.
- **Preview mode** (`live = false`): presses are reported with `runs: false` and nothing is
  executed. `set_live(true)` switches actions on; it is refused while the settings signature is bad.
- A bad signature makes the gesture rules refuse to dispatch at all: no press is reported and a
  note says why.
- **Changing settings while it runs:** `change(|settings| …)` applies an edit to a copy, saves it
  (atomic, signed, last-good copy) and only then puts it in force; the loop picks it up before the
  next event. If the edit is refused, or the save fails (the file was changed by another program,
  the signature is untrusted, the disk write failed), nothing changes anywhere.
  `reload_settings()` reads the file again; `accept_edited_settings()` keeps a file that was edited
  outside OASIS (the user's choice). `settings()` returns a copy of what is in force.
- **Label peek (for an overlay):** when labels are on for the active profile (general setting plus the
  profile's own, `LabelPolicy::effective`), the first tap on a button that has an action emits
  `ShowLabels` and runs nothing; a tap while they show runs it and emits `HideLabels`; they also
  hide by themselves after 4 s. `PadButton { button, down }` reports every button going down or up.
  The engine never draws; the desktop app's overlay window does.
- Lock order is always `settings` then `store`, so the two cannot deadlock.
- `OASIS_DATA_DIR` (see `defaults::detect_paths`) points the engine at a folder other than the real
  one, for rehearsing on a copy.
- **Threads.** Each has one owner and one job:
  - `oasis-engine`: the loop (`run_loop`). The only writer of the gesture rules, the profile choice
    and the voice session. Blocks on one inbox (`Input`, in `src/inbox.rs`) with `recv_timeout`,
    waking at the next deadline (a gesture timer, the label peek, a confirmation, the meeting,
    reminder and settings-file checks, the memory log, and every 10 ms while voice records).
  - `oasis-pad`: owns the `PadConnector`. Scans, connects, reads, and posts each event as
    `Input::Pad`. The loop switches it on and off over a small channel (pad off lets go of the pad).
  - `oasis-front`: reads what is in front (UI Automation, up to a second) and whether a text field
    has focus (up to 250 ms). Posts `Input::Front` when it changed (it looks every 100 ms) and
    answers each ask from the loop (`FrontAsk`), one at a time, in order. A press asks for a fresh
    read and is handled when `Input::ForPress` comes back, in press order; a capture waits for
    "typed or a command?" (`Input::Capture`) with the mic already open. The YouTube gestures use
    the front thread's last look (at most 100 ms old).
  - `oasis-actions` (ordered lane) and `oasis-actions-free` (focus-free jobs only, so a slow "open
    app" does not hold up a volume key or a timer). Each runs one job at a time. A job with
    `prepare` always goes to the ordered lane: a dictation's cleanup and the S1-mini polish run
    there, so the loop never waits on them and a later press still runs after the paste.
  - `oasis-speech` (in `src/voice.rs`): transcribes recordings. The loop polls its results.
  - Short-lived threads: the Google Calendar refresh (every 60 s, because the network can be
    slow), polish model start-up and downloads.
  - The foreground probe is shared (`SharedProbe`, a mutex held for one read), because the loop
    also reads it for spoken commands.
- **Outside edits of `profiles.json`:** `oasis_win::watch::FolderWatch` (Windows only) posts
  `Input::SettingsFileChanged` when something is written in the settings folder, and the loop
  checks the file at once. A 30 s safety-net check covers a missed change. If the watcher cannot
  start, a note goes to the security log and the file is checked every 1.5 s instead.
- Stopping: `stop` sets a flag and wakes the loop; the loop ends the pad and front threads, then
  drops the job senders, which ends both action workers, and emits `Stopped`.
- Actions never run on the loop, and the loop never waits on slow work.

| File | Contents |
|---|---|
| `src/engine.rs` | The loop (`run_loop`) and its small helpers; re-exports the public items of `engine/` (see [src/engine/README.md](src/engine/README.md)) |
| `src/engine/parts.rs` | What the engine is given: `EngineParts`, `EngineConfig`, `BridgeConfig`, `ForegroundProbe`, `BackendContext`, `BackendFactory`, `EngineError`, `ChangeError` |
| `src/engine/handle.rs` | The shared state (`Shared`), `EngineHandle`, `start` |
| `src/engine/dispatch.rs` | A recognised gesture: the label peek, `dispatch`, `plan` |
| `src/engine/worker.rs` | The two action workers (`Jobs`, `spawn_worker`) and the meeting, Spotify and Quick Launch runners |
| `src/engine/spoken.rs` | A spoken phrase (`handle_heard`): typed, run, asked on the pad or refused; spoken alarms, timers, reminders |
| `src/engine/dictation.rs` | Dictation: cleanup, polish, trace log, talk cues, `paste_typed`, `dictation_style` |
| `src/engine/bridge.rs` | The browser-extension socket (`sync_bridge`) and preferences |
| `src/engine/layouts.rs` | Spoken layouts: save and restore open windows as a Quick Launch setup |
| `src/inbox.rs` | What wakes the loop (`Input`), the pad thread (`spawn_pad`), the front thread (`spawn_front`), `SharedProbe` |
| `src/events.rs` | `EngineEvent`, `Press`, `EngineStatus`, `FrontApp` (all `Serialize`, for the page) |
| `src/defaults.rs` | The real Windows parts (`windows::parts()`), `detect_paths` |
| `src/voice.rs` | Push-to-talk dictation: `VoiceMic`, `VoiceParts`, `Voice`, `Heard` |
| `src/groq.rs` | Optional online speech-to-text through Groq Whisper (`transcribe`, `GroqStt`) |
| `src/polish.rs` | S1-mini sentence polish on the graphics card (`prewarm`, `apply`, `sync`) |
| `src/model_download.rs` | Resumable, SHA-256-checked voice model downloads (`download_set`) |
| `src/self_check.rs` | Times the speech and polish models on this PC (`measure`) |
| `examples/self_check.rs` | Before a release: decodes the test clip with every installed speech model and times one polish |
| `assets/voice-in.wav`, `assets/voice-out.wav`, `assets/voice-wake.wav` | The talk cues, made by `assets/make_cues.py` from `assets/cue-sources/` |
| `assets/selfcheck-16k.wav`, `assets/selfcheck.txt` | The test clip and its text, used by `self_check` |
| `src/history.rs` | Opt-in dictation history, WAV + JSON per dictation |
| `src/quick_launch.rs` | `Runner`: opens a Quick Launch setup's items and places windows |
| `src/meetings.rs` | Google Calendar sign-in, upcoming meetings, reminders, joining |
| `src/spotify.rs` | Spotify account connection and playback commands |
| `src/reminders.rs` | Alarms, timers and reminders saved to `reminders.txt` |
| `src/use_counts.rs` | `UseCounts`: profile press counts in `profile_use.json` |
| `src/activity.rs` | `work()` guard, `busy()`, `set_voice_active`: "restart only when idle" |
| `tests/engine.rs` | End-to-end tests (about 109) with a fake pad, fake foreground and recording backend: saving, reloading, adding, moving and deleting profiles while running, and multi-step buttons |

Voice (push-to-talk, spoken commands, dictation polish) and the overlay and YouTube bridge are in; see the notes below. The engine has no AI features beyond the S1-mini sentence polish and optional Groq speech-to-text.

- **Browser extension (`BridgeConfig`):** with a token the engine runs the extension's socket
  (when `BrowserBridgeEnabled`), keeps a `YouTubeWatchState` from its events, and passes the
  state's token to profile matching so a `youtube.playing` set of buttons is chosen. On each
  button-down for hold-2× or a volume button it checks that a playing YouTube watch page is in
  front and tells the arbiter; `SendToDevice` effects become `oasis.command` messages; a
  double-click on a volume button runs `VolumeUp`/`VolumeDown`. Events `BridgeStatus` and
  `YouTube`. `OASIS_BRIDGE_PORT` overrides the port, for rehearsing beside a running OASIS.
- **Push-to-talk (`VoiceParts`):** with a microphone, a speech model check and a speech-engine
  loader passed in, the engine runs `oasis_core::voice` for dictation: on button-down of a
  push-to-talk button it starts listening (pre-roll), a capture begins when the gesture rules say,
  a playing YouTube video is paused and resumed, the recording is transcribed on the `oasis-speech` thread
  as you speak (pieces) and when you let go, and the text is cleaned and polished on the action worker (`Job.prepare`) and typed as a `PasteText` action (only when actions are on).
  Events `Voice { phase }` (`listening`, `dictating`, `thinking`, `idle`) and `Transcript`.
  Status `voice`: `ready`, `off`, `no_model`, `unavailable`.
- **Spoken commands:** each recording is a typed dictation or a command (capture mode, the
  dictate-first list, a focus probe). A command goes through `oasis_core::voice::command::route`
  against a vocabulary built from the profiles and the running programs, then the safety policy:
  refused, run now, or asked on the pad (`Confirm` event; button 1 runs and remembers the target,
  button 2 cancels, 30 s timeout, `ConfirmEnd`). `VoiceCommand { heard, label, outcome, detail }`
  reports the result. A script never runs by voice.
- **Panels:** `ForegroundProbe::set_panel_profiles_exist` tells the OS layer whether any profile has a
  set of buttons for a panel or window title (`has_panel_profiles`); `current_for_press` then reads
  which part of the program has the focus at each press (terminal, editor, AI chat, …) and the
  profile matcher chooses that set. `crates/oasis-platform/examples/panel_probe.rs` prints it live.

## Interface
- `src/engine.rs` (re-exports; items in `src/engine/handle.rs`, `src/engine/parts.rs`): `start(config, parts, on_event) -> EngineHandle` (`stop`, `status`, `set_live`, `change`, `reload_settings`, `accept_edited_settings`, `settings`, `try_button`, `profile_use_counts`, `paths`, `meetings`, `answer_meeting_reminder`, `hold_keys`), `EngineParts<T: PadTransport>`, `EngineConfig`, `BridgeConfig`, `BackendContext`, `BackendFactory`, `ForegroundProbe`, `EngineError`, `ChangeError`.
- `src/events.rs`: `EngineEvent`, `Press`, `EngineStatus`, `FrontApp`.
- `src/defaults.rs`: `windows` module (real parts), `detect_paths`.
- `src/lib.rs`: modules `activity`, `defaults`, `engine`, `events`, `groq`, `history`, `meetings`, `polish`, `model_download`, `quick_launch`, `reminders`, `self_check`, `spotify`, `use_counts`, `voice`.
- Feature modules: see the file table above; each exposes the public items named there.
- Depends on sibling crates `oasis-core`, `oasis-app`, `oasis-platform`, `oasis-settings`, `oasis-ble`, `oasis-models`, `oasis-voice`, `oasis-win` (see `Cargo.toml`); `oasis-app`, `oasis-settings` and `oasis-models` are documented at [../oasis-app/README.md](../oasis-app/README.md), [../oasis-settings/README.md](../oasis-settings/README.md), [../oasis-models/README.md](../oasis-models/README.md).

## Invariants
- Lock order is always `settings` then `store`.
- A bad settings signature: gesture rules refuse to dispatch, `set_live(true)` is refused.
- `change` is all-or-nothing: if the edit or the save fails, nothing changes anywhere.
- Actions run one at a time per lane (ordered and focus-free), never on the loop; in preview mode (`live = false`) nothing executes.
- The engine never draws; overlay and windows belong to the desktop app.
- A script never runs by voice; a voice command is refused, run, or confirmed on the pad (30 s timeout).
- Model downloads are kept as `<name>.part` and renamed only after the SHA-256 check passes.

## Side Effects
- Reads and writes `profiles.json` (atomic, signed, last-good copy), `profile_use.json`, `reminders.txt`, history WAV/JSON files, and downloaded model files.
- Network: Groq (only when chosen), Google Calendar, Spotify, GitHub model downloads, the browser-extension socket on `127.0.0.1`.
- Spawns a llama.cpp server for polish and self-check; unpacks zips with `tar.exe`; uses the microphone and OS input/window APIs through the supplied parts.
- Environment: `OASIS_DATA_DIR`, `OASIS_BRIDGE_PORT`, `OASIS_SHERPA_DIR`, `OASIS_STT_DIR`, `OASIS_SPOTIFY_CLIENT_ID`, `OASIS_GOOGLE_CLIENT_ID`, `OASIS_GOOGLE_CLIENT_SECRET`.

## Verification
- Run: `cargo test -p oasis-engine`
- Before a release: `cargo run -p oasis-engine --example self_check`
