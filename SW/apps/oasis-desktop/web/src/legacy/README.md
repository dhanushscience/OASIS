# oasis-desktop/web/src/legacy

## Overview
The old window's script and styles, moved here as they were and restyled and typed screen by screen as each is rebuilt in Svelte. The Profiles editor is already gone from it.

## Interface
- `legacy.js`: untyped (`@ts-nocheck`) script loaded after the shell's markup exists. Calls the commands `status`, `system_status`, `restart_oasis`, `reload_settings`, `settings_fields`, `set_setting`, `groq_key_status`, `set_groq_key`, `test_groq_key`, `spotify_status`, `spotify_connect`, `spotify_disconnect`, `meetings_status`, `meetings_connect`, `meetings_disconnect`, `meeting_reminder_action`, `open_extension_folder`; listens to `engine-event`, `models-status`, `restart-needed`, `update-status`.
- `legacy.css`: the old look on the new tokens, scoped to `.legacy` so it cannot leak into the shell.

## Invariants
- Styles stay under `.legacy`; the shell's tokens (`../tokens.css`) are the source of colours.
- Settings are drawn from `settings_fields`; the script knows no setting by name.

## Side Effects
- Calls Tauri commands that save settings, secrets (Groq key) and connect Spotify and meetings accounts.

## Verification
- Run: `npm run check` in `apps/oasis-desktop/web`.
