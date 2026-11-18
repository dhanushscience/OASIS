#pragma once

#include <stddef.h>
#include <stdint.h>

// Org_v01 by Orgdot — ported from Adafruit Org_01.h
#define FONT_ORG01_FIRST 0x20
#define FONT_ORG01_LAST  0x7E
#define FONT_ORG01_YADV  7

void font_org01_draw_str(uint8_t *fb, int fb_w, int fb_h, int x, int y, const char *s);
int font_org01_string_width(const char *s);
int font_org01_char_height(void);
int font_org01_glyph_advance(char c);
void font_org01_fit_string(const char *s, int max_w, char *out, size_t out_len);
