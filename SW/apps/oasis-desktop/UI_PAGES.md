# oasis-desktop/ui

## Overview
The pages Tauri packs (`frontendDist` in `tauri.conf.json`). Two kinds: pages built from `../web/` by Vite (committed, never edited by hand) and small pages written by hand for the windows that must stay light and independent of the Svelte runtime.

## Interface
- Built by `npm run build` in `../web/` (do not edit):
  - `app.html`: the main window.
  - `welcome.html`: the first-run window (window `welcome`).
  - `assets/`: hashed bundles (`app-*`, `welcome-*`, `MrScience-*` shared chunk, `legacy-*` the old settings script), wiped and rebuilt each build.
  - `fonts/`: Geist and Geist Mono (`OFL-Geist.txt` is their licence); `oasis-logo-ink.png`, `oasis-logo-paper.png`, `oasis-pad.png`: copied from `../web/public/`.
  - `lines.js`, `mrscience.js`: copied from `../web/src/guide/` for `guide.html`.
- Written by hand (survive a rebuild):
  - `overlay.html`: the overlay window (pad bars, labels, move mode, "pick a program" countdown). Listens to `engine-event`, `pick-prompt`, `overlay-jump`, `overlay-moving`; calls `overlay_look` and `overlay_relayout`.
  - `hud.html`: the HUD window (listening pill, tick and cross, close question, "save this word?" offer). Listens to `engine-event`, `voice-level`, `hud-side`, `learn-offer`; calls `press_button` and `learn_answer`.
  - `guide.html`: the Getting Started caption panel (window `guide`). Listens to `guide-overlay` only; takes no clicks or keys.

## Invariants
- Built files are committed; change `../web/src/` and rebuild instead of editing `app.html`, `welcome.html` or `assets/`.
- The hand-written pages draw only from events and the commands listed above; they hold no state worth saving.
- `lines.js` and `mrscience.js` here are copies; edit them in `../web/src/guide/`.

## Side Effects
- None at runtime beyond what the pages call; the web pages invoke Tauri commands (see `../src/README.md`).

## Verification
- Run: `npm run build` then `cargo build -p oasis-desktop` (the build reads the capability files and packs these pages).
