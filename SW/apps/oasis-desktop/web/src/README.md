# oasis-desktop/web/src

## Overview
Svelte source of the OASIS window. `main.js` mounts `App.svelte` into `app.html`, loading `tokens.css`, `legacy/legacy.css` and `keybuttons.js`. State lives in `*.svelte.js` modules (Svelte 5 runes) that wrap the Tauri commands in `../../src/main.rs`.

## Interface
- [components/](./components/README.md): all Svelte components.
- [legacy/](./legacy/README.md): the old screens' script and styles, until rebuilt.
- `welcome.js`: mounts `components/Welcome.svelte` into `welcome.html` (the first-run window).
- [guide/](./guide/): Getting Started. `mrscience.js` (the sprite as a text grid, no imports), `lines.js` (everything he says), `onboarding.svelte.js` (`guide`, `startGuide`, `send`, `at`, the mirror of the app's progress), with tests.
- `App.svelte`: the shell; sections Profiles, Quick Launch, Voice, Insights, then Settings (a pop-up).
- State modules: `profiles.svelte.js` (`data`, `load`, `select`, `saveButton`, `saveProfile`, `addProfile`, `deleteProfile`, `moveProfile`, `setMulti`, `saveInput`, `appIcon`), `status.svelte.js` (`status`, `startStatus`, `refreshStatus`), `pad.svelte.js` (`pad`, `view`, `startPad`, `onWindowPad`), `models.svelte.js` (`models`, `startModels`, `modelsCall`), `restart.svelte.js` (`restart`, `startRestart`, `quietNow`, `restartNow`), `settings.svelte.js` (`openSettings`, `closeSettings`), `confirm.svelte.js` (`sure`, `answer`), `busy.svelte.js` (`reportUnsaved`, `trackUnsaved`, `hasUnsaved`), `apps.svelte.js` (`loadApps`, `loadRunning`), `quicksetups.svelte.js` (`loadQuick`).
- Helpers: `actionKinds.js` (`detect`, `suggestions`, `parseChord`, `chordFromKeyEvent`, `MEDIA`, `QUICK`), `matching.js` (`tokens`, `overlaps`), `hues.js` (`HUES`, `paint`), `keybuttons.js` (window-local key to button mapping), `theme.js` (`currentTheme`, `themeChoice`, `chooseTheme`), `globals.d.ts`.
- Styles: `tokens.css`, `settings.css`.
- Tests: `actionKinds.test.js`, `matching.test.js`.

## Invariants
- `actionKinds.js` key names mirror `crates/oasis-core/src/shortcut.rs` (`parse_key`); the engine decides on save.
- Theme is `system` (default), `light` or `dark`, saved in `localStorage` key `oasis-theme` only when explicit; `app.html` sets it before first paint.
- Every profile change reloads the view, so the screen shows what is saved.
- Restart waits until no unsaved change, no open dialog, 60 s idle and the engine not busy (`restart.svelte.js`).

## Side Effects
- Tauri `invoke` calls and event listeners (`engine-event`, `models-status`, `restart-needed`, `update-status`); `status` is polled once a second.
- `localStorage` for theme and layout; `keybuttons.js` intercepts keys inside the window and reports them with `press_button` / `set_typing`.

## Verification
- Run: `npm test` (node --test on `src/**/*.test.js`) and `npm run check` in `apps/oasis-desktop/web`.
