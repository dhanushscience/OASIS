#include "power_debug.h"

#include "ble_uart.h"
#include "oasis_config.h"

#include "esp_log.h"

#include <stdio.h>
#include <string.h>

static const char *TAG = "power_dbg";

static uint32_t s_last_heartbeat_ms = 0;

static const char *state_name(power_state_t state) {
    switch (state) {
        case POWER_STATE_ADV_OFF: return "ADV_OFF";
        case POWER_STATE_ADV_FAST: return "ADV_FAST";
        case POWER_STATE_ADV_SLOW: return "ADV_SLOW";
        case POWER_STATE_CONNECTED_ACTIVE: return "CONN_ACTIVE";
        case POWER_STATE_CONNECTED_IDLE: return "CONN_IDLE";
        default: return "UNKNOWN";
    }
}

void power_debug_init(void) {
    s_last_heartbeat_ms = 0;
    ESP_LOGI(TAG, "power debug session 998fe9 active");
}

void power_debug_on_state_change(power_state_t from, power_state_t to, uint32_t now_ms) {
    ESP_LOGI(TAG, "STATE %s -> %s @ %u ms", state_name(from), state_name(to), (unsigned)now_ms);

#if OASIS_POWER_DEBUG_BLE_TELEMETRY
    if (!ble_uart_is_connected()) {
        return;
    }

    char msg[32];
    snprintf(msg, sizeof(msg), "PWR:%s:%u\n", state_name(to), (unsigned)now_ms);
    ble_uart_notify((const uint8_t *)msg, strlen(msg));
#endif
}

void power_debug_on_loop_tick(uint32_t now_ms, uint32_t loop_delay_ms, bool connected, bool adv_off) {
    if (now_ms - s_last_heartbeat_ms < 60000U) {
        return;
    }

    s_last_heartbeat_ms = now_ms;
    ESP_LOGI(TAG, "HEARTBEAT ms=%u loop_delay=%u connected=%d adv_off=%d",
             (unsigned)now_ms, (unsigned)loop_delay_ms, connected ? 1 : 0, adv_off ? 1 : 0);

#if OASIS_POWER_DEBUG_BLE_TELEMETRY
    if (!ble_uart_is_connected()) {
        return;
    }

    char msg[48];
    snprintf(msg, sizeof(msg), "PWR:HB:%u:%u:%d:%d\n",
             (unsigned)now_ms, (unsigned)loop_delay_ms, connected ? 1 : 0, adv_off ? 1 : 0);
    ble_uart_notify((const uint8_t *)msg, strlen(msg));
#endif
}
