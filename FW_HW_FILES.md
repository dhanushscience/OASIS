# Firmware and hardware: required files

Only what is needed to build the firmware and to open, edit and manufacture the board.

## FW/ (ESP-IDF, Seeed XIAO ESP32-C3)

| File | Purpose |
|------|---------|
| `platformio.ini` | PlatformIO project, ESP-IDF framework, board `seeed_xiao_esp32c3` |
| `CMakeLists.txt`, `src/CMakeLists.txt` | ESP-IDF build |
| `sdkconfig.defaults`, `sdkconfig.seeed_xiao_esp32c3` | Chip and power settings (NimBLE, 80 MHz, light sleep) |
| `src/main.c` | Entry point and main loop |
| `src/ble_uart.*` | NimBLE Nordic UART link |
| `src/buttons.*` | Three buttons, 30 ms debounce |
| `src/battery.*` | ADC battery level (GPIO2) |
| `src/protocol.*` | `BTN:`, `BAT:`, `SCR:*`, `PING` messages |
| `src/power.*`, `power_mgr.*`, `power_debug.*` | Power states and sleep |
| `src/oasis_config.h`, `oasis_pins.h` | Feature switches and pin map |
| `src/display*.c/h`, `ui_draw.*`, `font*`, `logo_oasis.*` | Optional OLED code (display is off by default, stubbed) |
| `README.md` | Features, power profile, build and flash steps |

Build: `cd FW && pio run`, flash with `pio run -t upload`.

## HW/ (KiCad 10)

| File | Purpose |
|------|---------|
| `OASIS_HW.kicad_pro` / `.kicad_sch` / `.kicad_pcb` | Project, schematic, 2-layer 1.6 mm board |
| `fp-lib-table`, `sym-lib-table`, `design-block-lib-table` | Library paths (relative to the project) |
| `Resource/` | Only the footprints, symbols and 3D models the board uses (XIAO C3, USB-C, tactile switch) |
| `GERBER.zip` | Manufacturing export (Gerber and drill, version 1) |
| `SCH_OASIS.pdf` | Schematic printout |
| `OASIS.step` | 3D model of the assembled board |
| `3D/` | Extra 3D preview files (`.usdz`, `.mtl`) |
| `README.md` | Parts list, pin map, manufacturing notes |

Pin map: GPIO3 button 0, GPIO4 button 1, GPIO5 button 2, GPIO6/7 OLED I2C (reserved; OLED not fitted in this version to save battery), GPIO2 battery divider.
