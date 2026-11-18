#pragma once

// OASIS PCB — Seeed XIAO ESP32-C3 (OASIS_HW.kicad_pcb)
// OLED: GoldenMorning SSD1315 64x32 @ I2C 0x3C (SSD1306-compatible init — see lab/01-display-ssd1315)

#define PIN_BTN_0       3
#define PIN_BTN_1       4
#define PIN_BTN_2       5
#define PIN_I2C_SDA     6
#define PIN_I2C_SCL     7
#define PIN_BATTERY_ADC 2

#define BUTTON_COUNT    3
#define DEBOUNCE_MS     30

#define OLED_I2C_ADDR   0x3C
#define OLED_WIDTH      64
#define OLED_HEIGHT     32

// Lab-tested SSD1315 / SSD1306 64x32 panel tuning (lab/01-display-ssd1315/src/pins.h)
#define OLED_COL_OFFSET     32
#define OLED_MUX_RATIO      0x1F
#define OLED_COM_PINS       0x12
#define OLED_DISPLAY_OFFSET 0x00

#define BATTERY_ADC_EMPTY_MV 1500
#define BATTERY_ADC_FULL_MV  2040

// LiPo terminal voltage reads high while USB charging; subtract when reporting SOC.
#define BATTERY_CHARGE_INFLATION_PCT       7
// Taper inflation to zero between these raw ADC percents (avoids 100% -> 93% ceiling).
#define BATTERY_CHARGE_INFLATION_TAPER_START_PCT 90
#define BATTERY_CHARGE_FULL_RAW_PCT        97
#define BATTERY_CHARGE_FULL_MV_MARGIN      30
// USB present + ADC near full for this long => charge complete (matches CHG LED off).
#define BATTERY_CHARGE_COMPLETE_HOLD_MS    10000
// Tolerate brief voltage dips during near-full hold (BLE TX spikes etc.).
#define BATTERY_CHARGE_COMPLETE_DIP_MAX    2
// Credit only part of mV rise during charge as real SOC gain (charger lifts terminal V).
#define BATTERY_CHARGE_MV_CREDIT_DIVISOR   2
// Hold SOC steady after USB unplug until cell OCV settles (LiPo needs 3-5 s).
#define BATTERY_SETTLE_MS            4000
// Hold SOC steady after USB plug until charger/load transient settles.
#define BATTERY_CHARGE_SETTLE_MS     800
// Wait for ADC / cell OCV to stabilize after boot before first SOC commit.
#define BATTERY_BOOT_SETTLE_MS         3000
// Defer first BLE battery notify so ADC can stabilize after link-up.
#define BATTERY_CONNECT_SETTLE_MS    2000
// Limit how fast displayed SOC can change (prevents instant jumps).
#define BATTERY_SOC_MAX_STEP_PCT     2
// Number of ADC samples averaged per reading (more = less noise).
#define BATTERY_ADC_SAMPLES          16
// EMA on ADC pin mV: new = (old * (2^SHIFT - 1) + sample) / 2^SHIFT
#define BATTERY_ADC_EMA_SHIFT        5
// Hysteresis band (tenths of a percent) before displayed SOC steps.
#define BATTERY_PCT_HYSTERESIS_X10   12
// Debounce USB-serial presence before charge/discharge transitions.
#define BATTERY_USB_DEBOUNCE_MS    300
// Voltage jump (mV) that infers USB plug even without host enumeration.
#define BATTERY_CHARGE_DETECT_RISE_MV  40

#define USB_SETTLE_MS            300
#define BLE_START_MS             0

// Event-only BLE battery telemetry (no periodic notify while connected).
#define BAT_NOTIFY_RETRY_MS        500
// Hold a battery snapshot steady before BLE notify (stops 80/81% flutter).
#define BAT_NOTIFY_STABLE_MS       2000
// Shorter hold for charge-flag edges (charging <-> full / idle).
#define BAT_CHARGE_FLAG_STABLE_MS  750

#define APP_LOOP_IDLE_MS         50
#define APP_LOOP_ACTIVE_MS       15

#define ADV_FAST_MIN_MS          30
#define ADV_FAST_MAX_MS          60
#define ADV_SLOW_MIN_MS          400
#define ADV_SLOW_MAX_MS          800
#define ADV_FAST_DURATION_MS     120000
#define ADV_OFF_AFTER_MS         900000

#define CONN_ACTIVE_ITVL_MIN     8
#define CONN_ACTIVE_ITVL_MAX     12
#define CONN_ACTIVE_LATENCY      0
#define CONN_IDLE_ITVL_MIN       120
#define CONN_IDLE_ITVL_MAX       160
#define CONN_IDLE_LATENCY        8
#define CONN_IDLE_AFTER_MS       30000
#define BLE_CONN_SUPERVISION     600
