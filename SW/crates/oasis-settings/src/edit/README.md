# oasis-settings/src/edit

## Overview
Validated edits to profiles, sets and buttons, for a settings window. Every edit is checked with the rules the engine uses to run an action, so a button cannot be saved in a form that would fail at the first press (a shortcut that does not parse, a link the allowlist refuses, a mouse position that is not `x,y`). Fields the crate does not model are kept: an edit changes only what it names. New in Rust; the C# app validated inside its WPF editors.

`src/edit.rs` (one level up) holds the shared part: `EditError`, the length limits, `check_length` and `invalid`. The three files here add methods and `use super::*` for the shared imports.

## Interface
| File | Contents |
|---|---|
| `button.rs` | `ButtonEdit`, `validate_action`, `ACTION_TYPES`, `STEP_TYPES`, `WRITABLE_STEP_TYPES`; `Settings::set_button`; `Button::blank`, `Button::to_edit` |
| `profile.rs` | `Settings::{update_profile, add_profile, delete_profile, move_profile, set_overlay_position, set_profile_look, set_profile_icon_path}` |
| `set.rs` | `SetEdit`, `PANEL_PRESETS`, `WHEN_OPTIONS`; `Settings::{set_multi_config, add_config, update_config, delete_config}` |

From `edit.rs`: `EditError` (`NoSuchProfile`, `NoSuchConfig`, `NoSuchButton`, `Invalid(text)`), `MAX_SETS_PER_PROFILE` (6), `MAX_PROFILE_NAME` (40), `MAX_SET_NAME` (24), `MAX_MATCH` (200). `edit.rs` re-exports the public items of `button.rs` and `set.rs`. `add_profile` and `add_config` return the new index; `delete_profile` and `delete_config` return the removed `Profile` or `ConfigProfile`.

## Invariants
- A refused edit returns an `EditError` whose text is a sentence for the person, and changes nothing.
- Lengths count characters, not bytes.
- Indexes are positions in the profile, set and button lists; a bad one is `NoSuchProfile`, `NoSuchConfig` or `NoSuchButton`.
- A profile has at most `MAX_SETS_PER_PROFILE` sets.

## Side Effects
- None beyond changing the `Settings` passed in. No disk, threads or async. Saving is done by the caller.

## Verification
- Run: `cargo test -p oasis-settings edit`. Each of the three files has inline tests; `edit.rs` holds the shared test helper.
