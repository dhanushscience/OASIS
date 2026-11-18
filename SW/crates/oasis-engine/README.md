# oasis-engine

## Overview

**What:** the loop that makes the pad do things. One thread reads the pad, applies the gesture
rules, works out which profile applies to the app in front, and hands the button's action to a
worker thread. It has no window; it reports through events and a status snapshot.

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
- Actions run one at a time on a worker thread, so a long step cannot block the pad.
- **The loop never waits on slow work.** It sleeps on one inbox until something arrives (pad,
  keyboard, YouTube, a read of the app in front) or the next thing is due (a gesture timer, the
  label peek, a confirmation, the timed checks, and every 10 ms while voice records). The pad
  thread (`oasis-pad`) scans for and reads the pad; the front thread (`oasis-front`) reads what is
  in front (UI Automation). A press asks the front thread for a fresh read and is handled when the
  answer comes, in press order; a capture waits for "typed or a command?" with the mic already open.
  The YouTube gestures use the front thread's last look (at most 100 ms old). Spoken commands
  still read the app in front on the loop.

| File | Contents |
|---|---|
| `src/engine.rs` | `start`, `EngineHandle`, `EngineParts`, `EngineConfig`, `BridgeConfig`, `ForegroundProbe`, the loop, the worker |
| `src/inbox.rs` | What wakes the loop (`Input`), the pad thread and the front thread |
| `src/events.rs` | `EngineEvent`, `Press`, `EngineStatus`, `FrontApp` (all `Serialize`, for the page) |
| `src/defaults.rs` | The real Windows parts (`windows::parts()`), `detect_paths` |
| `src/voice.rs` | Push-to-talk dictation: `VoiceMic`, `VoiceParts`, `Voice`, `Heard` |
| `src/groq.rs` | Optional online speech-to-text through Groq Whisper (`transcribe`, `GroqStt`) |
| `src/polish.rs` | S1-mini sentence polish on the graphics card (`prewarm`, `apply`, `sync`) |
| `src/model_download.rs` | Resumable, SHA-256-checked voice model downloads (`download_set`) |
| `src/self_check.rs` | Times the speech and polish models on this PC (`measure`) |
| `examples/self_check.rs` | Before a release: decodes the test clip with every installed speech model and times one polish |
| `assets/voice-in.wav`, `assets/voice-out.wav` | The talk cues, made by `assets/make_cues.py` from `assets/cue-sources/` |
| `src/history.rs` | Opt-in dictation history, WAV + JSON per dictation |
| `src/quick_launch.rs` | `Runner`: opens a Quick Launch setup's items and places windows |
| `src/meetings.rs` | Google Calendar sign-in, upcoming meetings, reminders, joining |
| `src/spotify.rs` | Spotify account connection and playback commands |
| `src/reminders.rs` | Alarms, timers and reminders saved to `reminders.txt` |
| `src/use_counts.rs` | `UseCounts`: profile press counts in `profile_use.json` |
| `src/activity.rs` | `work()` guard, `busy()`, `set_voice_active`: "restart only when idle" |
| `tests/engine.rs` | Fifty-five end-to-end tests with a fake pad, fake foreground and recording backend: saving, reloading, adding, moving and deleting profiles while running, and multi-step buttons |

Not here yet: watching `profiles.json` for outside changes (reload is manual),
voice, AI. (The overlay and the YouTube bridge are in; see the notes above and below.)
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
  a playing YouTube video is paused and resumed, the recording is transcribed on its own thread
  when you let go, and the text is typed as a `PasteText` action (only when actions are on).
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
- `src/engine.rs`: `start(config, parts, on_event) -> EngineHandle` (`stop`, `status`, `set_live`, `change`, `reload_settings`, `accept_edited_settings`, `settings`), `EngineParts<T: PadTransport>`, `EngineConfig`, `BridgeConfig`, `BackendContext`, `BackendFactory`, `ForegroundProbe`, `EngineError`, `ChangeError`.
- `src/events.rs`: `EngineEvent`, `Press`, `EngineStatus`, `FrontApp`.
- `src/defaults.rs`: `windows` module (real parts), `detect_paths`.
- `src/lib.rs`: modules `activity`, `defaults`, `engine`, `events`, `groq`, `history`, `meetings`, `polish`, `model_download`, `quick_launch`, `reminders`, `self_check`, `spotify`, `use_counts`, `voice`.
- Feature modules: see the file table above; each exposes the public items named there.
- Depends on sibling crates `oasis-core`, `oasis-app`, `oasis-platform`, `oasis-settings`, `oasis-ble`, `oasis-models`, `oasis-voice`, `oasis-win` (see `Cargo.toml`); `oasis-app`, `oasis-settings` and `oasis-models` are documented at [../oasis-app/README.md](../oasis-app/README.md), [../oasis-settings/README.md](../oasis-settings/README.md), [../oasis-models/README.md](../oasis-models/README.md).

## Invariants
- Lock order is always `settings` then `store`.
- A bad settings signature: gesture rules refuse to dispatch, `set_live(true)` is refused.
- `change` is all-or-nothing: if the edit or the save fails, nothing changes anywhere.
- Actions run one at a time on a worker thread; in preview mode (`live = false`) nothing executes.
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
