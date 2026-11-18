# `oasis-platform/src/windows/` — Windows backends

## Overview

**What:** Windows implementations of the traits in `../ports.rs`.

**Why:** Windows is where OASIS already runs (C# app), so this is the first backend to finish
and the reference for the other two. Every item has a C# file to port from.

**When:** compiled only for `target_os = "windows"`; used for the whole session.

**How:** the `windows` crate (Microsoft's official Rust bindings for Win32 and WinRT) with `oasis-win` and `oasis-dpapi` doing the unsafe calls. BLE is `oasis-ble`; audio capture is in `oasis-voice`. Data paths and the DPAPI secret file layout match the C#
app, so a user can switch to the Rust build and keep their profiles and keys.

## Interface

| Trait | Backend | Port from |
|---|---|---|
| `PadTransport` | in `oasis-ble` (`BleTransport`, btleplug over WinRT GATT), not here | `Services/BleManager.cs`, `BleDeviceTrust.cs` |
| `ForegroundApp` | ✅ `oasis-win` (`GetForegroundWindow`, `QueryFullProcessImageNameW`) behind the portable `ForegroundTracker`; address bar and panel via `oasis-win::uia` | `Services/ContextTracker.cs`, `Services/Context/*` |
| `InputInjector` | ✅ `SendInput` via `oasis-win` (`input.rs`); mouse and media keys with the executor port | `Services/ActionExecutor.cs`, `ShortcutChord.cs` |
| `SecretStore` | ✅ DPAPI via `oasis-dpapi`, same `secrets/` files (`dpapi.rs`) | `Platform/CredentialStore.cs` |
| `MediaSessions` | ✅ GSMTC through `oasis-win::media` (called by the engine; no trait impl here) | `Services/System/*` |
| `Microphone` | not here: the engine's `VoiceMic` over `oasis-voice` | `Services/Voice/MicrophoneCapture.cs` |
| `Autostart` | ✅ `HKCU\…\Run`; scheduled task when elevated (`startup.rs`, plain functions) | `AutostartService.cs`, `Services/Elevation/*` |

| File | Contents |
|---|---|
| `mod.rs` | Backend plan and `backend_status()` |
| `dpapi.rs` | `DpapiSecretStore`, `DpapiKeyFile` (the profile-signing key) |
| `foreground.rs` | `Win32Windows` window source, `foreground_app()` |
| `input.rs` | `WindowsInput`, `virtual_key` (chord key → Windows key code) |
| `actions.rs` | `WindowsActions` (the real `ActionBackend`), `browser_executable`, `installed_browsers`, `set_preferred_browser`, `BROWSERS` |
| `app_discovery.rs` | `WindowsApps`, `windows_apps`, `resolve`, `warm` (reads the app lists on a background thread), `parse_start_apps`, `merge_installed`, `expand_start_app_path`, `start_command`, `StartCommand`, `launch_target`, `is_installed` |
| `key_buttons.rs` | `KeyboardButtons` (the keyboard as a `ButtonSource`, over `oasis_win::keyboard`), `chord_key_from_vk` |
| `pc_check.rs` | `pc_facts` (this PC's facts for the voice model choice, via `oasis_win::hardware`) |
| `startup.rs` | `autostart_enabled`, `set_autostart`, `admin_status` / `AdminStatus`, `is_task_installed`, `task_command`, `task_runs`, `install_admin_task`, `remove_admin_task`, `start_elevated_instance`, `request_window_on_next_start`, `consume_window_request`, `WindowRequest`, `SINGLE_INSTANCE_MUTEX`, `SHOW_WINDOW_EVENT` |
| `window_commands.rs` | `run` (window commands) |

Plan and order: [`../../../../docs/WINDOWS_BACKEND.md`](../../../../docs/WINDOWS_BACKEND.md).

## Invariants

- Compiled only for `target_os = "windows"`.
- Secret files keep the C# `secrets/` layout and DPAPI blobs (via `oasis-dpapi`), so existing keys and profile signatures stay valid.
- `backend_status()` still returns `not_built_yet(Os::Windows)`; per-trait status is not reported from here yet.

## Side Effects

- Registry: `HKCU` Run autostart value (`set_autostart`); scheduled task install and remove for admin start.
- Processes: launches apps and elevated instances.
- Input: synthesizes keyboard events and runs window commands on other programs' windows.
- Named mutex and event for the single-instance lock and show-window signal.
- Threads: short-lived ones to bring a started program's window forward (up to 8 s), run `RunScript`/AutoHotkey, and refresh the app list.
- Disk: DPAPI-protected secret and key files.

## Verification

- Run: `cargo test -p oasis-platform` (Windows only)
