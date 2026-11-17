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
- `run_action` → `Ignored` for a blank button, `Single(bool)`, or `Automation(result)`.
- `run_steps` stops at the first failed step: `Step 2 (OpenUrl) failed: Could not open URL 'x'.`
- `OpenUrl` and the URL in `browser|url` pass `url_policy::try_allow` (https, http, spotify);
  anything else is refused and written to the security log.
- `RunScript` runs only if the user switched it on; otherwise it is refused and logged.
- Types are compared exactly as C# did (`Shortcut`, `OpenUrl`, `LaunchApp` / `Launch App`,
  `MouseClick`, `PasteText`, `RunScript`, `JoinMeeting`, `Delay`; an old step of a removed kind is skipped).

## Interface
Re-exports in `mod.rs`: `run_action`, `run_single`, `run_steps`, `ActionOutcome`, `AutomationResult`, `StepOutcome`, `READY_LIMIT`, `try_allow`, `UrlScope`.

| File | Contents |
|---|---|
| `mod.rs` | `ButtonAction`, `ActionStep`, `Next`, `MouseButton`, `MouseClick::parse`, the `ActionBackend` trait |
| `runner.rs` | `run_action`, `run_steps`, `run_single`, failure messages |
| `url_policy.rs` | `try_allow` and `UrlScope` |
| `app_resolver.rs` | `Resolved`, `AppSources` trait, `normalize_token`, `squash`, `name_matches`, `score_name`: friendly app name to something startable (file, Start Menu, Store app, install folders, App Paths, fuzzy, plain `name.exe`) |
| `settings_pages.rs` | `resolve`, `is_known`: fixed list of openable Windows Settings pages |
| `sounds.rs` | `Sound`, `SOUNDS` (5 success, 5 trouble tunes), `find`, `wav`, `wav_cached` for `PlaySound` steps |

Not here: the AI-only window steps (`FocusWindow`, `CloseWindow`, `UiaClick`, …), which come with
the intent planner.

## Invariants
- `run_steps` stops at the first failed step (`runner.rs`).
- `OpenUrl` passes `url_policy::try_allow`; only https, http and spotify are allowed.
- `RunScript` runs only when the user enabled it (`runner.rs`); voice can never request it (see `../voice/README.md`).
- `OpenSettings` opens only an address from the fixed list in `settings_pages.rs`, so speech never becomes an arbitrary `ms-settings:` or command line.
- The app-resolver order is fixed (`app_resolver.rs` module doc).

## Side Effects
- None directly: every effect goes through `ActionBackend`. `sounds::wav_cached` keeps a process-wide cache (`OnceLock<Mutex<HashMap>>`).

## Verification
- Run: `cargo test -p oasis-core`
