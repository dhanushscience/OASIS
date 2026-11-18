#pragma once

#include <stdbool.h>
#include <stdint.h>

bool display_init(void);
void display_wake(void);
void display_mark_dirty(void);
void display_set_ble_connected(bool connected);
void display_set_battery_percent(int percent);
void display_set_title(const char *title);
void display_set_button_label(int index, const char *label);
void display_clear_screen_state(void);
void display_set_held_button(int index);
void display_set_ai_thinking(bool thinking);
bool display_is_animating(void);
uint32_t display_get_ui_delay_ms(void);
void display_refresh(void);
