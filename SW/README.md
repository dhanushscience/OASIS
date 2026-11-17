# OASIS — Rust companion app (`SW/`)

This folder is the Rust rebuild of the OASIS companion app: the program that sits on your
computer, listens to the three-button BLE pad, works out what each press means, and does it
(switch profile, run a macro, hold-for-2× on YouTube, push-to-talk voice, show desktop).

The original is the C# / WPF app in `OASIS/SW` (Windows only). This rebuild keeps the same
behaviour, same file locations and same pad protocol, and adds **macOS and Linux**.

> **Status:** on Windows the pad works end to end: it connects, understands taps and holds,
> picks the profile for the app or website in front of you, and runs the button's action. A small
> desktop app (tray icon and status window) runs it. It cannot replace the C# app yet: no
> settings window, overlay, voice, AI or YouTube gestures. **Read
> [`docs/STATUS.md`](docs/STATUS.md)** for what is done, how sure we are, and what is missing;
> [`docs/PORTING.md`](docs/PORTING.md) has the file-by-file detail.

---

## Folder map

Every folder has its own `README.md` answering four questions: **what** it is, **why** it
exists, **when** it runs, and **how** it works. Start there when you open a folder.

```
SW/
├── Cargo.toml              Workspace: lists every crate, shared version/lints
├── Cargo.lock              Exact dependency versions (committed: this is an application)
├── README.md               This file
├── .gitignore              Keeps target/ and secrets out of git
├── ci/                     GitHub Actions workflow for Windows + macOS + Linux (copy to repo root)
├── docs/                   Status, design docs: STATUS first, then porting, decisions, architecture
├── crates/                 Libraries — all the logic lives here
│   ├── oasis-core/         Pure rules: gestures, actions, profile matching, shortcuts (no I/O)
│   ├── oasis-protocol/     The pad's wire format over BLE (frozen, std-only)
│   ├── oasis-app/          Hosts that run the rules, own timers, track capabilities
│   ├── oasis-settings/     profiles.json: read, upgrade, profile matcher, gesture settings
│   ├── oasis-platform/     OS-facing code: paths, storage, signing, pad connection logic, per-OS backends
│   ├── oasis-models/       Swappable speech / language / voice models (the structure, no engines yet)
│   ├── oasis-engine/       The running OASIS: pad → gestures → profile → action, in one loop
│   ├── oasis-ble/          Bluetooth transport (btleplug)
│   ├── oasis-voice/        Microphone and on-device speech-to-text (sherpa-onnx loaded at run time)
│   ├── oasis-win/          Small safe wrappers over Windows calls (windows, keys, shell, UI Automation)
│   └── oasis-dpapi/        Windows per-user encryption, for secrets and the settings signature
└── apps/                   Programs you run
    ├── oasis-cli/          `oasis` — run the chain, scan the pad, read a browser address, replay traffic
    └── oasis-desktop/      The tray app and status window (Tauri 2). Settings window and overlay come next
```

### How a button press flows through the folders

```
 pad (BLE)                       oasis-platform         PadTransport trait (per-OS Bluetooth)
   │  bytes "BTN:1:D\n"                │
   ▼                                   ▼
 oasis-protocol   LineDecoder → parse_line → DeviceMessage::Button
   ▼
 oasis-app        PadLink → PadEvent::Down  →  ArbiterHost (owns state + timers)
   ▼                                             │ calls
 oasis-core       input::reduce(state, event) ───┘ returns next state + effects
   ▼
 oasis-app        effects: DispatchCommand / StartVoiceCapture / SendToDevice …
   ▼
 oasis-platform   InputInjector, MediaSessions, … (per-OS) carry them out
```

The rules in `oasis-core` never touch the OS. That is why they can be tested exhaustively
and why the same rules run unchanged on Windows, macOS and Linux.

---

## Build, test, run

Needs Rust 1.82 or newer (`rustup`, https://rustup.rs). Same commands on every OS.

```sh
cargo test --workspace                     # every test (ported C# tests included)
cargo clippy --workspace --all-targets -- -D warnings   # zero warnings, like the C# build
cargo run -p oasis-cli -- status           # what works on this OS today
cargo run -p oasis-cli -- paths            # where OASIS keeps files here
cargo run -p oasis-desktop                 # the tray app (Windows); starts in preview mode
cargo run -p oasis-cli -- run --dry-run 60 # press the pad: shows what each press would do
printf '0 BTN:1:D\n400 BTN:1:U\n' | cargo run -p oasis-cli -- replay --youtube
```

The desktop page is a Svelte project in `apps/oasis-desktop/web`; the built output is committed in `ui/`, so `cargo run` needs no Node. Rebuild it with `npm run build` in `web/` after changing the page.

The `replay` example prints:

```
   250 ms  extension: playbackRate.hold2x
   400 ms  extension: playbackRate.restore
```

## Subdirectories & System Boundaries

- [crates](./crates/README.md) - the eleven libraries holding all logic.
- [apps](./apps/README.md) - `oasis-cli` and `oasis-desktop`, thin programs over the engine.
- [ci](./ci/README.md) - the GitHub Actions workflow.
- [scripts](./scripts/README.md) - release, packaging and model-publishing scripts (with [bench](./scripts/bench/README.md)).
- [extensions/oasis-youtube](./extensions/oasis-youtube/README.md) - browser extension that talks to the local bridge.

## Global Side Effects

- Runtime: BLE pad connection, input injection, microphone, local websocket bridge, and network for updates, models and accounts. All OS access goes through `oasis-platform` / `oasis-win`.

## Rules carried over from the C# app

These come from `OASIS/CLAUDE.md` and still apply:

- **Zero warnings.** CI runs clippy with `-D warnings`.
- **`oasis-core` and `oasis-protocol` stay pure** — std only, no OS calls, no I/O.
- **Behaviour is the contract.** Pin behaviour with a test before changing it. The C# tests
  were ported with the same names so you can compare them side by side.
- **Offline is first-class.** Pad, models, API keys and cloud are optional. Missing → an
  honest `CapabilityState::Missing` with a reason. **Never auto-download a model.**
- **The BLE wire format is frozen** (`crates/oasis-protocol`).
- **Secrets never go in `profiles.json`** — they go through the `SecretStore` trait.
- Keep files small (the C# ratchet was 500 lines per file); split by feature.
