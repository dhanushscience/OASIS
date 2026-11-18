# oasis-settings

## Overview
The typed model of `profiles.json` (schema v2), the migration from older files, JSON read/write, the validated edits a settings window needs, and read-only views of the sections the app uses (voice, input, overlay, YouTube, meetings, Quick Launch). Port of `Oasis.Core/Settings`, in part.

Users already have a `profiles.json` from the C# app. The Rust build must read it without losing anything, and the two builds may share the file while the port is unfinished. Used at startup (`load`) and whenever settings change (`to_json`, saved by `oasis-platform::ProfileStore`).

How a section is held: `Settings` types only the profile list, the overlay position, `ShowOverlayLabels`, `AllowRunScriptActions` and `SchemaVersion`. Every other key (`Voice`, `Ai`, `PadGestures`, `YouTube`, `QuickLaunch`, `OverlayAppearance` and the rest) stays raw JSON in `Settings.extra`. The modules `voice`, `voice_commands`, `quick_launch`, `overlay`, `input`, `system`, `meetings` and `youtube` read (and in places write) those keys by name, with the C# default for anything missing. They are views, not stored structs. `Ai` and `PadGestures` have no view of their own: `arbiter_bridge` reads them raw, and `fields/table.rs` edits two `PadGestures` keys. Not done yet: the profile signature (D12).

Sub-folders, each with its own README:
- [`src/edit/`](src/edit/README.md): the validated edits, one file per thing edited.
- [`src/fields/`](src/fields/README.md): the table of simple settings and the dictionary box.

