# `oasis-core/src/` — crate source

## Overview
The pure heart of OASIS, ported from `SW/Oasis.Core` (C#). Reducers take state and an event and return
the next state plus *effects*; a host in `oasis-app` carries them out. Directly in this folder are the
small stand-alone rule modules; larger areas are subfolders.

## Interface
`lib.rs` declares every module and re-exports `ButtonId`, `ButtonMask`, `Clock`, `CommandId`, `Instant`, `SystemClock`.

Subfolders:
- [`actions/`](./actions/README.md) — what a button does, URL policy, app resolving
- [`bridge/`](./bridge/README.md) — browser-extension admission and YouTube phase
- [`input/`](./input/README.md) — gesture arbitration (with [`input/reducer/`](./input/reducer/README.md))
- [`matching/`](./matching/README.md) — browser/YouTube/fuzzy-name helpers (with [`matching/panel/`](./matching/panel/README.md))
- [`speech/`](./speech/README.md) — dictation cleanup
- [`voice/`](./voice/README.md) — voice session, models, placement (with [`voice/command/`](./voice/command/README.md))

Files:

| File | Contents |
|---|---|
| `primitives.rs` | `ButtonId`, `ButtonMask`, `CommandId`, `Instant`, `Clock`, `SystemClock` |
| `capability.rs` | Optional features and `CapabilityState` (`Missing` carries a reason) |
| `shortcut.rs` | `Chord::parse`, `ChordKey`, `plan` — shortcut text to OS-neutral keys and press/release timing |
| `display_label.rs` | Button label fitted to the 14-character overlay pill, in capitals |
| `battery.rs` | When to warn that the pad battery is low |
| `pad_connect.rs` | When to connect, back-off after failure, which pad to trust, latency probe rate |
| `meetings.rs` | Which meeting is joinable, when to remind, how to say it |
| `placement.rs` | Window-placement arithmetic: halves, screens, which window belongs to which launched item |
| `quick_launch.rs` | Matching a spoken Quick Launch setup name |
| `startup.rs` | Launch arguments, when to hand over to the administrator copy, the scheduled-task definition |

## Invariants
- Nothing in the crate touches a file, opens a socket or calls an OS API (`lib.rs` crate doc); time arrives as a parameter. The only clock read is `primitives.rs:SystemClock`, which the host chooses to use.
- The crate has no dependencies (`Cargo.toml`).

## Side Effects
- None, apart from `SystemClock` reading the monotonic clock and the process-wide sound cache in `actions/sounds.rs`.

## Verification
- Run: `cargo test -p oasis-core`
