# oasis-protocol

## Overview
The pad's wire format: the exact bytes the pad and the computer exchange over Bluetooth LE (Nordic UART service). Parses what the pad sends and formats what the app sends.

It is its own tiny, std-only crate because the format is **frozen**: firmware and app are updated separately and must keep working across mismatched versions. Tests built from the spec make an accidental change obvious in review and CI. The Rev 2 firmware can reuse it too.

| Pad to computer | Meaning |
|---|---|
| `BTN:{0-2}:D` / `:U` | Button down / up (debounced 30 ms on the pad). Bare `BTN:{i}` = down |
| `BAT:{0-100}` / `:C` / `:F` | Battery; charging; full |
| `LAT:{ms}` | Reply to `PING` |
| `PWR:*` | Power telemetry, ignored |

| Computer to pad | Meaning |
|---|---|
| `PING` | Latency probe |
| `SCR:TITLE:` / `SCR:B0..2:` / `SCR:THINK:0/1` / `SCR:CLR` | Display (never sent: shipped pads have no screen) |

## Interface
- `src/lib.rs`: `LineDecoder` (`new`, `push_notification`, `push_and_parse`); re-exports `parse_line`, `BatteryCharge`, `ButtonEdge`, `DeviceMessage`, `HostCommand`.
- `src/gatt.rs`: `SERVICE_UUID`, `RX_CHARACTERISTIC_UUID`, `TX_CHARACTERISTIC_UUID`, `DEVICE_NAME`, `LEGACY_DEVICE_NAME`, `is_oasis_ble_name`, `DEVICE_HAS_DISPLAY`.
- `src/device.rs`: `DeviceMessage` (`Button`, `Battery`, `Latency`, `Power`, `Unknown`), `ButtonEdge`, `BatteryCharge`, `parse_line` (mirrors C# `BleManager` parsing), `battery_percent_clamped`, `marks_link_active`.
- `src/host.rs`: `HostCommand` (`ScreenTitle`, `ScreenButtonLabel`, `ScreenThinking`, `ScreenClear`, `Ping`), `to_wire`, `needs_display`.

## Invariants
- All ASCII, newline-terminated. **Unknown lines are ignored, never fatal**: they become `DeviceMessage::Unknown`.
- `LineDecoder` splits on `\n` and also treats the end of a notification as the end of a line, matching how the C# app read one notification as one message.
- `HostCommand::to_wire` strips newlines from labels, so a label cannot inject a command.
- `DEVICE_HAS_DISPLAY` is false; `SCR:*` is not sent by the shipped build.
- The format is frozen; changes must stay compatible with `docs/PROTOCOLS.md` section 1.

## Side Effects
- None. Pure parsing and formatting; the BLE transport is `oasis-ble` (it implements the `PadTransport` trait from `oasis-platform`), and `oasis_app::PadLink` feeds notification bytes through `LineDecoder`.

## Verification
- Run: `cargo test -p oasis-protocol` (`tests/wire_format.rs` covers every example from `docs/PROTOCOLS.md` section 1)
