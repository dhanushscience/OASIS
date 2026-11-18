# oasis-settings/src/fields

## Overview
The table of simple settings (switches, numbers, choices, colours, and the dictionary box). Each setting is described once: label, where `profiles.json` keeps it, its limits and its default. The settings screen draws itself from `sections` and saves through `set_field`, so adding a setting is one line in the table. Limits are the C# window's dial and list ranges.

`src/fields.rs` (one level up) holds the public types, the private `Kind`, `Place` and `Spec`, and the engine: `sections`, `set_field`, `warnings`, `reset_overlay_look`. The two files here use `super::*` to share them. A value lives in one of: a typed `Settings` field (`ShowOverlayLabels`, `AllowRunScriptActions`), a root key, a key inside a section (`Voice.Enabled`), the push-to-talk buttons (`Voice.PushToTalkMask` with `Voice.Mode`), or a glossary list under `Voice`.

## Interface
| File | Contents |
|---|---|
| `table.rs` | `specs` (every field, in Settings screen order), the section list (General, YouTube, Pad gestures, Overlay look, Voice, Meetings, Preferences), theme colours, talk presets (`TALK_PRESETS`, `talk_presets`, `mask_key`, `read_mask`); all `pub(super)` |
| `glossary.rs` | `add_glossary_word` (public); `glossary_text`, `glossary_rows` (`pub(super)`) |

Public API, from `fields.rs`: `sections(settings, ctx) -> Vec<SectionView>`, `set_field(settings, id, value, ctx) -> Result<(), EditError>`, `add_glossary_word`, `reset_overlay_look`, `warnings`, `FieldContext { microphones, browsers }`, `FieldView`, `SectionView`, `ChoiceView`.

## Invariants
- `set_field` checks the value against the field's limits and kind first; a refused value returns `EditError` and changes nothing.
- A stored number is shown as plain JSON (350.0 reads as 350).
- A missing or null stored value shows the field's default.
- `warnings` notes clashes without refusing them (for example, the show-desktop button is also the 2x-speed button).
- The YouTube hold-for-2x field works. Double-click for volume is switched off in `arbiter_bridge.rs`, but the hint of the YouTube gestures field in `table.rs` still mentions it.

## Side Effects
- None beyond changing the `Settings` passed in. No disk, threads or async.

## Verification
- Run: `cargo test -p oasis-settings fields`. `fields.rs` and `glossary.rs` have inline tests.
