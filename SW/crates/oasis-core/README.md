# `oasis-core` Blueprint

## System Overview
**What:** the pure decision logic of OASIS. Given what just happened (a button went down, a
timer fired, the extension said a video ended), it decides what that means and what should
happen next. Port of `SW/Oasis.Core`.

**Why it exists separately:** this is where the hard-won behaviour lives: gesture precedence,
the YouTube end-of-video detection, ad-skip-safe media ducking. Keeping it free of I/O means:
- every rule is tested in milliseconds without a pad, a window or a microphone;
- the same rules run identically on Windows, macOS and Linux;
- a bug in an OS backend cannot change what a gesture means.

**When it runs:** on every pad event, timer expiry and extension message, called by the hosts
in `oasis-app`. It never runs on its own and keeps no global state (two process-wide caches exist: the sound WAV cache in `actions/sounds.rs` and the spoken-punctuation table in `speech/punctuation.rs`).

**How it works:** *reducers*. Each one is a function `(state, event, config) → (next state,
effects)`. Effects are requests ("set timer `hold-2x` for 250 ms", "dispatch BTN 2 as a bound
tap", "start the mic"); the host carries them out and feeds results back as new events.

## Subdirectories & System Boundaries
- `src/`: [src/README.md](./src/README.md) - crate source: stand-alone rule modules plus the areas below
- `src/actions/`: [README](./src/actions/README.md) - what a button does, URL policy, app resolving
- `src/bridge/`: [README](./src/bridge/README.md) - browser-extension trust and YouTube phase
- `src/input/`: [README](./src/input/README.md) - gesture arbitration; `src/input/reducer/`: [README](./src/input/reducer/README.md)
- `src/matching/`: [README](./src/matching/README.md) - browsers, hosts, fuzzy names; `src/matching/panel/`: [README](./src/matching/panel/README.md)
- `src/speech/`: [README](./src/speech/README.md) - dictation cleanup
- `src/voice/`: [README](./src/voice/README.md) - push-to-talk, models; `src/voice/command/`: [README](./src/voice/command/README.md)
- `examples/`: [README](./examples/README.md) - cleanup benchmark tool
- `tests/`: [README](./tests/README.md) - `pad_reducer.rs` (41 gesture tests, ported from `PadReducerTests.cs`), `voice_reducer.rs` (23 voice tests, from `VoiceReducerTests.cs`) and `quality.rs` (dictation quality pin, fixtures in `tests/fixtures/quality/`)

C# origins: `Primitives.cs`, `Capabilities/Capability.cs`, `Input/*`, `Voice/*`, `Speech/*`, `Matching/*`, `Bridge/*`, and the rules of `Services/ActionExecutor.cs`, `UrlLaunchPolicy.cs`, `ShortcutChord.cs`, `BleManager.cs`.

Not in this crate (see `../../docs/PORTING.md`): the `profiles.json` schema and migrations (they live in `oasis-settings`), `Diagnostics/`, and `Elevation/` beyond `startup.rs`. The C# `Intent/`, `QuickLaunch/` and `SystemControl/` rules now sit in `voice/command/` and `quick_launch.rs`. This crate also holds code with no C# original: `onboarding.rs`, `meetings.rs`, `placement.rs`, `voice/placement.rs`, `voice/self_check.rs`, `voice/command/{decide,edit,intent}.rs`, `speech/{fragments,terms,style,fixes}.rs`.

## Global Invariants
- No I/O and no dependencies (`Cargo.toml`: empty `[dependencies]`); effects are returned, never performed. The only shared state is the two caches named above and the `SystemClock` origin.
- The same rules run identically on every OS.

## Global Side Effects
- None. Hosts in `oasis-app` interpret effects. Exception: `examples/eval_cleanup.rs` reads and writes files, and it is not part of the library.

## System Verification
- Run: `cargo test -p oasis-core`
