#include "battery.h"
#include "ble_uart.h"
#include "buttons.h"
#include "display.h"
#include "oasis_config.h"
#include "oasis_pins.h"
#include "power.h"
#include "power_mgr.h"
#include "power_debug.h"
#include "protocol.h"

#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include "freertos/task.h"
#include "nvs_flash.h"

#include <stdio.h>
#include <string.h>

static const char *TAG = "oasis";

static bool s_ble_started = false;
static int s_last_btn[BUTTON_COUNT] = {1, 1, 1};
static int s_last_notified_pct = -1;
static bool s_last_notified_charging = false;
static bool s_last_notified_full = false;
static bool s_last_usb_present = false;
static bool s_last_charge_active = false;
static bool s_bat_send_asap = false;
static uint32_t s_bat_retry_after_ms = 0;
static uint32_t s_bat_ready_after_ms = 0;
static uint32_t s_notify_stable_ms = 0;
static int s_pending_pct = -1;
static bool s_pending_charge_active = false;
static bool s_pending_charge_full = false;
static uint32_t s_pending_since_ms = 0;

static void send_battery_notification(int pct, bool charge_active, bool charge_full) {
    if (!ble_uart_is_connected()) {
        return;
    }
    char msg[20];
    if (charge_active) {
        snprintf(msg, sizeof(msg), "BAT:%d:C\n", pct);
    } else if (charge_full) {
        snprintf(msg, sizeof(msg), "BAT:%d:F\n", pct);
    } else {
        snprintf(msg, sizeof(msg), "BAT:%d\n", pct);
    }
    if (ble_uart_notify((const uint8_t *)msg, strlen(msg)) >= 0) {
        s_last_notified_pct = pct;
        s_last_notified_charging = charge_active;
        s_last_notified_full = charge_full;
        s_bat_send_asap = false;
        s_bat_retry_after_ms = 0;
        ESP_LOGI(TAG, "BAT event TX %s", msg);
    } else {
        ESP_LOGW(TAG, "BAT notify failed for %d%%", pct);
    }
}

static void push_battery_telemetry(int pct, bool charge_active, bool charge_full, bool force) {
    display_set_battery_percent(pct);
    if (!ble_uart_is_connected()) {
        return;
    }
    if (!force &&
        pct == s_last_notified_pct &&
        charge_active == s_last_notified_charging &&
        charge_full == s_last_notified_full) {
        return;
    }
    send_battery_notification(pct, charge_active, charge_full);
}

static void request_battery_notify(void) {
    if (!ble_uart_is_connected()) {
        return;
    }
    s_bat_send_asap = true;
    s_bat_retry_after_ms = 0;
    s_bat_ready_after_ms = buttons_millis() + BATTERY_CONNECT_SETTLE_MS;
    s_notify_stable_ms = 0;
    s_pending_pct = -1;
    s_pending_charge_active = false;
    s_pending_charge_full = false;
    s_pending_since_ms = 0;
    s_last_notified_pct = -1;
    s_last_notified_charging = false;
    s_last_notified_full = false;
}

static void clear_battery_notify_state(void) {
    s_bat_send_asap = false;
    s_bat_retry_after_ms = 0;
    s_bat_ready_after_ms = 0;
    s_notify_stable_ms = 0;
    s_pending_pct = -1;
    s_pending_charge_active = false;
    s_pending_charge_full = false;
    s_pending_since_ms = 0;
    s_last_notified_pct = -1;
    s_last_notified_charging = false;
    s_last_notified_full = false;
}

static void note_battery_snapshot(int pct,
                                  bool charge_active,
                                  bool charge_full,
                                  uint32_t now_ms) {
    if (pct != s_pending_pct ||
        charge_active != s_pending_charge_active ||
        charge_full != s_pending_charge_full) {
        s_pending_pct = pct;
        s_pending_charge_active = charge_active;
        s_pending_charge_full = charge_full;
        s_pending_since_ms = now_ms;
    }
}

static bool battery_snapshot_is_stable(uint32_t now_ms,
                                       int pct,
                                       bool charge_active,
                                       bool charge_full) {
    if (pct != s_pending_pct ||
        charge_active != s_pending_charge_active ||
        charge_full != s_pending_charge_full) {
        return false;
    }
    if (s_notify_stable_ms == 0) {
        return true;
    }
    return (now_ms - s_pending_since_ms) >= s_notify_stable_ms;
}

static void on_ble_connection_changed(bool connected) {
    if (connected) {
        request_battery_notify();
        power_mgr_on_connect();
        ESP_LOGI(TAG, "BLE linked — battery notify queued");
    } else {
        clear_battery_notify_state();
        power_mgr_on_disconnect();
        ESP_LOGI(TAG, "BLE unlinked — battery notify stopped");
    }
}

static void on_ble_notify_subscribed(void) {
    request_battery_notify();
    ESP_LOGI(TAG, "Notify subscribed — sending battery level");
}

static void on_ble_sync(void) {
    power_mgr_on_ble_sync();
}

