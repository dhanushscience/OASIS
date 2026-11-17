# oasis-desktop/capabilities

## Overview
Tauri 2 capability files: what each web window is allowed to do. Contains JSON only (no source code).

## Interface
- `default.json` (identifier `default`, window `main`): `core:default`, `core:window:allow-start-dragging`, `allow-minimize`, `allow-toggle-maximize`, `allow-close`, for the window's own title bar (the window has `decorations: false` in `tauri.conf.json`).
- `overlay.json` (identifier `overlay`, window `overlay`): `core:default`, `core:window:allow-start-dragging` only (listen to events, read its look, be dragged in move mode).

- `welcome.json` (identifier `welcome`, window `welcome`): `core:default`, `allow-start-dragging`, `allow-close`; the first-run window, with no minimise or maximise.
- `guide.json` (identifier `guide`, window `guide`): `core:default` only; the caption panel over other programs only listens.
- `hud.json` (identifier `hud`, window `hud`): `core:default` only; the listening pill and close question only listen.

## Invariants
- Window labels must match `main` (tauri.conf.json), `overlay` (`overlay::LABEL`), `welcome` and `guide` (`onboarding::WELCOME`, `onboarding::PANEL`), which are created in code.
- The overlay gets no window controls beyond dragging.

## Side Effects
- None at runtime beyond restricting the web windows' API access; read at build time by `tauri-build`.

## Verification
- Run: `cargo build -p oasis-desktop` (tauri-build validates the capabilities).
