#include "battery.h"
#include "oasis_pins.h"

#include "driver/usb_serial_jtag.h"
#include "esp_adc/adc_cali.h"
#include "esp_adc/adc_cali_scheme.h"
#include "esp_adc/adc_oneshot.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

static const char *TAG = "battery";

static adc_oneshot_unit_handle_t s_adc_handle;
static adc_cali_handle_t s_adc_cali_handle;
static bool s_adc_ready;

static int s_reported_pct = -1;
static int s_resting_pct = -1;
static int s_filtered_mv = -1;
static int s_charge_anchor_soc = -1;
static int s_charge_plug_mv = -1;
static uint32_t s_settle_until_ms = 0;
static bool s_prev_usb_present = false;
static bool s_charge_complete = false;
static uint32_t s_near_full_since_ms = 0;
static int s_near_full_dip_count = 0;

static bool s_usb_raw = false;
static bool s_usb_debounced = false;
static uint32_t s_usb_last_change_ms = 0;

static int s_prev_pin_mv = -1;

static bool s_boot_settle_armed = true;
static bool s_in_settle_prev = false;
static bool s_pending_charge_begin = false;
static int s_pending_anchor_soc = -1;
static bool s_pending_ocv_commit = false;

static bool init_adc_calibration(void) {
    adc_oneshot_unit_init_cfg_t unit_cfg = {
        .unit_id = ADC_UNIT_1,
    };
    if (adc_oneshot_new_unit(&unit_cfg, &s_adc_handle) != ESP_OK) {
        return false;
    }

    adc_oneshot_chan_cfg_t chan_cfg = {
        .bitwidth = ADC_BITWIDTH_DEFAULT,
        .atten = ADC_ATTEN_DB_12,
    };
    if (adc_oneshot_config_channel(s_adc_handle, ADC_CHANNEL_2, &chan_cfg) != ESP_OK) {
        return false;
    }

    adc_cali_curve_fitting_config_t cali_cfg = {
        .unit_id = ADC_UNIT_1,
        .chan = ADC_CHANNEL_2,
        .atten = ADC_ATTEN_DB_12,
        .bitwidth = ADC_BITWIDTH_DEFAULT,
    };
    if (adc_cali_create_scheme_curve_fitting(&cali_cfg, &s_adc_cali_handle) != ESP_OK) {
        ESP_LOGW(TAG, "ADC calibration unavailable — using raw conversion");
        s_adc_cali_handle = NULL;
    }

    for (int i = 0; i < 4; ++i) {
        int dummy = 0;
        adc_oneshot_read(s_adc_handle, ADC_CHANNEL_2, &dummy);
        vTaskDelay(pdMS_TO_TICKS(10));
    }

    return true;
}

void battery_init(void) {
    s_adc_ready = init_adc_calibration();
    if (!s_adc_ready) {
        ESP_LOGE(TAG, "Battery ADC init failed");
    }
}

static uint32_t battery_now_ms(void) {
    return (uint32_t)(xTaskGetTickCount() * portTICK_PERIOD_MS);
}

static int battery_read_pin_mv(void) {
    if (!s_adc_ready) {
        return 0;
    }

    int total_mv = 0;
    int samples = 0;
    for (int i = 0; i < BATTERY_ADC_SAMPLES; ++i) {
        int raw = 0;
        if (adc_oneshot_read(s_adc_handle, ADC_CHANNEL_2, &raw) != ESP_OK) {
            continue;
        }

        int mv = 0;
        if (s_adc_cali_handle != NULL &&
            adc_cali_raw_to_voltage(s_adc_cali_handle, raw, &mv) == ESP_OK) {
            total_mv += mv;
            samples++;
            continue;
        }

        total_mv += (raw * 3300) / 4095;
        samples++;
    }

    if (samples == 0) {
        return 0;
    }

    return total_mv / samples;
}

static int clamp_pct(int pct) {
    if (pct < 0) {
        return 0;
    }
    if (pct > 100) {
        return 100;
    }
    return pct;
}