static void send_button_event(int index, bool pressed) {
    if (!ble_uart_is_connected()) {
        return;
    }
    char msg[16];
    snprintf(msg, sizeof(msg), "BTN:%d:%c\n", index, pressed ? 'D' : 'U');
    ble_uart_notify((const uint8_t *)msg, strlen(msg));
}

static void poll_buttons_ble(void) {
    for (int i = 0; i < BUTTON_COUNT; ++i) {
        const int level = buttons_stable_level(i);
        if (level != s_last_btn[i]) {
            s_last_btn[i] = level;
            power_mgr_on_button_activity();
            display_wake();
            display_mark_dirty();
            send_button_event(i, level == 0);
        }
    }
}

static void process_battery_events(uint32_t now_ms) {
    if (!s_ble_started || !ble_uart_is_connected()) {
        return;
    }

    const int pct = battery_read_percent();
    const bool usb_present = battery_is_usb_present();
    const bool charge_active = battery_is_charge_active();
    const bool charge_full = usb_present && !charge_active && pct >= 100;

    if (pct >= 0) {
        display_set_battery_percent(pct);
    }

    if (!battery_is_adc_settled()) {
        return;
    }

    note_battery_snapshot(pct, charge_active, charge_full, now_ms);

    if (usb_present != s_last_usb_present) {
        s_last_usb_present = usb_present;
        s_bat_send_asap = true;
        s_bat_retry_after_ms = 0;
        s_pending_since_ms = now_ms;
        if (usb_present) {
            s_bat_ready_after_ms = now_ms + BATTERY_CHARGE_SETTLE_MS;
            s_notify_stable_ms = BAT_CHARGE_FLAG_STABLE_MS;
            return;
        }

        s_bat_ready_after_ms = now_ms + BATTERY_SETTLE_MS;
        s_notify_stable_ms = 0;
        return;
    }

    if (charge_active != s_last_charge_active) {
        s_last_charge_active = charge_active;
        s_bat_send_asap = true;
        s_bat_retry_after_ms = 0;
        s_pending_since_ms = now_ms;
        s_notify_stable_ms = BAT_CHARGE_FLAG_STABLE_MS;
    }

    if (!s_bat_send_asap && now_ms >= s_bat_ready_after_ms) {
        if (pct != s_last_notified_pct ||
            charge_active != s_last_notified_charging ||
            charge_full != s_last_notified_full) {
            s_bat_send_asap = true;
            s_notify_stable_ms = BAT_NOTIFY_STABLE_MS;
            s_pending_since_ms = now_ms;
        }
    }

    if (!s_bat_send_asap) {
        return;
    }

    if (now_ms < s_bat_ready_after_ms) {
        return;
    }

    if (s_bat_retry_after_ms != 0 && now_ms < s_bat_retry_after_ms) {
        return;
    }

    if (!battery_snapshot_is_stable(now_ms, pct, charge_active, charge_full)) {
        return;
    }

    push_battery_telemetry(pct, charge_active, charge_full, true);
    if (s_last_notified_pct < 0) {
        s_bat_retry_after_ms = now_ms + BAT_NOTIFY_RETRY_MS;
    }
}

static void app_task(void *arg) {
    (void)arg;

    for (;;) {
        if (power_mgr_should_wait_for_button()) {
            xEventGroupWaitBits(buttons_event_group(), BUTTON_WAKE_BIT,
                                pdTRUE, pdFALSE, portMAX_DELAY);
            power_mgr_on_button_activity();
            continue;
        }

        const uint32_t now_ms = buttons_millis();

        buttons_update();
        power_mgr_tick(now_ms);

        if (s_ble_started) {
            protocol_update();
            poll_buttons_ble();
            process_battery_events(now_ms);
        }

        vTaskDelay(pdMS_TO_TICKS(power_mgr_loop_delay_ms()));
        power_debug_on_loop_tick(
            now_ms,
            power_mgr_loop_delay_ms(),
            ble_uart_is_connected(),
            power_mgr_state() == POWER_STATE_ADV_OFF);
    }
}

void app_main(void) {
    esp_err_t ret = nvs_flash_init();
    if (ret == ESP_ERR_NVS_NO_FREE_PAGES || ret == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_ERROR_CHECK(nvs_flash_erase());
        ESP_ERROR_CHECK(nvs_flash_init());
    }

    power_init();
    power_mgr_init();
    power_debug_init();

    ESP_LOGI(TAG, "OASIS headless — USB settle %d ms", USB_SETTLE_MS);
    vTaskDelay(pdMS_TO_TICKS(USB_SETTLE_MS));

    protocol_init();
    buttons_init();
    battery_init();
    ble_uart_set_notify_subscribed_cb(on_ble_notify_subscribed);
    ble_uart_set_connection_cb(on_ble_connection_changed);
    ble_uart_set_sync_cb(on_ble_sync);

    ble_uart_start();
    s_ble_started = true;
    ESP_LOGI(TAG, "BLE starting");

    xTaskCreate(app_task, "app", 4096, NULL, 4, NULL);

    ESP_LOGI(TAG, "OASIS ESP-IDF running (power-managed headless)");
}
