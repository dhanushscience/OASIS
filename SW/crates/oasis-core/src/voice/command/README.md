# `oasis-core/src/voice/command/` — spoken commands

## Overview
Spoken commands without the assistant (the C# "Tier A"): a transcript becomes one button action by a
deterministic router against a closed vocabulary, and a safety policy decides whether it may run now,
must wait for a pad press, or is refused. Whether a phrase is typed or run is decided by `gate`
(`docs/VOICE_DECISIONS.md` V3, V4). The router never invents an action: a match is a button the user
made, a program name, or one of a few fixed phrases ("type …", "open …", "press …", "switch to …").

## Interface
Re-exports in `mod.rs`: `build_lexicon`, `LabeledAction`, `LexiconEntry`, `LexiconInputs`, `LexiconKind`,
`LexiconProfile`, `RunningApp`, `route`, `RouteResult`, `RouterSettings`, `fast_path`, `ApprovalMode`, `FastPath`.

| File | Contents |
|---|---|
| `text.rs` | `normalize`, `is_usable_command`, `repair_command_homophones` |
| `apps.rs` | `canonical_app`, `launch_name`, `find_app_in`, `guess_app_from_text`, `resolve_window_alias`, `parse_browser_open` |
| `lexicon.rs` | `build_lexicon` and its input types |
| `router.rs` | `route`, `RouterSettings`, `RouteResult`, `find_running` |
| `classifier.rs` | `is_addressed`, `without_address`, `looks_like_command` ("Oasis, …" address) |
| `gate.rs` | `Phrase`, `prepare`, `may_run_in_text_field`, `is_prose_about_writing`, `reads_as_command`, `MAX_WORDS_IN_TEXT_FIELD` (6) |
| `web_search.rs` | `Site`, `WebSearchRequest`, `try_parse`, `plan`, `search_endpoint`, `is_empty_search` |
| `reminders.rs` | `plan`, `LocalTime`, `describe`, `spoken`, `clock_text`, `flat` (alarm / timer / reminder / cancel) |
| `system.rs` | `plan` (lock, theme), `radio`, `brightness` |
| `volume.rs` | `plan` (volume phrases as media-key `Automation`) |
| `spotify.rs` | `Phrase`, `parse`, `QueryKind`, `classify_query`, `pick_track`, `pick_playlist`, `pick_album_or_artist`, `pick_device`, `seek_ms` |
| `window_cmd.rs` | `plan` (`WindowCommand`, `WindowSplit`, `WindowLayout`), `Arrangement`, `arrangements` |
| `safety.rs` | `SafetyTier`, `ApprovalMode`, `step_tier`, `evaluate`, `decide`, `fast_path`, `FastPath`, `target_key`, `names_a_shell` |

## Invariants
- Window commands are whole-utterance only; "close <app>" is not here but the router's `CloseApp`, which asks first (`window_cmd.rs`).
- Lock/theme phrases must be the whole utterance (`system.rs`).
- In a focused text field only a short (≤ 6 words), bare, single-sentence command may run, and only on an exact router match with an existing target (`gate.rs`).
- The fast router runs an action by itself only if the full safety check would run it without asking; anything else needs a pad press (button 1 runs, button 2 cancels) (`safety.rs`).
- A transport word ("next", "stop", "volume 30") goes to Spotify only when the phrase names Spotify or Spotify is running (`spotify.rs`).
- Pure: the caller passes profile data, running apps and `now` (`lexicon.rs`, `reminders.rs`).
- Known gaps: "tomorrow", dates and "1.5 hours" are not understood in reminders; volume levels are reached by media-key presses at 2% per step (`volume.rs`, shortcut).

## Side Effects
- None. Results are action descriptions; the engine runs them and owns reminders, app existence checks and the Spotify API.

## Verification
- Run: `cargo test -p oasis-core`
