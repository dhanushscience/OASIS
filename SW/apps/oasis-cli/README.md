# `oasis` — headless harness

## Overview

**What:** the command line for driving the Rust build without a UI. Also the first place the
whole chain (pad → arbiter → profile matcher → action) runs for real.

**Why:** the desktop app (overlay, tray, settings) is not built yet. The CLI lets each layer be
checked against the real pad and the real `profiles.json` as it is ported.

**When:** during the port; `run` is the seed of what `apps/oasis-desktop` will do.

**How:** see Interface.

## Interface

| Command | Does |
|---|---|
| `oasis replay [--youtube] [--voice] [--ai] [--user-pad] [FILE]` | Replays timed pad lines through the real pipeline; prints effects |
| `oasis pc-check` | What this PC has and which voice model set it would get (Windows) |
| `oasis download-models <light\|balanced\|quality> <folder>` | Downloads a voice model set into a folder |
| `oasis paths` | Where OASIS keeps its files on this machine |
| `oasis status` | What works on this OS today |
| `oasis do <Type> [Value]` | Runs one button action, as a press would (`do Shortcut "Win+D"`) |
| `oasis address [PROCESS]` | Reads an open browser's address bar and shows which profile it picks, with and without the address |
| `oasis ble-scan [SECONDS]` | Lists pads in range; changes nothing |
| `oasis ble-listen [SECONDS]` | Connects to the pad and prints what it sends |
| `oasis run [--dry-run] [SECONDS]` | **The chain.** A press runs the action bound in the matching profile |

`run` details:
- Reads your real `profiles.json`, and checks its signature; if it does not match, presses are
  shown but no action runs (the C# app asks you to accept or restore).
- `--dry-run` shows what each press *would* run and runs nothing. Start here.
- It is a thin wrapper over `oasis-engine`, the same engine the desktop app runs. To see the raw
  messages from the pad, use `oasis ble-listen`.
- The gesture layout (which button is volume, hold-2×, show-desktop, push-to-talk, AI) comes
  from your `profiles.json`, the way the C# app builds it. Voice is treated as off because it is
  not ported, so button 1 stays an ordinary tap.
- A tap runs the bound action of the profile for the foreground app; holding button 3 for 800 ms
  shows the desktop (`Win+D`). Voice, AI, meeting reminders and the YouTube gestures are not wired
  yet and print a note.
- Run only one of the C# app and `oasis run` at a time: the pad takes one connection, and both
  would run the same press.

Source files are documented in [src/README.md](./src/README.md):

| File | Contents |
|---|---|
| `src/main.rs` | Command dispatch, `replay`, `paths`, `status`, `pc-check`, `download-models` |
| `src/act.rs` | `do` |
| `src/address.rs` | `address` |
| `src/ble.rs` | `ble-scan`, `ble-listen` |
| `src/run.rs` | `run` |

## Invariants
- `run` runs actions only when the `profiles.json` signature matches; `--dry-run` never runs any.
- Run only one of the C# app and `oasis run` at a time (one pad connection, same press run twice).
- The binary is named `oasis` (`Cargo.toml` `[[bin]]`); Windows-only commands are stubs elsewhere.

## Side Effects
- `do` and `run` (without `--dry-run`) run real actions; `ble-listen` and `run` hold the pad's BLE connection; `download-models` downloads and writes files; `replay`, `paths`, `status`, `ble-scan` change nothing.

## Verification
- Run: `cargo test -p oasis-cli`
