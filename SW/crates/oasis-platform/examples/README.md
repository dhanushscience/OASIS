# oasis-platform examples

## Overview
Manual probes for the Windows backends. Each has a `#[cfg(windows)]` main.

## Interface
- `panel_probe.rs`: `cargo run -p oasis-platform --example panel_probe -- [seconds]` prints the focused program, panel and confidence once a second; never prints window text.
- `resolve_app.rs`: `cargo run -p oasis-platform --example resolve_app -- spotify "vs code"` shows what app names resolve to, without starting anything.

## Invariants
- Neither example starts programs or writes files.

## Side Effects
- Read the foreground window and the installed-app list.

## Verification
- Run: `cargo build -p oasis-platform --examples`