static int mv_to_percent(int mv) {
    if (mv <= BATTERY_ADC_EMPTY_MV) {
        return 0;
    }
    if (mv >= BATTERY_ADC_FULL_MV) {
        return 100;
    }
    const int range = BATTERY_ADC_FULL_MV - BATTERY_ADC_EMPTY_MV;
    return ((mv - BATTERY_ADC_EMPTY_MV) * 100) / range;
}

static int mv_to_percent_x10(int mv) {
    if (mv <= BATTERY_ADC_EMPTY_MV) {
        return 0;
    }
    if (mv >= BATTERY_ADC_FULL_MV) {
        return 1000;
    }
    const int range = BATTERY_ADC_FULL_MV - BATTERY_ADC_EMPTY_MV;
    return ((mv - BATTERY_ADC_EMPTY_MV) * 1000) / range;
}

static void update_filtered_mv(int mv) {
    if (s_filtered_mv < 0) {
        s_filtered_mv = mv;
        return;
    }

    const int alpha = (1 << BATTERY_ADC_EMA_SHIFT) - 1;
    s_filtered_mv = ((s_filtered_mv * alpha) + mv) >> BATTERY_ADC_EMA_SHIFT;
}

static void update_usb_debounce(uint32_t now_ms) {
    const bool raw = usb_serial_jtag_is_connected();
    if (raw != s_usb_raw) {
        s_usb_raw = raw;
        s_usb_last_change_ms = now_ms;
    }

    if ((now_ms - s_usb_last_change_ms) >= BATTERY_USB_DEBOUNCE_MS) {
        s_usb_debounced = s_usb_raw;
    }
}

static bool is_near_full_reading(int raw_pct, int mv) {
    return raw_pct >= BATTERY_CHARGE_FULL_RAW_PCT ||
           mv >= (BATTERY_ADC_FULL_MV - BATTERY_CHARGE_FULL_MV_MARGIN);
}

static int apply_charge_inflation(int raw_pct) {
    if (raw_pct <= BATTERY_CHARGE_INFLATION_TAPER_START_PCT) {
        return clamp_pct(raw_pct - BATTERY_CHARGE_INFLATION_PCT);
    }

    const int range =
        BATTERY_CHARGE_FULL_RAW_PCT - BATTERY_CHARGE_INFLATION_TAPER_START_PCT;
    const int tapered_raw = raw_pct > BATTERY_CHARGE_FULL_RAW_PCT
                                ? BATTERY_CHARGE_FULL_RAW_PCT
                                : raw_pct;
    const int excess = tapered_raw - BATTERY_CHARGE_INFLATION_TAPER_START_PCT;
    const int inflation =
        BATTERY_CHARGE_INFLATION_PCT -
        (excess * BATTERY_CHARGE_INFLATION_PCT) / range;
    return clamp_pct(raw_pct - inflation);
}

static void begin_charge_session(int anchor_soc, int plug_mv) {
    s_charge_anchor_soc = clamp_pct(anchor_soc);
    s_charge_plug_mv = plug_mv;
    ESP_LOGI(TAG, "Charge session anchor %d%% @ %dmV",
             s_charge_anchor_soc, s_charge_plug_mv);
}

static void end_charge_session(void) {
    s_charge_anchor_soc = -1;
    s_charge_plug_mv = -1;
}

/** SOC while charging: anchor at plug-in level + conservative credit for mV rise. */
static int charge_session_target(int filtered_mv) {
    if (s_charge_anchor_soc < 0 || s_charge_plug_mv < 0) {
        return -1;
    }

    if (s_charge_complete) {
        return 100;
    }

    int progress_pct = 0;
    const int mv_delta = filtered_mv - s_charge_plug_mv;
    if (mv_delta > 0) {
        const int range = BATTERY_ADC_FULL_MV - BATTERY_ADC_EMPTY_MV;
        progress_pct = (mv_delta * 100) / range;
        progress_pct /= BATTERY_CHARGE_MV_CREDIT_DIVISOR;
    }

    int target = s_charge_anchor_soc + progress_pct;
    if (target > 99) {
        target = 99;
    }
    if (s_reported_pct >= 0 && target < s_reported_pct) {
        target = s_reported_pct;
    }
    return clamp_pct(target);
}

