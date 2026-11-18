#pragma once

#include "ble_uart.h"

#include <stdbool.h>
#include <stdint.h>

typedef enum {
    POWER_STATE_ADV_OFF = 0,
    POWER_STATE_ADV_FAST,
    POWER_STATE_ADV_SLOW,
    POWER_STATE_CONNECTED_ACTIVE,
    POWER_STATE_CONNECTED_IDLE,
} power_state_t;

void power_mgr_init(void);
void power_mgr_tick(uint32_t now_ms);
power_state_t power_mgr_state(void);
bool power_mgr_should_wait_for_button(void);
uint32_t power_mgr_loop_delay_ms(void);
void power_mgr_on_ble_sync(void);
void power_mgr_on_connect(void);
void power_mgr_on_disconnect(void);
void power_mgr_on_button_activity(void);
void power_mgr_on_conn_profile_applied(ble_conn_profile_t profile);
