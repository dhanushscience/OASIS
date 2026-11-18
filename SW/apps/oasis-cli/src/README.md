# oasis-cli/src

## Overview
Source of the `oasis` binary (`[[bin]] name = "oasis"`, `src/main.rs`): a headless harness that drives the engine and platform layers without a UI. The module doc in `main.rs` lists the commands; `main` dispatches on the first argument.

## Interface
- `main.rs`: `main` dispatches `replay`, `paths`, `status`, `pc-check`, `download-models <light|balanced|quality> <folder>`, `do`, `address`, `run`, `ble-scan`, `ble-listen`; anything else prints usage and exits 2. `pub fn run_replay` is the testable replay core (`LineDecoder` -> `PadLink` -> `ArbiterHost`, lines of `<ms> <wire message>`).
- `act.rs`: `run` for `oasis do <Type> [Value]`; runs one `ButtonAction` through `WindowsActions` using the loaded profile store and the security log (non-Windows stub just returns).
- `address.rs`: `run` for `oasis address [PROCESS]`; reads a browser address bar and shows the profile `matching_profile` picks with and without it (non-Windows stub).
- `ble.rs`: `scan` (`ble-scan`, default 5 s) and `listen` (`ble-listen`, default 30 s) over `BleTransport` / `PadConnector`.
- `run.rs`: `run` for `oasis run [--dry-run] [SECONDS]`; thin wrapper over `oasis_engine::start` that prints `EngineEvent`s (non-Windows stub).

## Invariants
- `replay`, `paths`, `status`, `ble-scan` change nothing on disk (`ble.rs` scan doc comment).
- `run` is a harness: it uses the same engine as `oasis-desktop` and has no overlay or settings window; it only prints engine events (`run.rs` module doc).
- `download-models` writes only into the folder given, never the app's real Models folder (`main.rs:download_models` doc).
- Windows-only commands (`do`, `address`, `run`, `pc-check`) have `cfg(not(windows))` stubs that print a message and fail.

## Side Effects
- `do` runs a real action (keys, launching, URLs); `run` without `--dry-run` runs bound actions from the real `profiles.json`.
- `ble-listen` and `run` open the pad's single BLE connection.
- `download-models` makes network downloads and writes files.
- `act.rs` passes the security log to the action backend, which writes to it.

## Verification
- Run: `cargo test -p oasis-cli` (tests in `main.rs` cover replay gestures and telemetry/bad lines).