static void update_charge_complete(bool usb_present, int raw_pct, int mv, uint32_t now_ms) {
    if (!usb_present) {
        s_charge_complete = false;
        s_near_full_since_ms = 0;
        s_near_full_dip_count = 0;
        return;
    }

    if (!is_near_full_reading(raw_pct, mv)) {
        /* Tolerate brief voltage dips (e.g. BLE TX spike) during near-full hold
         * instead of resetting the timer immediately. */
        if (s_near_full_since_ms != 0 &&
            s_near_full_dip_count < BATTERY_CHARGE_COMPLETE_DIP_MAX) {
            s_near_full_dip_count++;
            return;
        }
        s_charge_complete = false;
        s_near_full_since_ms = 0;
        s_near_full_dip_count = 0;
        return;
    }
    s_near_full_dip_count = 0;

    if (s_near_full_since_ms == 0) {
        s_near_full_since_ms = now_ms;
        return;
    }

    if (!s_charge_complete &&
        (now_ms - s_near_full_since_ms) >= BATTERY_CHARGE_COMPLETE_HOLD_MS) {
        s_charge_complete = true;
        ESP_LOGI(TAG, "Charge complete inferred");
    }
}

static int apply_display_hysteresis(int reported,
                                    int filtered_pct_x10,
                                    int max_target,
                                    bool allow_decrease) {
    if (reported < 0) {
        return clamp_pct(max_target);
    }

    int next = reported;
    const int up_threshold = reported * 10 + BATTERY_PCT_HYSTERESIS_X10;
    if (filtered_pct_x10 >= up_threshold && next < max_target && next < 100) {
        next++;
    }

    if (allow_decrease) {
        const int down_threshold = reported * 10 - BATTERY_PCT_HYSTERESIS_X10;
        if (filtered_pct_x10 <= down_threshold && next > 0 && next > max_target) {
            next--;
        }
    }

    if (next > max_target) {
        next = max_target;
    }
    return clamp_pct(next);
}

bool battery_is_usb_present(void) {
    return s_usb_debounced;
}

bool battery_is_charge_active(void) {
    return battery_is_usb_present() && !s_charge_complete;
}

bool battery_is_charging(void) {
    return battery_is_charge_active();
}

bool battery_is_adc_settled(void) {
    return battery_now_ms() >= s_settle_until_ms;
}

static void on_adc_settle_complete(bool usb_present,
                                   int filtered_mv,
                                   int raw_pct,
                                   int filtered_pct_x10) {
    if (s_pending_charge_begin && usb_present) {
        int anchor = s_pending_anchor_soc;
        if (anchor < 0) {
            anchor = clamp_pct(raw_pct);
        }
        begin_charge_session(anchor, filtered_mv);
        s_pending_charge_begin = false;
        ESP_LOGI(TAG, "Charge settle done — anchor %d%% @ %dmV (raw=%d%%)",
                 anchor, filtered_mv, raw_pct);
    }

    if (s_pending_ocv_commit && !usb_present) {
        s_resting_pct = clamp_pct(raw_pct);
        if (s_reported_pct >= 0) {
            s_reported_pct = apply_display_hysteresis(
                s_reported_pct, filtered_pct_x10, s_resting_pct, true);
        } else {
            s_reported_pct = s_resting_pct;
        }
        s_pending_ocv_commit = false;
        ESP_LOGI(TAG, "OCV settle done — SOC %d%% (pin=%dmV)",
                 s_reported_pct, filtered_mv);
        return;
    }

    if (s_reported_pct < 0 && !usb_present) {
        s_resting_pct = clamp_pct(raw_pct);
        s_reported_pct = s_resting_pct;
        ESP_LOGI(TAG, "Boot settle done — SOC %d%% (pin=%dmV)",
                 s_reported_pct, filtered_mv);
    }
}

