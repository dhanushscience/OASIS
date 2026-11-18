# OASIS Hardware

Custom macro-pad PCB built around the **Seeed XIAO ESP32-C3** module.

## Project files

| File | Purpose |
|------|---------|
| `OASIS_HW.kicad_pro` | KiCad 10 project |
| `OASIS_HW.kicad_sch` | Schematic |
| `OASIS_HW.kicad_pcb` | PCB layout (2-layer, 1.6 mm) |
| `GERBER.zip` | Manufacturing export (Gerber + drill) |

Open the project in [KiCad 10](https://www.kicad.org/) or newer.

## Key components

| Ref | Part | Role |
|-----|------|------|
| XIAO | Seeed XIAO-ESP32-C3-SMD | MCU, BLE radio |
| TOE, TAC, TIC | APEM Multimec MEC tactile switches | Three macro buttons |
| OLED | 0.49″ display module | **Not fitted in this version** (see note below) |
| TYPE-C | TYPE-C-31-G-10 | USB-C power |
| BATTERY | 2-pin connector | Optional battery input |
| PWR_SW | Power switch | Main power control |

Supporting passives (pull-ups, dividers) are on the schematic.

## Manufacturing

Unzip **`GERBER.zip`** and use:

- Copper: `OASIS_HW-F_Cu.gbr`, `OASIS_HW-B_Cu.gbr`
- Masks / paste / silkscreen: `F_*` and `B_*` layers
- Drills: `OASIS_HW-PTH.drl`, `OASIS_HW-NPTH.drl`
- Job file: `OASIS_HW-job.gbrjob`

Standard stack: **2-layer FR4, 1.6 mm**.

## Resource folder

`Resource/` holds vendor footprints, symbols, and 3D models used by the design (XIAO ESP32-C3 reference, USB-C connector, switch footprints, etc.). Only the parts this board uses are kept.

## OLED not fitted

The OLED is deliberately left off this version to save battery and extend the device's runtime. The firmware runs headless with display stubs (`OASIS_HAS_DISPLAY=0`), and status is shown in the app instead. The I2C pins (GPIO6/7) are reserved for a future display.

## MCU pin map (firmware)

| GPIO | Net | Function |
|------|-----|----------|
| 3 | TIC | Button 0 (SW1) |
| 4 | TAC | Button 1 (SW2) |
| 5 | TOE | Button 2 (SW3) |
| 6 | SDA | OLED I2C (reserved, not fitted) |
| 7 | SCL | OLED I2C (reserved, not fitted) |
| 2 | B_LVL | Battery ADC (220k/220k divider) |

## After fab

1. Solder the XIAO ESP32-C3 and passives per the schematic.
2. Flash firmware from [`FW/`](../FW/) (`pio run -t upload`, ESP-IDF).
3. Pair with the OasisShell Windows app ([`../SW/`](../SW/)).