## Interface
- `src/lib.rs`: `load(text) -> Result<Loaded, SettingsError>`, `to_json`, `SettingsError` (`Parse`, `TooNew`), `Loaded { settings, migrated_from, extracted_google_client_secret }`; re-exports `Settings`, `Profile`, `ConfigProfile`, `Button`, `may_persist`, `CURRENT_SCHEMA`. Its module table lists only some modules.
- `src/model.rs`: `Settings`, `Profile`, `ConfigProfile`, `Button` (C# `SettingsSnapshot`, partly).
- `src/profile_ext.rs`: a private module that adds public methods to the model types: `ConfigProfile::{is_primary, normalized_when, has_media_when}`, `Profile::primary_config_index`, `Button::{to_action, automation_steps}` (C# `ProfileExtensions.cs`, read-only parts).
- `src/migrate.rs`: `apply`, `read_version`, `MigrationResult` (v0 to v1 to v2; C# `SettingsMigrationPipeline`).
- `src/schema.rs`: `CURRENT_SCHEMA`, `DEFAULT_STT_MODEL_FOLDER`, `DEFAULT_LLM_MODEL_FILE_NAME`, `is_retired_stt_model`, `may_persist` (C# `SettingsSchema`, `IntegrityLoadPolicy`).
- `src/matcher.rs`: `resolve_active_profile`, `matching_profile`, `matching_config`, `ActiveProfile`, `has_url_based_profiles`, `has_panel_profiles`, `is_youtube_foreground`, `is_clearly_non_youtube_foreground` (C# `ProfileMatcher.cs`). Results are indexes into the profile list.
- `src/arbiter_bridge.rs`: `arbiter_config(settings, voice_model_present)` (C# `ArbiterSettingsBridge.cs`). Reads `YouTube`, `Voice`, `Ai`, `PadGestures` raw. Double-click for volume on YouTube is switched off here: `volume_up` and `volume_down` are always `None`, and the stored `VolumeUpButtonIndex` / `VolumeDownButtonIndex` are kept but not used. Hold for 2x still works.
- `src/edit.rs` and `src/edit/`: see [src/edit/README.md](src/edit/README.md). Public: `EditError`, `ButtonEdit`, `SetEdit`, `validate_action`, `ACTION_TYPES`, `STEP_TYPES`, `WRITABLE_STEP_TYPES`, `PANEL_PRESETS`, `WHEN_OPTIONS`, the limits `MAX_SETS_PER_PROFILE`, `MAX_PROFILE_NAME`, `MAX_SET_NAME`, `MAX_MATCH`, and the `Settings` and `Button` edit methods. New (C# validated in WPF editors).
- `src/fields.rs` and `src/fields/`: see [src/fields/README.md](src/fields/README.md). Public: `sections`, `set_field`, `add_glossary_word`, `reset_overlay_look`, `warnings`, `FieldContext`, `FieldView`, `SectionView`, `ChoiceView`.
- `src/overlay.rs`: `OverlayAppearance`, `overlay_appearance`, `overlay_position`, `css_color`, `popup_vertical`, `POPUP` (`"VoicePopup"`), `DOCKS`, `IDLE_MODES`.
- `src/input.rs`: `InputSettings`, `DEFAULT_KEYS`, `VERTICAL_KEYS`, `preset_keys`, `check_keys`; `Settings::set_input`, `Settings::choose_layout`.
- `src/system.rs`: `start_with_windows`, `run_as_administrator`, `read_run_as_administrator`; `Settings::set_start_with_windows`, `Settings::set_run_as_administrator`.
- `src/voice.rs`: `VoiceSettings` (`VoiceSettings::from(&Settings)`); the default model folder comes from `schema.rs`.
- `src/voice_commands.rs`: `CaptureMode`, `capture_mode`, `dictate_first_apps`, `is_dictate_first`, `router_settings`, `approval_mode`, `lexicon`, `trusted_targets`; `Settings::trust_voice_targets`.
- `src/quick_launch.rs`: `Setup`, `Item`, `ItemKind`, `ItemEdit`, `SetupEdit`, `setups`, `voice_refs`, `startup_setup`, `find`, `step_of`, `without_blocked`, `to_action`; `Settings::save_setup`, `Settings::delete_setup`.
- `src/meetings.rs`: `MeetingSettings` (`MeetingSettings::from(&Settings)`).
- `src/youtube.rs`: `DEFAULT_BRIDGE_PORT`, `bridge_enabled`, `bridge_port`, `gestures_enabled`, `extension_settings`, `gestures_active`. `extension_settings` still sends the volume button indexes to the extension; the arbiter ignores them.

## Invariants
- Fields typed in `Settings` are typed; everything else is kept as raw JSON in `extra` and written back unchanged, so a section can be ported later without a data migration.
- No `SchemaVersion` means schema 0; it is migrated v0 to v1 to v2 by a port of the C# pipeline working on raw JSON. A newer schema is refused (a deliberate difference from C#).
- A Google client secret found in the file is returned in `Loaded.extracted_google_client_secret` and blanked in the settings; the caller must store it or it is lost.
- Edits are validated before they change `Settings`; a refused edit returns `EditError` and changes nothing.
- Defaults: `StartWithWindows` on, `RunAsAdministrator` off, `InputHardware` on, `InputKeyboard` off, `InputKeysExclusive` on. `set_input` refuses bad or duplicate shortcuts, and refuses turning off both the pad and the keyboard.
- Read views fall back to the C# default for a missing or wrong-typed key; none of them fails.

## Side Effects
- Pure on in-memory `Settings`; the one disk read is `system::read_run_as_administrator` (reads a `profiles.json` path). Saving is done by `oasis-platform::ProfileStore`. No threads or async.
- Env var `OASIS_REAL_PROFILES=<path to profiles.json>` enables the real-file round-trip test.

## Verification
- Run: `cargo test -p oasis-settings`.
- `tests/settings.rs`: ports of `SettingsMigrationTests` plus extra migration, load and round-trip cases.
- `tests/corpus/`: `v0.json`, `v1.json`, `hand-edited.json`, `corrupt.json`.
- Each module also has inline unit tests (`#[cfg(test)]`).
