# `oasis-platform/src/linux/` — Linux backends

## Overview

**What:** Linux implementations of the traits in `../ports.rs`. New: the C# app never ran on
Linux.

**Why:** cross-platform support was a goal of the rewrite.

**When:** compiled only for `target_os = "linux"`.

**How:** zbus for D-Bus services (BlueZ via btleplug, MPRIS, Secret Service), x11rb on X11,
`/dev/uinput` for key injection, cpal for audio.

The hard part is **Wayland**: it deliberately gives apps no standard way to read the focused
window. Plan: X11 works directly; on Wayland, ship a tiny GNOME Shell extension and a KWin
script that report the focused app over D-Bus. Without either, OASIS stays on the default
profile and the status panel says why.

## Interface

| Trait | Backend | Caveat |
|---|---|---|
| `PadTransport` | BlueZ | — |
| `ForegroundApp` | X11 `_NET_ACTIVE_WINDOW`; Wayland via compositor extension | see above |
| `InputInjector` | `/dev/uinput` | needs a udev rule (installer adds it) |
| `SecretStore` | Secret Service | headless sessions may have none |
| `MediaSessions` | MPRIS | browsers expose YouTube tabs through MPRIS |
| `Microphone` | PipeWire / PulseAudio / ALSA | — |
| `Autostart` | `~/.config/autostart/oasis.desktop` | — |

| File | Contents |
|---|---|
| `mod.rs` | Backend plan and `backend_status()` |

## Invariants

- Compiled only for `target_os = "linux"`.
- No backend is implemented yet: `backend_status()` returns `ports::not_built_yet(Os::Linux)`, so every capability reads as missing, never a crash.

## Side Effects

- None today (plan only).

## Verification

- Run: `cargo test -p oasis-platform` (on Linux)
