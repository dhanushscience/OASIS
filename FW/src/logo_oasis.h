#pragma once

#include <stdint.h>

// OASIS wordmark — 54x18 px, MSB-first rows (Lopaka Vector2 + paint layers, composited)
#define OASIS_LOGO_W         54
#define OASIS_LOGO_H         18
#define OASIS_LOGO_ROW_BYTES   7

void logo_oasis_draw(uint8_t *fb, int fb_w, int fb_h, int x, int y);
