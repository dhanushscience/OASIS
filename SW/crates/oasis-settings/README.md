# oasis-settings

## Overview
The typed model of `profiles.json` (schema v2), the migration from older files, JSON read/write, and the validated edits a settings window needs. Port of `Oasis.Core/Settings`, in part.

Users already have a `profiles.json` from the C# app. The Rust build must read it without losing anything, and the two builds may share the file while the port is unfinished. Used at startup (`load`) and whenever settings change (`to_json`, saved by `oasis-platform::ProfileStore`).

Not done yet: typing `Voice`, `Ai`, `PadGestures`, `QuickLaunch`, `OverlayAppearance` (`SettingsSnapshotJson.cs`, 312 lines); the profile signature (D12).

`src/fields.rs` is a table of the simple settings (switches, numbers, choices, colours): label, where `profiles.json` keeps it, its limits and default. `sections()` gives the screen its contents, `set_field()` saves one value after checking it, `warnings()` notes clashes. Limits are the C# window's dial and list ranges. `Settings::set_profile_look` sets a profile's label behaviour and emoji.

## Interface
- `src/lib.rs`: `load(text) -> Result<Loaded, SettingsError>`, `to_json`, `SettingsError`, `Loaded { settings, migrated_from, extracted_google_client_secret }`; re-exports `Settings`, `Profile`, `ConfigProfile`, `Button`, `may_persist`, `CURRENT_SCHEMA`.
- `src/model.rs`: `Settings`, `Profile`, `ConfigProfile`, `Button` (C# `SettingsSnapshot`, partly).
- `src/migrate.rs`: `apply`, `read_version`, `MigrationResult` (v0 to v1 to v2; C# `SettingsMigrationPipeline`).
- `src/schema.rs`: `CURRENT_SCHEMA`, `DEFAULT_STT_MODEL_FOLDER`, `DEFAULT_LLM_MODEL_FILE_NAME`, `is_retired_stt_model`, `may_persist` (C# `SettingsSchema`, `IntegrityLoadPolicy`).
- `src/matcher.rs`: `resolve_active_profile`, `matching_profile`, `matching_config`, `ActiveProfile`, `has_url_based_profiles`, `has_panel_profiles`, `is_youtube_foreground`, `is_clearly_non_youtube_foreground` (C# `ProfileMatcher.cs`).
- `src/arbiter_bridge.rs`: `arbiter_config(settings, voice_model_present)` (C# `ArbiterSettingsBridge.cs`).
- `src/edit.rs`: `EditError`, `ButtonEdit`, `SetEdit`, `validate_action`, `ACTION_TYPES`, `STEP_TYPES`, `WRITABLE_STEP_TYPES`, limits (`MAX_SETS_PER_PROFILE`, `MAX_PROFILE_NAME`, `MAX_SET_NAME`, `MAX_MATCH`), `PANEL_PRESETS`, `WHEN_OPTIONS`; `Settings` methods `set_button`, `update_profile`, `add_profile`, `delete_profile`, `move_profile`, `set_overlay_position`, `set_profile_look`, `set_profile_icon_path`, `set_multi_config`, `add_config`, `update_config`, `delete_config`. New (C# validated in WPF editors).
  Split by what is edited: `src/edit.rs` (`EditError`, the limits, `check_length`), `src/edit/button.rs` (`ButtonEdit`, `validate_action`, the action and step type lists, `set_button`, `Button::blank`, `Button::to_edit`), `src/edit/profile.rs` (`update_profile`, `add_profile`, `delete_profile`, `move_profile`, `set_overlay_position`, `set_profile_look`, `set_profile_icon_path`), `src/edit/set.rs` (`SetEdit`, `PANEL_PRESETS`, `WHEN_OPTIONS`, `set_multi_config`, `add_config`, `update_config`, `delete_config`).
- `src/fields.rs`: `sections`, `set_field`, `add_glossary_word`, `reset_overlay_look`, `warnings`, `FieldContext`, `FieldView`, `SectionView`, `ChoiceView`.
  `src/fields.rs` holds the types and the engine (`sections`, `set_field`, `warnings`, `reset_overlay_look`); `src/fields/table.rs` the field table (`specs`, in Settings screen order, sections, colours, talk presets); `src/fields/glossary.rs` the dictionary box and `add_glossary_word`.
- `src/overlay.rs`: `OverlayAppearance`, `overlay_appearance`, `overlay_position`, `css_color`, `DOCKS`, `IDLE_MODES`.
- `src/input.rs`: `InputSettings`, `DEFAULT_KEYS`, `check_keys`.
- `src/system.rs`: `start_with_windows`, `run_as_administrator`, `read_run_as_administrator`.
- `src/voice.rs`: `VoiceSettings`; `src/voice_commands.rs`: `CaptureMode`, `capture_mode`, `dictate_first_apps`, `is_dictate_first`, `router_settings`, `approval_mode`, `lexicon`, `trusted_targets`.
- `src/quick_launch.rs`: `Setup`, `Item`, `ItemKind`, `ItemEdit`, `SetupEdit`, `setups`, `voice_refs`, `startup_setup`, `find`, `step_of`, `without_blocked`, `to_action`.
- `src/meetings.rs`: `MeetingSettings`; `src/youtube.rs`: `DEFAULT_BRIDGE_PORT`, `bridge_enabled`, `bridge_port`, `gestures_enabled`, `extension_settings`, `gestures_active`.
- `src/profile_ext.rs`: private helpers (no public items).

## Invariants
- Fields typed today are typed; everything else is kept as raw JSON in `extra` and written back unchanged, so a section can be ported later without a data migration.
- No `SchemaVersion` means schema 0; it is migrated v0 to v1 to v2 by a port of the C# pipeline working on raw JSON. A newer schema is refused (a deliberate difference from C#).
- A Google client secret found in the file is returned in `Loaded.extracted_google_client_secret` and blanked in the settings; the caller must store it or it is lost.
- Edits are validated before they change `Settings`; a refused edit returns `EditError` and changes nothing.
- Defaults: `StartWithWindows` on, `RunAsAdministrator` off, `InputHardware` on, `InputKeyboard` off, `InputKeysExclusive` on.

## Side Effects
- Pure on in-memory `Settings`; the one disk read is `system::read_run_as_administrator` (reads a `profiles.json` path). Saving is done by `oasis-platform::ProfileStore`.
- Env var `OASIS_REAL_PROFILES=<path to profiles.json>` enables the real-file round-trip test.

## Verification
- Run: `cargo test -p oasis-settings` (`tests/settings.rs`: ports of `SettingsMigrationTests` plus extra migration cases; `tests/corpus/`: v0, v1, hand-edited, corrupt files)
