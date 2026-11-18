# `crates/` — the libraries

**What:** eleven Rust libraries that hold all of OASIS's logic. The programs in `../apps` are thin
wrappers around them.

**Why so many:** each layer has a different rule about what it may touch, and splitting them lets
the compiler enforce it (a crate cannot call what it does not depend on). This is the same
layering as the C# solution, where `CoreIsolationTests` enforced it by test.

| Crate | May depend on | Must never | C# equivalent |
|---|---|---|---|
| `oasis-core` | std | do I/O, read the clock, call the OS | `Oasis.Core` |
| `oasis-protocol` | std | change the wire format | wire code in `BleManager.cs` |
| `oasis-app` | core, protocol | call the OS | `Oasis.Application` |
| `oasis-settings` | core | touch the OS | `Oasis.Core/Settings`, `ProfileMatcher` |
| `oasis-platform` | core, protocol, settings (and `oasis-win`, `oasis-dpapi` on Windows) | contain product rules | `Oasis.Adapters.Windows`, `Platform/` |
| `oasis-models` | core | do I/O (copying files is the platform's job) | new (swappable models) |
| `oasis-engine` | core, app, platform, settings, ble, models, voice (and `oasis-win` on Windows) | show a window | `OverlayService`, `OasisHost` (the wiring) |
| `oasis-ble` | platform, protocol, core | decide *when* to connect | the Bluetooth half of `BleManager` |
| `oasis-voice` | models (core in tests only) | decide what a transcript means | speech-to-text half of the voice service |
| `oasis-win` | Windows APIs | be called by non-Windows code | the P/Invoke code in several C# files |
| `oasis-dpapi` | Windows APIs | do anything but encrypt and decrypt | `ProtectedData` use |

Only `oasis-win`, `oasis-dpapi` and `oasis-voice` may use `unsafe` (they wrap native calls, each
block with a `SAFETY:` note; their `Cargo.toml` sets `unsafe_code = "deny"`); every other crate
forbids it (DECISIONS D13).

**When:** all of them are linked into every OASIS program and run inside its process.

**How to choose where new code goes:**
1. Is it a *decision* (what a gesture means, which profile applies, what is allowed)? →
   `oasis-core` (or `oasis-settings` if it reads the profile model), with tests.
2. Is it about the bytes on the BLE link? → `oasis-protocol`.
3. Does it *coordinate* decisions over time (queues, timers)? → `oasis-app`.
4. Does it call an OS API or touch a file? → `oasis-platform`, behind a trait; the raw Windows
   call goes in `oasis-win`.
5. Does it wire the parts into the running app? → `oasis-engine`.

## Subdirectories & Boundaries

- [oasis-core](./oasis-core/README.md) - pure rules: gestures, actions, matching, speech cleanup, voice commands.
- [oasis-protocol](./oasis-protocol/README.md) - frozen BLE wire format.
- [oasis-app](./oasis-app/README.md) - single-writer hosts, timers, capability registry.
- [oasis-settings](./oasis-settings/README.md) - typed `profiles.json` model and edits (see also [edit](./oasis-settings/src/edit/README.md) and [fields](./oasis-settings/src/fields/README.md)).
- [oasis-platform](./oasis-platform/README.md) - paths, atomic files, logs, profile store, per-OS backends.
- [oasis-models](./oasis-models/README.md) - STT/LLM/TTS engine traits and model registry.
- [oasis-engine](./oasis-engine/README.md) - the running loop and its features; the loop itself is in [src/engine](./oasis-engine/src/engine/README.md).
- [oasis-ble](./oasis-ble/README.md) - btleplug transport.
- [oasis-voice](./oasis-voice/README.md) - microphone capture and on-device speech-to-text.
- [oasis-win](./oasis-win/README.md) - safe Windows API wrappers.
- [oasis-dpapi](./oasis-dpapi/README.md) - per-user DPAPI encryption.

## Global Invariants

- `oasis-core` and `oasis-protocol` stay std-only with no I/O; only `oasis-win`, `oasis-dpapi` and `oasis-voice` may use `unsafe`.

## Global Side Effects

- None at this level; I/O is confined to `oasis-platform`, `oasis-engine`, `oasis-ble`, `oasis-voice`, `oasis-win` and `oasis-dpapi`.

## System Verification

- Run: `cargo test --workspace` (from `SW/`)
