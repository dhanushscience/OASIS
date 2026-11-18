# `oasis-core/src/actions/` — what a pad button does

## Overview
**What:** the rules of `Services/ActionExecutor.cs` and `Services/UrlLaunchPolicy.cs`: the action
types, what is allowed, how a multi-step automation runs, and how a failure is worded.

**Why:** the C# executor mixed these rules with Win32 calls, so none of it could be tested
without a desktop. Here every side effect goes through the `ActionBackend` trait; the rules run
against a recording fake in tests, and each OS supplies the real backend.

**When:** a pad gesture resolves to a `ButtonAction` (via the active profile) and the host calls
`run_action`.

**How:**
- `run_action` → `Ignored` for a blank button, `Single(bool)`, or `Automation(result)`. `JoinMeeting` and `Automation` need no value.
- `run_single` reads the kind with `ActionKind::of` (exact text; `Launch App` and `Wait for Release` are the only old spellings). It runs `Shortcut`, `OpenUrl`, `LaunchApp`, `CloseApp`, `WindowCommand` / `WindowSplit` / `WindowLayout`, `RunScript`, `AutoHotkey`, `MouseClick`, `PasteText`, `OpenSettings`, `JoinMeeting`, `QuickLaunch`, `LockPc`, `Chime`, `PlaySound`, `Radio`, `Brightness`, `Theme`. Any other kind returns `false`.
- `run_steps` follows each step's `on_ok` / `on_fail` link (`Next::Auto` = the step below on success, stop and report on failure). A failure that has its own path is not a failure of the automation. It gives up after 100 runs, so a retry loop cannot go on forever.
- After a step that opens something (`LaunchApp`, `OpenUrl`, `QuickLaunch`) and has a step to go to, the runner asks the backend to wait until it is ready, at most `READY_LIMIT` (15 s).
- A failure reads `Step 2 (OpenUrl) failed: Could not open URL 'x'.` The text is the backend's `take_problem()` if it has one, otherwise a fixed sentence per kind.
- `OpenUrl` and the URL in a `LaunchApp` `browser|url` value pass `url_policy::try_allow` (https, http, spotify); anything else is refused and written to the security log.
- `RunScript` and `AutoHotkey` run only if the user switched scripts on; otherwise they are refused and logged.
- `Delay` needs a whole number of milliseconds, 0 or more. A removed `WaitRelease` step is skipped, not failed.

## Interface
Re-exports in `mod.rs`: `ActionKind`, `run_action`, `run_single`, `run_steps`, `ActionOutcome`, `AutomationResult`, `StepOutcome`, `READY_LIMIT`, `try_allow`, `UrlScope`.

| File | Contents |
|---|---|
| `mod.rs` | `ButtonAction`, `ActionStep` (each has `kind()`), `Next` (`code` / `from_code`), `MouseButton`, `MouseClick::parse`, the `ActionBackend` trait, `is_focus_free` |
| `kind.rs` | `ActionKind`: every kind as one enum, declared once by a macro. `of` (exact, plus the two old spellings), `of_ignore_case` (no aliases), `as_str` (`const`; `Unknown` is `""`), `is_spotify_text` |
| `runner.rs` | `run_action`, `run_steps`, `run_single`, `READY_LIMIT`, `StepOutcome`, `AutomationResult`, `ActionOutcome`, failure messages |
| `url_policy.rs` | `try_allow` and `UrlScope` (`Automation`, `MeetingJoin`, `Catalog`) |
| `app_resolver.rs` | `Resolved`, `AppSources` trait, `normalize_token`, `squash`, `name_matches`, `score_name`, `fuzzy_match_app_name`, `resolve`: friendly app name to something startable (file, Start Menu, Store app, install folders, App Paths, fuzzy, plain `name.exe`) |
| `settings_pages.rs` | `resolve`, `is_known`: fixed list of openable Windows Settings pages |
| `sounds.rs` | `Sound`, `SOUNDS` (5 success, 5 trouble tunes), `find`, `wav`, `wav_cached` for `PlaySound` steps |

`is_focus_free(&ButtonAction)` is true when the action does not depend on the front window: any `Spotify*` kind, `Reminder`, `CancelReminders`, `Chime`, `PlaySound`, `Radio`, `Brightness`, and a `Shortcut` made only of volume or media keys. The host may run these beside a slow "open app" instead of queuing behind it. Unknown kinds and every `Automation` stay in the ordered lane.

`ActionBackend` has required methods (`send_shortcut`, `open_url`, `launch_browser_url`, `launch_app`, `run_script`, `mouse_click`, `paste_text`, `join_meeting`, `system`, `pause`, `run_script_allowed`, `security_log`) and defaulted ones (`take_problem`, `run_autohotkey`, `close_app`, `window_action`, `quick_launch`, `front_window`, `wait_until_ready`). `system` receives only fixed commands: `lock`, `theme-dark`, `theme-light`, `chime:N`, `sound:ID`, `radio:KIND:on|off`, `brightness:N`.

Kinds named in `ActionKind` but not run by `run_single`: `CloseWindow`, `SpotifyPlay`, `SpotifyControl`, `Reminder`, `CancelReminders`, `WaitRelease`, `DeleteFile`, `MoveFile`. The host handles the Spotify and reminder kinds; `DeleteFile` and `MoveFile` exist only so the safety check can refuse them. Not built: the AI-only window steps (`FocusWindow`, `UiaClick`, ...).

## Invariants
- `run_steps` stops at the first failed step that has no `on_fail` link, and after 100 runs (`runner.rs`).
- `OpenUrl` passes `url_policy::try_allow`; a bare address with no scheme becomes https; control characters, `javascript:`, `data:`, `file:` and similar are refused.
- `RunScript` and `AutoHotkey` run only when the user enabled scripts (`runner.rs`); voice can never request them (see `../voice/README.md`).
- `OpenSettings` opens only an address from the fixed list in `settings_pages.rs`, so speech never becomes an arbitrary `ms-settings:` or command line.
- `ActionBackend::system` gets only the fixed commands listed above; `Chime`, `Radio`, `Brightness` and `Theme` values are checked first.
- The app-resolver order is fixed (`app_resolver.rs` module doc).
- `is_focus_free` is narrow on purpose: anything unknown is not free.

## Side Effects
- None directly: every effect goes through `ActionBackend`. `sounds::wav_cached` keeps a process-wide cache (`OnceLock<Mutex<HashMap>>`) and leaks each built WAV on purpose (at most 10), so the bytes stay valid for background playback.

## Verification
- Run: `cargo test -p oasis-core`
