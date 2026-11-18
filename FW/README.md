# OASIS Firmware (ESP-IDF)

Native **ESP-IDF** firmware for the Seeed XIAO ESP32-C3 — headless, always-on, battery optimized.

## Features (all retained)

- **NimBLE** Nordic UART — buttons, battery, screen protocol (for app-side display)
- **ADC battery** sense on GPIO2 → `BAT:{percent}\n` to OasisShell
- **Three buttons** with debounce → `BTN:{n}:D|U\n`
- **Protocol** — `SCR:TITLE`, `SCR:B0..B2`, `SCR:THINK`, `SCR:CLR` (no local OLED required)

## Power profile (headless)

Tiered BLE power state machine (`power_mgr.c`):

| State | Behavior |
|-------|----------|
| **AdvFast** | 30–60 ms advertising for 2 min after wake or disconnect |
| **AdvSlow** | 400–800 ms advertising for up to 15 min |
| **AdvOff** | Advertising stopped; CPU light sleep; GPIO wake on buttons 3/4/5 |
| **ConnectedActive** | 10–15 ms conn interval, latency 0 (button use) |
| **ConnectedIdle** | 150–200 ms conn interval, latency 8 + modem sleep after 30 s quiet |

| Setting | Value |
|---------|--------|
| CPU | 80 MHz max, light sleep between idle |
| OLED / I2C | **Off** (`OASIS_HAS_DISPLAY=0`, display stubs) |
| BLE modem sleep | Enabled when connected idle |
| Main loop | 50 ms idle / 15 ms when connected, fast adv, or button held |
| Battery ADC | 60 s when connected, 5 min when disconnected; skipped in AdvOff |
| Debounce | 30 ms |

AdvOff does **not** deep sleep — NimBLE stays loaded so the first button press after idle starts fast advertising in ~50–200 ms (no reboot delay).

### Manual power measurement (before enabling runtime estimate in app)

Measure average mA per state with a multimeter or USB power meter:

1. **AdvFast** — first 2 min after button press
2. **AdvSlow** — after 2 min, before 15 min
3. **AdvOff** — after 15 min idle (no advertising)
4. **ConnectedActive** — linked, pressing buttons
5. **ConnectedIdle** — linked, 20+ s no buttons

Set `BatteryRuntimeEstimator.ShowBatteryRuntimeEstimate = true` in OasisShell after updating `AssumedDrawMa`.

Set `OASIS_HAS_DISPLAY=1` in `src/oasis_config.h` and restore display sources in `CMakeLists.txt` to re-enable the OLED build.

## Flash

```bash
cd FW
pio run -t upload
pio device monitor
```

After changing `sdkconfig.defaults`, run `pio run -t clean` once, then `pio run -t upload`.

Build output is stored outside OneDrive (`platformio.ini` → `build_dir`) to avoid CMake stalls on synced folders.

## BLE protocol

Same as `FW/README.md`: `BTN:`, `BAT:`, `SCR:*`, `PING` → `LAT:{ms}`.
