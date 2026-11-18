# oasis-desktop/web

## Overview
Svelte 5 + Vite project that builds the OASIS window page. `npm run build` writes into `../ui` (`app.html`, `assets/`, `fonts/`); the built files are committed so `cargo build -p oasis-desktop` needs no Node. Rebuild whenever `src/` changes.

## Interface
- `vite.config.js`: Svelte plugin, `base: './'`, inputs `app.html` and `welcome.html` (the first-run window), `outDir: '../ui'` with `emptyOutDir: false` (`ui/overlay.html`, `ui/hud.html`, `ui/guide.html`, `ui/lines.js` and `ui/mrscience.js` are hand-written or copied separately and never wiped by Vite), dev server port 5173 strict.
- `package.json` scripts: `dev`, `build` (`node scripts/clean.mjs && vite build && node scripts/copy-guide.mjs`; the last copies `src/guide/mrscience.js` and `lines.js` into `../ui` for the hand-written `guide.html`), `check` (`svelte-check`), `test` (`node --test "src/**/*.test.js"`).
- [src/](./src/README.md): the page source.
- [scripts/](./scripts/README.md): build clean-up and motion probe.
- `app.html` and `welcome.html`: entry pages for the main and first-run windows; `app.html` sets the saved or system theme before first paint. `public/`: Geist fonts and the logo and pad pictures, copied into `ui/`. `jsconfig.json`: type-check settings for `svelte-check`.

## Invariants
- Output goes to `../ui`; `ui/assets` is wiped by `scripts/clean.mjs` before each build.
- Built file names carry hashes; `ui/app.html` and `ui/welcome.html` point at them, so commit the whole `ui/` change together.
- Dependencies are pinned exactly in `package.json`.

## Side Effects
- `npm run build` rewrites `../ui/app.html`, `../ui/welcome.html`, `../ui/assets/*`, and copies `../ui/lines.js`, `../ui/mrscience.js`.

## Verification
- Run: `npm test`
