#pragma once

#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"

#define BUTTON_WAKE_BIT  (1 << 0)

void buttons_init(void);
void buttons_update(void);
int buttons_held_index(void);
int buttons_stable_level(int index);
uint32_t buttons_millis(void);
EventGroupHandle_t buttons_event_group(void);
void buttons_enable_gpio_wake(bool enable);
