#pragma once

#include <stdint.h>

// 5x7 font, ASCII 32..127
void font5x7_draw_char(uint8_t *fb, int w, int h, int x, int y, char c);
int font5x7_char_width(void);
int font5x7_char_height(void);
int font5x7_string_width(const char *s);
