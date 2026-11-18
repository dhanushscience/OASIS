#include "ui_draw.h"

static void set_pixel(uint8_t *fb, int fb_w, int fb_h, int x, int y, bool on) {
    if (x < 0 || y < 0 || x >= fb_w || y >= fb_h) {
        return;
    }
    const int idx = x + (y / 8) * fb_w;
    const uint8_t mask = (uint8_t)(1u << (y & 7));
    if (on) {
        fb[idx] |= mask;
    } else {
        fb[idx] &= (uint8_t)~mask;
    }
}

void ui_draw_pixel(uint8_t *fb, int fb_w, int fb_h, int x, int y, bool on) {
    set_pixel(fb, fb_w, fb_h, x, y, on);
}

static void hline(uint8_t *fb, int fb_w, int fb_h, int x0, int x1, int y) {
    for (int x = x0; x <= x1; ++x) {
        set_pixel(fb, fb_w, fb_h, x, y, true);
    }
}

static int round_inset(int row, int h, int r) {
    if (r <= 0) {
        return 0;
    }
    const int top = row;
    const int bot = h - 1 - row;
    const int inset_top = (top < r) ? (r - top) : 0;
    const int inset_bot = (bot < r) ? (r - bot) : 0;
    return inset_top > inset_bot ? inset_top : inset_bot;
}

void ui_fill_rect(uint8_t *fb, int fb_w, int fb_h, int x, int y, int w, int h) {
    if (w <= 0 || h <= 0) {
        return;
    }
    for (int row = 0; row < h; ++row) {
        hline(fb, fb_w, fb_h, x, x + w - 1, y + row);
    }
}

void ui_draw_rect(uint8_t *fb, int fb_w, int fb_h, int x, int y, int w, int h) {
    if (w <= 0 || h <= 0) {
        return;
    }
    hline(fb, fb_w, fb_h, x, x + w - 1, y);
    hline(fb, fb_w, fb_h, x, x + w - 1, y + h - 1);
    for (int row = 0; row < h; ++row) {
        set_pixel(fb, fb_w, fb_h, x, y + row, true);
        set_pixel(fb, fb_w, fb_h, x + w - 1, y + row, true);
    }
}

void ui_fill_round_rect(uint8_t *fb, int fb_w, int fb_h, int x, int y, int w, int h, int r) {
    if (w <= 0 || h <= 0) {
        return;
    }
    if (r > w / 2) {
        r = w / 2;
    }
    if (r > h / 2) {
        r = h / 2;
    }
    for (int row = 0; row < h; ++row) {
        const int inset = round_inset(row, h, r);
        hline(fb, fb_w, fb_h, x + inset, x + w - 1 - inset, y + row);
    }
}

static void clear_hline(uint8_t *fb, int fb_w, int fb_h, int x0, int x1, int y) {
    for (int x = x0; x <= x1; ++x) {
        set_pixel(fb, fb_w, fb_h, x, y, false);
    }
}

static void clear_round_rect(uint8_t *fb, int fb_w, int fb_h, int x, int y, int w, int h, int r) {
    if (w <= 0 || h <= 0) {
        return;
    }
    if (r > w / 2) {
        r = w / 2;
    }
    if (r > h / 2) {
        r = h / 2;
    }
    for (int row = 0; row < h; ++row) {
        const int inset = round_inset(row, h, r);
        clear_hline(fb, fb_w, fb_h, x + inset, x + w - 1 - inset, y + row);
    }
}

void ui_draw_round_rect(uint8_t *fb, int fb_w, int fb_h, int x, int y, int w, int h, int r) {
    ui_fill_round_rect(fb, fb_w, fb_h, x, y, w, h, r);
    if (w > 2 && h > 2) {
        clear_round_rect(fb, fb_w, fb_h, x + 1, y + 1, w - 2, h - 2, r > 0 ? r - 1 : 0);
    }
}
