# `apps/` — programs you run

**What:** executables. Each one is thin: it starts the engine and shows what it does. No product
logic lives here.

**Why separate from `crates/`:** libraries can be tested and reused; programs are the entry
points with argument parsing, windows and event loops.

| Folder | Binary | Status | Purpose |
|---|---|---|---|
| `oasis-cli/` | `oasis` | ✅ works | Run the pad chain, scan the pad, read a browser address, run one action, replay pad traffic |
| `oasis-desktop/` | `oasis-desktop` | 🟡 first version | Tray icon and status window around the engine. No settings window or overlay yet |

What each does and does not do yet is in [`../docs/STATUS.md`](../docs/STATUS.md).

## Subdirectories & Boundaries

- `oasis-cli`: [README](./oasis-cli/README.md) - the `oasis` headless harness (replay, run, do, address, ble, pc-check, download-models); source in [src](./oasis-cli/src/README.md).
- `oasis-desktop`: [README](./oasis-desktop/README.md) - Tauri 2 tray app, window and overlay around `oasis-engine`. Parts: [src](./oasis-desktop/src/README.md), [web](./oasis-desktop/web/README.md) (Svelte page built into `ui/`), [capabilities](./oasis-desktop/capabilities/README.md), [examples](./oasis-desktop/examples/README.md).

## Global Invariants

- Apps hold no product rules; they start `oasis_engine::start` and show or drive it.
- Real actions run only when the `profiles.json` signature is valid (CLI `run`) or the app is out of preview mode (desktop).

## Global Side Effects

- Both programs can hold the pad's BLE connection and run real actions; `oasis-cli download-models` and the desktop updater use the network.

## System Verification

- Run: `cargo test --workspace` (from `SW/`)
