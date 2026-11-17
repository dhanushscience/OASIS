# `oasis-ble` — Bluetooth LE transport for the pad

## Overview

**What:** `BleTransport`, the real implementation of `oasis_platform::ports::PadTransport`. It
scans for the pad, connects, subscribes to the Nordic UART TX characteristic and writes to RX.
Port of the Bluetooth half of `Services/BleManager.cs`.

**Why:** `btleplug` is one BLE library for Windows, macOS and Linux, so the pad link is written
once. Its async runtime (tokio) is kept inside this crate; everything else stays runtime-free
(DECISIONS D3, D14).

**When:** created at startup, owned by `PadConnector`, which calls `scan`, `connect`,
`next_notification` and `write` from its poll loop.

## Interface

**How:**
- A pad's id is its Bluetooth address as a decimal number, the same text the C# app saved in
  `trusted_device.json`.
- A device counts as a pad if it advertises an `OASIS*` name or the Nordic UART service.
- After connecting it looks for the RX/TX characteristics (up to 4 tries, like C#), and
  subscribes **before** returning so the first `BAT:` message is not lost.
- Writes go without response when the pad allows it.
- A dropped link shows up as the notification stream ending; `is_connected` turns false and the
  connector starts over.
- *When* to connect, back off or ping is **not** decided here: see `oasis_core::pad_connect` and
  `oasis_platform::pad_connector`.

| File | Contents |
|---|---|
| `src/lib.rs` | `BleTransport`, id and write-type helpers, tests of those |

Try it with a real pad: `oasis ble-scan` and `oasis ble-listen` (see `docs/WINDOWS_BACKEND.md`).

Public symbols: `BleTransport` (`new`; implements `PadTransport`), `pad_id`.

## Invariants

- The tokio runtime stays inside this crate.
- The TX subscription is made before `connect` returns, so the first `BAT:` line is not lost.
- Connection policy (when to connect, back off, ping) lives elsewhere, not here.

## Side Effects

- Bluetooth scanning, connecting and GATT writes; runs an internal tokio runtime.

## Verification

- Run: `cargo test -p oasis-ble`
