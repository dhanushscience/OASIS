#pragma once

#include "power_mgr.h"

#include <stdint.h>

void power_debug_init(void);
void power_debug_on_state_change(power_state_t from, power_state_t to, uint32_t now_ms);
void power_debug_on_loop_tick(uint32_t now_ms, uint32_t loop_delay_ms, bool connected, bool adv_off);
