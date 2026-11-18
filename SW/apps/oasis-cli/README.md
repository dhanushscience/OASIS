# `oasis` — headless harness

## Overview

**What:** the command line for driving the Rust build without a UI. It runs the same chain as
the desktop app (pad → arbiter → profile matcher → action) and prints what happens.

**Why:** each layer can be checked against the real pad and the real `profiles.json` with no
window, tray or overlay in the way.

**When:** for debugging and testing. The desktop app (`apps/oasis-desktop`) is the product; the
CLI is a harness.

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
  shown but no action runs (the desktop app asks you to accept or restore).
- `--dry-run` shows what each press *would* run and runs nothing. Start here.
- It is a thin wrapper over `oasis-engine`, the same engine the desktop app runs, and prints
  each `EngineEvent` as a line: pad status and battery, presses (`BTN n [profile / config] kind
  value`), actions that failed, voice phases, heard text and voice commands, meeting reminders,
  confirmations, browser-bridge status and YouTube state. To see the raw messages from the pad,
  use `oasis ble-listen`.
- The gesture layout (which button is volume, hold-2×, show-desktop, push-to-talk, AI) and the
  voice, AI and YouTube settings come from your `profiles.json`, through the engine.
- It has no overlay, settings window or tray. The first tap on a button only prints what the
  buttons do (`labels [...]`); tap again to run.
- `SECONDS` ends the run after that many seconds; without it, it runs until Ctrl+C.
- Run only one of the desktop app and `oasis run` at a time: the pad takes one connection, and
  both would run the same press.

Source files are documented in [src/README.md](./src/README.md):

| File | Contents |
|---|---|
| `src/main.rs` | Command dispatch, `replay`, `paths`, `status`, `pc-check`, `download-models`, and the tests |
| `src/act.rs` | `do` (types: Shortcut, OpenUrl, LaunchApp, MouseClick, PasteText, RunScript; scripts run only if allowed in settings) |
| `src/address.rs` | `address` |
| `src/ble.rs` | `ble-scan`, `ble-listen` |
| `src/run.rs` | `run` |

## Invariants
- `run` runs actions only when the `profiles.json` signature matches; `--dry-run` never runs any.
- Run only one of the desktop app and `oasis run` at a time (one pad connection, same press run twice).
- The binary is named `oasis` (`Cargo.toml` `[[bin]]`); `do`, `address`, `run` and `pc-check` are stubs that fail on other systems.

## Side Effects
- `do` and `run` (without `--dry-run`) run real actions; `ble-listen` and `run` hold the pad's BLE connection; `download-models` downloads and writes files; `replay`, `paths`, `status`, `pc-check`, `ble-scan` change nothing.

## Verification
- Run: `cargo test -p oasis-cli`
