# oasis-desktop/examples

## Overview
Manual probes for `oasis-desktop` pieces that need a real window.

## Interface
- `fixes_probe.rs`: `cargo run -p oasis-desktop --example fixes_probe -- "Open keycat and the board."` runs `fixes_core::run` (included with `#[path = "../src/fixes_core.rs"]`) against the real focused field via `oasis_win::uia::FieldReader`, printing `OFFER: ...` lines for hand fixes. Default sentence if no argument: "Open keycat and the board.".

## Invariants
- Uses `fixes_core::REAL` timing and a `|| true` still-current check, so it watches for a full 60 s unless interrupted.
- Reuses the app's loop file directly; it must not depend on Tauri types (`fixes_core` has none).

## Side Effects
- Reads text of whichever window is focused (UI Automation). Writes nothing.

## Verification
- Run: `cargo build -p oasis-desktop --examples` (manual run needs a Windows desktop).
