# oasis-desktop/web/src/guide

## Overview
Getting Started, the first-run guide, as the pages see it. The steps are decided in Rust (`oasis_core::onboarding`) and run by `../../../src/onboarding.rs`; nothing here keeps its own idea of the step.

## Interface
- `mrscience.js`: Mr. Science as a text grid (`GRID`, one letter per pixel), `COLOURS` per theme, `svg(theme, scale)`. No imports and no DOM, so a test and the hand-written `ui/guide.html` load it as it is.
- `lines.js`: everything he says, by step (`LINES`, `line`), plus `BLOCKED`, `DOWNLOAD`, `REFUSED`, `NOTICE`, the pitch. No imports. One file so the wording can change without touching a component.
- `onboarding.svelte.js`: `guide` (a reactive mirror of the app's progress), `startGuide`, `send(type, value)` (the events the pages own), `at(step)`.
- Tests: `mrscience.test.js` (grid shape, outline contrast 3 to 1 in both themes), `lines.test.js` (every step, blocked reason and download state has a line; at most 90 characters).

## Invariants
- `mrscience.js` and `lines.js` are copied to `../../../ui/` by `scripts/copy-guide.mjs` for the caption panel (`ui/guide.html`); keep them import-free.
- A line is at most 90 characters and uses no long dash.

## Verification
- Run: `npm test` in `apps/oasis-desktop/web`.
