#include "power_mgr.h"
#include "ble_uart.h"
#include "buttons.h"
#include "power_debug.h"

#include "esp_log.h"
#include "esp_sleep.h"

#include "oasis_pins.h"

static const char *TAG = "power";

static power_state_t s_state = POWER_STATE_ADV_FAST;
static uint32_t s_state_enter_ms = 0;
static uint32_t s_last_button_ms = 0;
static bool s_gpio_wake_armed = false;

static void enter_state(power_state_t state, uint32_t now_ms) {
    if (s_state == state) {
        return;
    }

    ESP_LOGI(TAG, "State %d -> %d", (int)s_state, (int)state);
    power_debug_on_state_change(s_state, state, now_ms);
    s_state = state;
    s_state_enter_ms = now_ms;

    switch (state) {
        case POWER_STATE_ADV_OFF:
            ble_uart_stop_advertising();
            buttons_enable_gpio_wake(true);
            s_gpio_wake_armed = true;
            break;
        case POWER_STATE_ADV_FAST:
            buttons_enable_gpio_wake(false);
            s_gpio_wake_armed = false;
            ble_uart_start_advertising(BLE_ADV_FAST);
            break;
        case POWER_STATE_ADV_SLOW:
            buttons_enable_gpio_wake(false);
            s_gpio_wake_armed = false;
            ble_uart_start_advertising(BLE_ADV_SLOW);
            break;
        case POWER_STATE_CONNECTED_ACTIVE:
        case POWER_STATE_CONNECTED_IDLE:
            buttons_enable_gpio_wake(false);
            s_gpio_wake_armed = false;
            ble_uart_stop_advertising();
            break;
        default:
            break;
    }
}

static void promote_connected_active(uint32_t now_ms) {
    s_last_button_ms = now_ms;
    if (s_state != POWER_STATE_CONNECTED_ACTIVE) {
        enter_state(POWER_STATE_CONNECTED_ACTIVE, now_ms);
    }
    ble_uart_request_conn_profile(BLE_CONN_PROFILE_ACTIVE);
}

void power_mgr_init(void) {
    s_state = POWER_STATE_ADV_FAST;
    s_state_enter_ms = 0;
    s_last_button_ms = 0;
    s_gpio_wake_armed = false;
}

void power_mgr_on_ble_sync(void) {
    const uint32_t now_ms = buttons_millis();
    if (s_state != POWER_STATE_ADV_FAST) {
        enter_state(POWER_STATE_ADV_FAST, now_ms);
        return;
    }

    // Cold boot: init already leaves us in ADV_FAST, so enter_state() is a
    // no-op and would skip ble_uart_start_advertising() until ADV_SLOW (~120s).
    s_state_enter_ms = now_ms;
    if (!ble_uart_is_connected()) {
        ble_uart_start_advertising(BLE_ADV_FAST);
    }
}

void power_mgr_on_connect(void) {
    const uint32_t now_ms = buttons_millis();
    promote_connected_active(now_ms);
}

void power_mgr_on_disconnect(void) {
    const uint32_t now_ms = buttons_millis();
    enter_state(POWER_STATE_ADV_FAST, now_ms);
}

void power_mgr_on_button_activity(void) {
    const uint32_t now_ms = buttons_millis();

    if (ble_uart_is_connected()) {
        promote_connected_active(now_ms);
        return;
    }

    if (s_state == POWER_STATE_ADV_OFF) {
        enter_state(POWER_STATE_ADV_FAST, now_ms);
    } else if (s_state == POWER_STATE_ADV_SLOW) {
        enter_state(POWER_STATE_ADV_FAST, now_ms);
    }
}

void power_mgr_on_conn_profile_applied(ble_conn_profile_t profile) {
    (void)profile;
}

void power_mgr_tick(uint32_t now_ms) {
    if (ble_uart_is_connected()) {
        if (s_state == POWER_STATE_CONNECTED_ACTIVE &&
            now_ms - s_last_button_ms >= CONN_IDLE_AFTER_MS) {
            enter_state(POWER_STATE_CONNECTED_IDLE, now_ms);
            ble_uart_request_conn_profile(BLE_CONN_PROFILE_IDLE);
        }
        return;
    }

    switch (s_state) {
        case POWER_STATE_ADV_FAST:
            if (now_ms - s_state_enter_ms >= ADV_FAST_DURATION_MS) {
                enter_state(POWER_STATE_ADV_SLOW, now_ms);
            }
            break;
        case POWER_STATE_ADV_SLOW:
            if (now_ms - s_state_enter_ms >= ADV_OFF_AFTER_MS) {
                enter_state(POWER_STATE_ADV_OFF, now_ms);
            }
            break;
        default:
            break;
    }
}

power_state_t power_mgr_state(void) {
    return s_state;
}

bool power_mgr_should_wait_for_button(void) {
    return s_state == POWER_STATE_ADV_OFF && !ble_uart_is_connected();
}

uint32_t power_mgr_loop_delay_ms(void) {
    if (buttons_held_index() >= 0) {
        return APP_LOOP_ACTIVE_MS;
    }
    if (ble_uart_is_connected()) {
        if (s_state == POWER_STATE_CONNECTED_IDLE) {
            return APP_LOOP_IDLE_MS;
        }
        return APP_LOOP_ACTIVE_MS;
    }
    if (s_state == POWER_STATE_ADV_FAST) {
        return APP_LOOP_ACTIVE_MS;
    }
    return APP_LOOP_IDLE_MS;
}