static int battery_read_percent_settled(bool usb_present,
                                        int filtered_mv,
                                        int raw_pct,
                                        int filtered_pct_x10) {
    update_charge_complete(usb_present, raw_pct, filtered_mv, battery_now_ms());

    if (usb_present) {
        int target = charge_session_target(filtered_mv);
        if (target < 0) {
            target = apply_charge_inflation(raw_pct);
            if (!s_charge_complete && target >= 100) {
                target = 99;
            }
        }

        s_reported_pct = apply_display_hysteresis(
            s_reported_pct, filtered_pct_x10, target, false);
        return s_reported_pct;
    }

    if (s_resting_pct < 0) {
        s_resting_pct = raw_pct;
    } else {
        s_resting_pct = (s_resting_pct * 3 + raw_pct) / 4;
    }
    s_resting_pct = clamp_pct(s_resting_pct);
    s_reported_pct = apply_display_hysteresis(
        s_reported_pct,
        filtered_pct_x10,
        s_resting_pct,
        true);

    return s_reported_pct >= 0 ? s_reported_pct : clamp_pct(raw_pct);
}

int battery_read_percent(void) {
    const uint32_t now_ms = battery_now_ms();

    if (s_boot_settle_armed) {
        s_boot_settle_armed = false;
        s_settle_until_ms = now_ms + BATTERY_BOOT_SETTLE_MS;
        ESP_LOGI(TAG, "Boot ADC settle %d ms", BATTERY_BOOT_SETTLE_MS);
    }

    update_usb_debounce(now_ms);

    const int mv = battery_read_pin_mv();
    update_filtered_mv(mv);

    const int filtered_mv = s_filtered_mv;
    const int raw_pct = mv_to_percent(filtered_mv);
    const int filtered_pct_x10 = mv_to_percent_x10(filtered_mv);
    bool usb_present = s_usb_debounced;

    /* P4: Voltage-rise heuristic — infer USB plug from a sharp mV jump
     * even when usb_serial_jtag_is_connected() returns false (wall charger). */
    if (!usb_present && s_prev_pin_mv >= 0 &&
        (mv - s_prev_pin_mv) >= BATTERY_CHARGE_DETECT_RISE_MV &&
        now_ms >= s_settle_until_ms) {
        ESP_LOGI(TAG, "Voltage rise %d->%d mV — inferring USB plug",
                 s_prev_pin_mv, mv);
        s_usb_debounced = true;
        usb_present = true;
    }
    s_prev_pin_mv = mv;

    if (s_prev_usb_present && !usb_present) {
        s_settle_until_ms = now_ms + BATTERY_SETTLE_MS;
        end_charge_session();
        s_filtered_mv = -1;  /* P2: flush inflated charger voltage from EMA */
        s_pending_charge_begin = false;
        s_pending_ocv_commit = true;
        ESP_LOGI(TAG, "USB unplugged — OCV settle %d ms (pin=%dmV raw=%d%%)",
                 BATTERY_SETTLE_MS, filtered_mv, raw_pct);
    } else if (!s_prev_usb_present && usb_present) {
        s_settle_until_ms = now_ms + BATTERY_CHARGE_SETTLE_MS;
        s_pending_ocv_commit = false;
        s_pending_charge_begin = true;
        s_pending_anchor_soc = s_resting_pct >= 0 ? s_resting_pct
                              : s_reported_pct >= 0 ? s_reported_pct
                                                    : -1;
        ESP_LOGI(TAG, "USB plugged — charger settle %d ms (pin=%dmV raw=%d%%)",
                 BATTERY_CHARGE_SETTLE_MS, filtered_mv, raw_pct);
    }
    s_prev_usb_present = usb_present;

    const bool in_settle = now_ms < s_settle_until_ms;
    if (s_in_settle_prev && !in_settle) {
        on_adc_settle_complete(usb_present, filtered_mv, raw_pct, filtered_pct_x10);
    }
    s_in_settle_prev = in_settle;

    if (in_settle) {
        /* P6: Return preliminary SOC during settle instead of -1 */
        return s_reported_pct >= 0 ? s_reported_pct : clamp_pct(raw_pct);
    }

    return battery_read_percent_settled(
        usb_present, filtered_mv, raw_pct, filtered_pct_x10);
}
