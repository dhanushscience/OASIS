#include "display.h"

bool display_init(void) { return false; }
void display_wake(void) {}
void display_mark_dirty(void) {}
void display_set_ble_connected(bool connected) { (void)connected; }
void display_set_battery_percent(int percent) { (void)percent; }
void display_set_title(const char *title) { (void)title; }
void display_set_button_label(int index, const char *label) {
    (void)index;
    (void)label;
}
void display_clear_screen_state(void) {}
void display_set_held_button(int index) { (void)index; }
void display_set_ai_thinking(bool thinking) { (void)thinking; }
bool display_is_animating(void) { return false; }
uint32_t display_get_ui_delay_ms(void) { return 1000; }
void display_refresh(void) {}
