# oasis-desktop/src

## Overview
Rust side of the desktop app (binary `oasis-desktop`). `main.rs` starts `oasis-engine`, forwards its events to the page, owns the tray, windows and most Tauri commands; the other modules are single-purpose.

## Interface
- `main.rs`: `main` (panic hook to the crash log, `system::begin`, updater plugin, `generate_handler!`, tray menu Show OASIS / Move overlay / Quit, hide-on-close). Commands: `status`, `settings_view`, `save_button`, `save_profile`, `save_profile_look`, `save_input`, `add_profile`, `delete_profile`, `move_profile`, `set_multi_config`, `add_config`, `update_config`, `delete_config`, `try_button`, `set_typing`, `press_button`, `restore_caps_lock`, `sound_list`, `play_sound`, `overlay_look`, `overlay_relayout`, `move_overlay`, `learn_answer`, `history_list`, `history_audio`, `open_history_folder`, `open_extension_folder`, `settings_fields`, `set_setting`, `reset_overlay_look_setting`, `reload_settings`, `groq_key_status`, `set_groq_key`, `test_groq_key`, `spotify_status` / `spotify_connect` / `spotify_disconnect`, `meetings_status` / `meetings_connect` / `meetings_disconnect`, `meeting_reminder_action`, `installed_apps`, `running_apps`, `capture_focus`, `app_icon`, `site_icon`, `set_profile_icon`, `profile_icons`, `profile_use_counts`, `quick_setups`, `save_quick_setup`, `delete_quick_setup`, `quick_screens`, `capture_windows`, `run_quick_setup`. Events emitted: `engine-event`, `models-status`, `restart-needed`, `update-status`.
- `onboarding.rs`: the first-run guide (Getting Started). The one copy of its progress, saved as `onboarding.json` beside `profiles.json`; every event goes through one worker thread (`send`); `begin`, `apply`, the demo button 2 (`write_demo` / `clear_demo`), the windows `welcome` and `guide` (`show_welcome`, `create_panel`). Commands `onboarding_state`, `onboarding_event`, `onboarding_prepare_press`, `onboarding_try_press`, `onboarding_open_groq_keys`; events `onboarding-changed`, `guide-overlay`. The steps themselves are in `oasis_core::onboarding`. Test switches: `OASIS_GUIDE_TEST` (plays events), `OASIS_TEST_BELOW_FLOOR` (in `models.rs`).
- `hud.rs`: the HUD window (`LABEL = "hud"`, page `ui/hud.html`): the listening pill at the bottom centre of the screen under the mouse (with tick and cross while recording is locked), and the "close this app?" question in the same place. Solid white, or near-black in the dark theme (follows `oasis-theme` live); the bars follow the `voice-level` event (the engine's `voice_level`, sent under its own name so the other windows are not woken by it). Hidden when nothing is up; `create`, `on_engine_event`.
- `overlay.rs`: overlay window (`LABEL = "overlay"`): `create`, `place`, `follow`, `toggle_moving`, `choose_position`, `size_for`, `size_factor`, `MoveMode`.
- `settings_view.rs`: `view(&Settings) -> SettingsView` (`ProfileView`, `ConfigView`, `ButtonView`, `StepView`, `InputView`): the only shape the page sees.
- `system.rs`: `begin() -> Begin`, `watch_show_requests`; commands `system_status`, `set_start_with_windows`, `set_run_as_administrator`, `restart_oasis`.
- `update.rs`: `start`; commands `update_status`, `check_for_update`, `install_update`.
- `models.rs`: `start`; commands `models_status`, `models_choose`, `models_pause`, `models_resume`, `models_download_anyway`, `models_check_again`, `models_remove`.
- `restart.rs`: `request`; commands `restart_pending`, `engine_busy`.
- `fixes.rs` / `fixes_core.rs`: `fixes::watch(app, dictated)` and the window-free loop `fixes_core::run` with `Timing` / `REAL` (60 s watch, 2 s settle, 700 ms tick).
- `site_icon.ps1`: PowerShell that finds a website's logo, run by the `site_icon` command.

## Invariants
- Starts in preview: presses run nothing until "Run actions" is on; a failed signature check keeps it off.
- Closing the main window hides it (`CloseRequested` is prevented); only tray Quit stops the engine.
- Single-copy lock `Local\ProjectOASIS.SingleInstance`; rehearsals (`OASIS_DATA_DIR` set) use a private name.
- Every profile or setting edit goes through the engine's checked, signed, safe save.
- `fixes.rs:note` log lines never hold what was said.
- Updates install only when the person presses Install; checks run at start and every 12 h (`update.rs:CHECK_EVERY`).

## Side Effects
- Starts engine threads, a tray icon and two windows; writes the crash log, `fixes.log` and the profiles file (with signature).
- Spawns PowerShell for icons and running programs; `oasis_win` keyboard hook while keyboard input is on.
- Network: GitHub Releases updater, voice model downloads, Groq key test, Spotify and meeting connections.
- Windows login entry, scheduled task and process restart via `system.rs`.

## Verification
- Run: `cargo test -p oasis-desktop`
