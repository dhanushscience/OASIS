# oasis-desktop/src

## Overview
Rust side of the desktop app (binary `oasis-desktop`). `main.rs` starts `oasis-engine`, forwards its events to the pages, owns the tray and windows, and registers every Tauri command; the commands live in one file per screen, and the other modules are single-purpose.

## Interface
- `main.rs`: `main` (panic hook to the crash log, `system::begin`, updater plugin, managed state, `generate_handler!`, tray menu Show OASIS / Move overlay / Quit, window events), `status` (the only command defined here), `with_engine`, `start_engine` / `stop_engine` / `quit`, `show_window` / `show_main`, `fit_zoom` (page zoom 1 to 1.5 on big windows), `ensure_profiles` (first run: one profile for everything, keyboard keys on, button 1 "Voice", button 3 `Win+D`). Every engine event goes to `onboarding::on_engine_event`, `hud::on_engine_event` and the pages; `voice_level` goes out as `voice-level`, the rest as `engine-event`; a typed `Transcript` starts `fixes::watch`; a reminder brings the window forward.
- Commands by file (all registered in `main.rs`'s `generate_handler!`):
  - `profiles.rs`: `settings_view`, `save_button`, `save_profile`, `save_profile_look`, `save_input`, `add_profile`, `delete_profile`, `move_profile`, `set_multi_config`, `add_config`, `update_config`, `delete_config`, `try_button`, `set_typing`, `press_button`, `restore_caps_lock`, `sound_list`, `play_sound`, `profile_use_counts`.
  - `icons.rs`: `set_profile_icon`, `profile_icons`, `app_icon`, `site_icon`.
  - `overlay_window.rs`: `overlay_look`, `overlay_relayout`, `move_overlay`; plain functions `toggle_overlay_move` (tray and `move_overlay`), `save_overlay_position`, `follow_the_screen_in_use` (thread, every 100 ms: keeps the overlay in front and on the screen under the mouse).
  - `voice.rs`: `learn_answer`, `history_list`, `history_audio`, `history_delete`, `history_delete_all`, `history_export_dataset`, `open_history_folder`, `groq_key_status`, `set_groq_key`, `test_groq_key`.
  - `settings_screen.rs`: `settings_fields`, `set_setting`, `reset_overlay_look_setting`, `reload_settings`, `open_extension_folder`.
  - `apps.rs`: `installed_apps`, `running_apps`, `capture_focus` (waits up to `PICK_SECONDS` = 20 s for a click on a program and shows the countdown on the overlay as `pick-prompt`).
  - `quick_launch.rs`: `quick_setups`, `save_quick_setup`, `delete_quick_setup`, `quick_screens`, `capture_windows`, `run_quick_setup`.
  - `accounts.rs`: `spotify_status`, `spotify_connect`, `spotify_disconnect`, `meetings_status`, `meetings_connect`, `meetings_disconnect`, `meeting_reminder_action`.
  - `system.rs`: `system_status`, `set_start_with_windows`, `set_run_as_administrator`, `restart_oasis`. Also `begin() -> Begin`, `watch_show_requests`.
  - `update.rs`: `update_status`, `check_for_update`, `install_update`; `start`.
  - `models.rs`: `models_status`, `models_choose`, `models_pause`, `models_resume`, `models_download_anyway`, `models_check_again`, `models_remove`; `start`, `guide_facts`.
  - `restart.rs`: `restart_pending`, `engine_busy`; `request`.
  - `onboarding.rs`: `onboarding_state`, `onboarding_event`, `onboarding_prepare_press`, `onboarding_try_press`, `onboarding_layout`, `onboarding_check_mic`, `onboarding_open_mic_settings`, `onboarding_test_groq`, `onboarding_open_groq_keys`.
- Events emitted to the pages: `engine-event`, `voice-level`, `models-status`, `restart-needed`, `update-status`, `onboarding-changed`; to one window: `pick-prompt`, `overlay-jump`, `overlay-moving` (overlay), `learn-offer`, `hud-side` (HUD), `guide-overlay` (guide panel).
- `onboarding.rs`: Getting Started as the app runs it. The one copy of its progress, saved as `onboarding.json` beside `profiles.json`; every event goes through one worker thread (`send`); `begin`, `apply`, `engine_ready`, `on_engine_event`, `on_models`, the demo button 2 (`write_demo` / `clear_demo`), the windows `welcome` (`WELCOME`, `show_welcome`, `welcome_due`) and `guide` (`PANEL`, `create_panel`, 520x136). The steps themselves are in `oasis_core::onboarding`. Test switches: `OASIS_GUIDE_TEST` (plays events), `OASIS_TEST_BELOW_FLOOR` (read in `models.rs`).
- `hud.rs`: the HUD window (`LABEL = "hud"`, page `ui/hud.html`): the listening pill at the bottom centre of the screen under the mouse, or standing up at a side when the overlay docks there (`Side`, `set_side`); a tick and cross while recording is locked; the "close this app?" question; the "save this word?" offer (`offer_learn`, `end_learn`). Solid white, or near-black in the dark theme; the bars follow `voice-level`. Hidden when nothing is up. `create`, `on_engine_event`.
- `overlay.rs`: overlay window (`LABEL = "overlay"`): `create`, `place`, `follow`, `toggle_moving`, `choose_position`, `follow_position`, `size_for` (380x260 tall, 680x190 wide), `size_factor`, `MoveMode`, `Screen`.
- `settings_view.rs`: `view(&Settings) -> SettingsView` (`ProfileView`, `ConfigView`, `ButtonView`, `StepView`, `InputView`): the only shape the page sees.
- `fixes.rs` / `fixes_core.rs`: `fixes::watch(app, dictated)` (thread `oasis-fixes`, Windows only; writes `fixes.log`) and the window-free loop `fixes_core::run` with `Timing` / `REAL` (60 s watch, 2 s settle, 700 ms tick). Offers go to the HUD.
- `site_icon.ps1`: PowerShell that finds a website's logo, run by the `site_icon` command.

## Invariants
- Every command the page calls is in `generate_handler!`; a new command must be added there (the windows' capabilities need no change; they use `core:default`).
- Closing the main window hides it (`CloseRequested` is prevented); only tray Quit stops the engine. The welcome window really closes.
- Single-copy lock `Local\ProjectOASIS.SingleInstance`; rehearsals (`OASIS_DATA_DIR` set) use a private name.
- Every profile or setting edit goes through the engine's checked, signed, safe save.
- `fixes.rs:note` log lines never hold what was said.
- Updates install only when the person presses Install; checks run at start and every 12 h (`update.rs:CHECK_EVERY`).
- The engine starts live (`defaults::windows::config(true)`); the app has no "Run actions" switch.

## Side Effects
- Starts engine threads, a tray icon and five windows (`main`, `overlay`, `hud`, `welcome`, `guide`); writes the crash log, `fixes.log`, `onboarding.json` and the profiles file (with signature).
- Spawns PowerShell for icons and running programs; `oasis_win` keyboard hook while keyboard input is on.
- Network: GitHub Releases updater, voice model downloads, Groq key test, Spotify and meeting connections.
- Windows login entry, scheduled task and process restart via `system.rs`.

## Verification
- Run: `cargo test -p oasis-desktop`
