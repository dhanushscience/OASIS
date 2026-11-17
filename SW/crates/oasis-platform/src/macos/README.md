# `oasis-platform/src/macos/` — macOS backends

## Overview

**What:** macOS implementations of the traits in `../ports.rs`. New: the C# app never ran on
macOS.

**Why:** cross-platform support was a goal of the rewrite.

**When:** compiled only for `target_os = "macos"`.

**How:** objc2 bindings to AppKit / CoreGraphics / Accessibility, btleplug (CoreBluetooth),
cpal (CoreAudio), keyring (Keychain). macOS gates most of this behind permissions the user
grants once; until then each trait returns `PlatformError::PermissionNeeded` with the exact
System Settings pane to open.

## Interface

| Trait | Backend | Permission |
|---|---|---|
| `PadTransport` | CoreBluetooth | Bluetooth |
| `ForegroundApp` | `NSWorkspace.frontmostApplication`; URL via Accessibility / AppleScript | Accessibility, Automation |
| `InputInjector` | `CGEventPost` | Accessibility |
| `SecretStore` | Keychain | — |
| `MediaSessions` | MediaRemote "Now Playing" (private; falls back to `Unknown`) | — |
| `Microphone` | CoreAudio | Microphone |
| `Autostart` | `~/Library/LaunchAgents/com.projectoasis.oasis.plist` | — |

Profiles written on Windows use Ctrl/Win chords; macOS needs a translation table
(Ctrl→⌘ for common shortcuts). That table belongs in `oasis-core` (it is a rule), tested.

| File | Contents |
|---|---|
| `mod.rs` | Backend plan and `backend_status()` |

## Invariants

- Compiled only for `target_os = "macos"`.
- No backend is implemented yet: `backend_status()` returns `ports::not_built_yet(Os::MacOs)`, so every capability reads as missing, never a crash.

## Side Effects

- None today (plan only).

## Verification

- Run: `cargo test -p oasis-platform` (on macOS)
