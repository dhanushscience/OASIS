# oasis-desktop/web/src/components

## Overview
The Svelte 5 components of the OASIS window. `../App.svelte` composes them into the shell: side menu, top bar, banners, one screen at a time, and pop-ups.

## Interface
- Getting Started: `Welcome` (the first-run window's five screens), `Guide` (Mr. Science's card in the main window, the locked voice row, the leftover-data pop-up), `MrScience`.
- Shell: `SideMenu`, `TopBar`, `Banners`, `PadCard`, `UpdateCard`, `ModelsStrip`, `ModelsCard`, `SettingsDialog`, `ConfirmDialog`, `DuplicateDialog`, `UpdatePrompt` (the new-version pop-up; when it shows is `../updatePrompt.js`).
- Practice: `TryCommands` (a practice box and checklist; which step an event completes is `../tryCommands.js`). Shown in Settings › Voice ("Try voice commands", under Voice commands) and, with `compact`, in `Welcome`'s `try_commands` step after the voice part of the guide. Escape and Ctrl+F typed in its box stop there, so they do not close the window or jump to its search.
- Screens: `ProfilesScreen` (with `ProfileStrip`, `ProfileHeader`, `PadView`, `KeyboardView`, `ConfigurePanel`, `ActionField`, `AppInput`, `FlowBuilder`, `ScriptEditor`, `KeyBox`, `KeyMapDialog`), `QuickLaunchScreen`, `VoiceScreen`, `InsightsScreen`, `InputScreen`, `OverlayScreen`, `AppearanceCard`.
- Widgets: `Select`, `Switch`, `Popover`, `OsLogo`.
- Components that call Tauri commands directly (`invoke`): `ConfigurePanel`, `FlowBuilder`, `InsightsScreen`, `OverlayScreen`, `ProfileHeader`, `QuickLaunchScreen`, `UpdateCard`, `UpdatePrompt`, `VoiceScreen`; the rest use the state modules in `..` (`../profiles.svelte.js`, `../status.svelte.js`, ...).

## Invariants
- Deletes go through `../confirm.svelte.js` (`sure`), so each asks first.
- A screen with unsaved changes reports it to `../busy.svelte.js` so a restart waits.
- The engine is the judge of every value on save; components only draw and send.

## Side Effects
- Tauri command calls save profiles, settings and Quick Launch setups, read history audio, and check or install updates (see `../../../src/main.rs`).

## Verification
- Run: `npm run check` in `apps/oasis-desktop/web`.
