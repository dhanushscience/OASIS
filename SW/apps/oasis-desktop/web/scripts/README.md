# oasis-desktop/web/scripts

## Overview
Node helper scripts for the `web/` build.

## Interface
- `clean.mjs`: removes `../../ui/assets` before each build (run by `npm run build`), so old hashed bundles do not pile up.
- `copy-guide.mjs`: copies `src/guide/mrscience.js` and `lines.js` into `../../ui` (run by `npm run build` after Vite), for the hand-written `ui/guide.html`.
- `motion-probe.mjs`: `node scripts/motion-probe.mjs [reduced]` loads the built page in headless Chrome (remote debugging port 9333) with a stub Tauri API and prints the real animations (property, duration, delay). Needs `npx vite preview --port 4173 --strictPort` and a headless Chrome started by hand first; `reduced` emulates `prefers-reduced-motion`.

## Invariants
- `clean.mjs` deletes only `ui/assets`; `ui/app.html`, fonts and logos stay.
- `motion-probe.mjs` must be given its own Chrome profile folder so no other Chrome is touched.

## Side Effects
- `clean.mjs` deletes files. `motion-probe.mjs` opens a debugging tab on 127.0.0.1:9333 and makes no changes to the repo.

## Verification
- Run: `npm run build` in `apps/oasis-desktop/web` (exercises `clean.mjs`).
