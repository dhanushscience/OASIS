# `oasis-platform` — everything that touches the OS

## Overview

**What:** the only crate allowed to call operating-system APIs or touch files. It provides
file locations, crash-safe storage and logging (working on all OSes today), and one trait per
OS feature that each of Windows, macOS and Linux implements.

**Why:** so the rest of OASIS never asks "which OS am I on?". Product rules stay in
`oasis-core`; this crate only answers "how do I do X on this machine?". A feature an OS cannot
offer becomes a clear `PlatformError` / `CapabilityState::Missing` with a sentence the UI shows
as-is, instead of a crash.

**When:** at startup (paths, crash log, capability probe) and whenever an effect needs the OS
(send keys, pause media, read the foreground window, talk to the pad).

**How:**

## Interface

| Path | Status | Contents |
|---|---|---|
| `src/lib.rs` | ✅ | `Os::current()`, `Os::name()`, `backend_status()` |
| `src/browser_bridge.rs` | ✅ | `BridgeServer` (`start`, `port`, `clients`, `send_command`, `push_settings`, `stop`), `BridgeEvent`, `DEFAULT_PORT`, `write_token_files` |
| `src/extension_install.rs` | ✅ | `ensure_published`, `published_folder`, `bundled_source_candidates`, `EXTENSION_FOLDER_NAME` |
| `src/foreground.rs` | ✅ | `ForegroundTracker`, `WindowSnapshot`, `WindowSource` |
| `src/integrity.rs` | ✅ | `HmacSigner`, `KeyProvider` (HMAC-SHA256 over `profiles.json`) |
| `src/pad_connector.rs` | ✅ | `PadConnector`, `PadStatus`, `ConnectorEvent` |
| `src/profile_icons.rs` | ✅ | `is_managed`, `resolve_managed`, `delete_managed` |
| `src/profile_store.rs` | ✅ | `ProfileStore`, `LoadOutcome`, `SaveError`, `SettingsIntegrity` |
| `src/secrets.rs` | ✅ | Secret-name constants (`GEMINI_API_KEY`, ...), `get_or_create_bridge_token` |
| `src/security_log.rs` | ✅ | `SecurityLog` |
| `src/trusted_device.rs` | ✅ | `TrustedDevice` (`load`, `id`, `is_trusted`, `trust`) |
| `src/paths.rs` | ✅ all OSes | Data/model/log locations. Windows paths match the C# app exactly |
| `src/atomic_file.rs` | ✅ all OSes | Temp-file → fsync → rename writes (`profiles.json` cannot be half-written) |
| `src/crash_log.rs` | ✅ all OSes | Rotating crash log that never panics; panic hook |
| `src/ports.rs` | ✅ traits | `PadTransport`, `ForegroundApp`, `InputInjector`, `SecretStore`, `MediaSessions`, `Microphone`, `Autostart`, `Speaker`, `ButtonSource`, `PlatformError`, `not_built_yet` |
| `src/windows/` | 🟡 partly built | Plan + C# sources to port from → [README](src/windows/README.md) |
| `src/macos/` | 🔲 backends | Plan + permissions needed → [README](src/macos/README.md) |
| `src/linux/` | 🔲 backends | Plan + X11/Wayland caveats → [README](src/linux/README.md) |

Only the module for the OS being compiled is built (`#[cfg(target_os = …)]`). The shared
status table lives in `ports.rs` so all three report the same way.

Backends need third-party crates (btleplug, cpal, windows, objc2, zbus, keyring). They are
listed in `Cargo.toml` and get added one backend at a time.

Subdirectories: [`src/windows/`](src/windows/README.md), [`src/macos/`](src/macos/README.md),
[`src/linux/`](src/linux/README.md), [`examples/`](examples/README.md).

## Invariants

- This crate is the place for OS and file access (with `oasis-win`, `oasis-dpapi`, `oasis-ble`, `oasis-voice`); `unsafe_code` is forbidden workspace-wide except in `oasis-win` and `oasis-dpapi`.
- Writes of `profiles.json` go through `atomic_file` (temp file, fsync, rename), never directly.
- `backend_status()` and `ports::not_built_yet` report a missing backend as `CapabilityState::Missing` with a reason; they never panic.
- `crash_log` and `security_log` writers never panic.
- Only the module for the OS being compiled exists (`#[cfg(target_os = ...)]`).

## Side Effects

- Disk: profiles, signature, trusted-device file, crash/security/voice/dictation logs, models folders, profile icons, published browser extension (all located by `OasisPaths`).
- Network: `BridgeServer` listens for the browser extension on `DEFAULT_PORT`.
- Process: `CrashLog::install_panic_hook` replaces the global panic hook.

## Verification

- Run: `cargo test -p oasis-platform`
