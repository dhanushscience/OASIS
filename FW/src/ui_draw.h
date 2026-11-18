#pragma once

#include <stdbool.h>
#include <stdint.h>

void ui_draw_pixel(uint8_t *fb, int fb_w, int fb_h, int x, int y, bool on);
void ui_fill_rect(uint8_t *fb, int fb_w, int fb_h, int x, int y, int w, int h);
void ui_draw_rect(uint8_t *fb, int fb_w, int fb_h, int x, int y, int w, int h);
void ui_fill_round_rect(uint8_t *fb, int fb_w, int fb_h, int x, int y, int w, int h, int r);
void ui_draw_round_rect(uint8_t *fb, int fb_w, int fb_h, int x, int y, int w, int h, int r);
