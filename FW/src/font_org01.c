#include "font_org01.h"

#include "ui_draw.h"

#include <ctype.h>
#include <string.h>

// Org_v01 by Orgdot — bitmaps/glyphs from Adafruit Org_01.h
typedef struct {
    uint16_t bitmapOffset;
    uint8_t width;
    uint8_t height;
    uint8_t xAdvance;
    int8_t xOffset;
    int8_t yOffset;
} GFXglyph;

static const uint8_t k_bitmaps[] = {
    0xE8, 0xA0, 0x57, 0xD5, 0xF5, 0x00, 0xFD, 0x3E, 0x5F, 0x80, 0x88, 0x88,
    0x88, 0x80, 0xF4, 0xBF, 0x2E, 0x80, 0x80, 0x6A, 0x40, 0x95, 0x80, 0xAA,
    0x80, 0x5D, 0x00, 0xC0, 0xF0, 0x80, 0x08, 0x88, 0x88, 0x00, 0xFC, 0x63,
    0x1F, 0x80, 0xF8, 0xF8, 0x7F, 0x0F, 0x80, 0xF8, 0x7E, 0x1F, 0x80, 0x8C,
    0x7E, 0x10, 0x80, 0xFC, 0x3E, 0x1F, 0x80, 0xFC, 0x3F, 0x1F, 0x80, 0xF8,
    0x42, 0x10, 0x80, 0xFC, 0x7F, 0x1F, 0x80, 0xFC, 0x7E, 0x1F, 0x80, 0x90,
    0xB0, 0x2A, 0x22, 0xF0, 0xF0, 0x88, 0xA8, 0xF8, 0x4E, 0x02, 0x00, 0xFD,
    0x6F, 0x0F, 0x80, 0xFC, 0x7F, 0x18, 0x80, 0xF4, 0x7D, 0x1F, 0x00, 0xFC,
    0x21, 0x0F, 0x80, 0xF4, 0x63, 0x1F, 0x00, 0xFC, 0x3F, 0x0F, 0x80, 0xFC,
    0x3F, 0x08, 0x00, 0xFC, 0x2F, 0x1F, 0x80, 0x8C, 0x7F, 0x18, 0x80, 0xF9,
    0x08, 0x4F, 0x80, 0x78, 0x85, 0x2F, 0x80, 0x8D, 0xB1, 0x68, 0x80, 0x84,
    0x21, 0x0F, 0x80, 0xFD, 0x6B, 0x5A, 0x80, 0xFC, 0x63, 0x18, 0x80, 0xFC,
    0x63, 0x1F, 0x80, 0xFC, 0x7F, 0x08, 0x00, 0xFC, 0x63, 0x3F, 0x80, 0xFC,
    0x7F, 0x29, 0x00, 0xFC, 0x3E, 0x1F, 0x80, 0xF9, 0x08, 0x42, 0x00, 0x8C,
    0x63, 0x1F, 0x80, 0x8C, 0x62, 0xA2, 0x00, 0xAD, 0x6B, 0x5F, 0x80, 0x8A,
    0x88, 0xA8, 0x80, 0x8C, 0x54, 0x42, 0x00, 0xF8, 0x7F, 0x0F, 0x80, 0xEA,
    0xC0, 0x82, 0x08, 0x20, 0x80, 0xD5, 0xC0, 0x54, 0xF8, 0x80, 0xF1, 0xFF,
    0x8F, 0x99, 0xF0, 0xF8, 0x8F, 0x1F, 0x99, 0xF0, 0xFF, 0x8F, 0x6B, 0xA4,
    0xF9, 0x9F, 0x10, 0x8F, 0x99, 0x90, 0xF0, 0x55, 0xC0, 0x8A, 0xF9, 0x90,
    0xF8, 0xFD, 0x63, 0x10, 0xF9, 0x99, 0xF9, 0x9F, 0xF9, 0x9F, 0x80, 0xF9,
    0x9F, 0x20, 0xF8, 0x88, 0x47, 0x1F, 0x27, 0xC8, 0x42, 0x00, 0x99, 0x9F,
    0x99, 0x97, 0x8C, 0x6B, 0xF0, 0x96, 0x69, 0x99, 0x9F, 0x10, 0x2E, 0x8F,
    0x2B, 0x22, 0xF8, 0x89, 0xA8, 0x0F, 0xE0};

static const GFXglyph k_glyphs[] = {
    {0, 0, 0, 6, 0, 1},     {0, 1, 5, 2, 0, -4},    {1, 3, 1, 4, 0, -4},
    {2, 5, 5, 6, 0, -4},    {6, 5, 5, 6, 0, -4},    {10, 5, 5, 6, 0, -4},
    {14, 5, 5, 6, 0, -4},   {18, 1, 1, 2, 0, -4},   {19, 2, 5, 3, 0, -4},
    {21, 2, 5, 3, 0, -4},   {23, 3, 3, 4, 0, -3},   {25, 3, 3, 4, 0, -3},
    {27, 1, 2, 2, 0, 0},    {28, 4, 1, 5, 0, -2},   {29, 1, 1, 2, 0, 0},
    {30, 5, 5, 6, 0, -4},   {34, 5, 5, 6, 0, -4},   {38, 1, 5, 2, 0, -4},
    {39, 5, 5, 6, 0, -4},   {43, 5, 5, 6, 0, -4},   {47, 5, 5, 6, 0, -4},
    {51, 5, 5, 6, 0, -4},   {55, 5, 5, 6, 0, -4},   {59, 5, 5, 6, 0, -4},
    {63, 5, 5, 6, 0, -4},   {67, 5, 5, 6, 0, -4},   {71, 1, 4, 2, 0, -3},
    {72, 1, 4, 2, 0, -3},   {73, 3, 5, 4, 0, -4},   {75, 4, 3, 5, 0, -3},
    {77, 3, 5, 4, 0, -4},   {79, 5, 5, 6, 0, -4},   {83, 5, 5, 6, 0, -4},
    {87, 5, 5, 6, 0, -4},   {91, 5, 5, 6, 0, -4},   {95, 5, 5, 6, 0, -4},
    {99, 5, 5, 6, 0, -4},   {103, 5, 5, 6, 0, -4},  {107, 5, 5, 6, 0, -4},
    {111, 5, 5, 6, 0, -4},  {115, 5, 5, 6, 0, -4},  {119, 5, 5, 6, 0, -4},
    {123, 5, 5, 6, 0, -4},  {127, 5, 5, 6, 0, -4},  {131, 5, 5, 6, 0, -4},
    {135, 5, 5, 6, 0, -4},  {139, 5, 5, 6, 0, -4},  {143, 5, 5, 6, 0, -4},
    {147, 5, 5, 6, 0, -4},  {151, 5, 5, 6, 0, -4},  {155, 5, 5, 6, 0, -4},
    {159, 5, 5, 6, 0, -4},  {163, 5, 5, 6, 0, -4},  {167, 5, 5, 6, 0, -4},
    {171, 5, 5, 6, 0, -4},  {175, 5, 5, 6, 0, -4},  {179, 5, 5, 6, 0, -4},
    {183, 5, 5, 6, 0, -4},  {187, 5, 5, 6, 0, -4},  {191, 2, 5, 3, 0, -4},
    {193, 5, 5, 6, 0, -4},  {197, 2, 5, 3, 0, -4},  {199, 3, 2, 4, 0, -4},
    {200, 5, 1, 6, 0, 1},   {201, 1, 1, 2, 0, -4},  {202, 4, 4, 5, 0, -3},
    {204, 4, 5, 5, 0, -4},  {207, 4, 4, 5, 0, -3},  {209, 4, 5, 5, 0, -4},
    {212, 4, 4, 5, 0, -3},  {214, 3, 5, 4, 0, -4},  {216, 4, 5, 5, 0, -3},
    {219, 4, 5, 5, 0, -4},  {222, 1, 4, 2, 0, -3},  {223, 2, 5, 3, 0, -3},
    {225, 4, 5, 5, 0, -4},  {228, 1, 5, 2, 0, -4},  {229, 5, 4, 6, 0, -3},
    {232, 4, 4, 5, 0, -3},  {234, 4, 4, 5, 0, -3},  {236, 4, 5, 5, 0, -3},
    {239, 4, 5, 5, 0, -3},  {242, 4, 4, 5, 0, -3},  {244, 4, 4, 5, 0, -3},
    {246, 5, 5, 6, 0, -4},  {250, 4, 4, 5, 0, -3},  {252, 4, 4, 5, 0, -3},
    {254, 5, 4, 6, 0, -3},  {257, 4, 4, 5, 0, -3},  {259, 4, 5, 5, 0, -3},
    {262, 4, 4, 5, 0, -3},  {264, 3, 5, 4, 0, -4},  {266, 1, 5, 2, 0, -4},
    {267, 3, 5, 4, 0, -4},  {269, 5, 3, 6, 0, -3},
};

static const GFXglyph *glyph_for(char c) {
    if ((unsigned char)c < FONT_ORG01_FIRST || (unsigned char)c > FONT_ORG01_LAST) {
        return &k_glyphs[0];
    }
    return &k_glyphs[(unsigned char)c - FONT_ORG01_FIRST];
}

static void draw_glyph(uint8_t *fb, int fb_w, int fb_h, int x, int y, char c) {
    const GFXglyph *glyph = glyph_for(c);
    const uint8_t *bitmap = &k_bitmaps[glyph->bitmapOffset];
    const int gw = glyph->width;
    const int gh = glyph->height;
    const int xo = glyph->xOffset;
    const int yo = glyph->yOffset;

    uint8_t bit = 0;
    uint8_t bits = 0;
    for (int yy = 0; yy < gh; ++yy) {
        for (int xx = 0; xx < gw; ++xx) {
            if (!(bit & 7)) {
                bits = bitmap[bit >> 3];
            }
            if (bits & 0x80) {
                ui_draw_pixel(fb, fb_w, fb_h, x + xo + xx, y + yo + yy, true);
            }
            bit++;
            bits <<= 1;
        }
    }
}

int font_org01_glyph_advance(char c) {
    return glyph_for(c)->xAdvance;
}

int font_org01_char_height(void) {
    return FONT_ORG01_YADV;
}

int font_org01_string_width(const char *s) {
    int w = 0;
    while (s && *s) {
        w += glyph_for(*s)->xAdvance;
        ++s;
    }
    return w;
}

void font_org01_draw_str(uint8_t *fb, int fb_w, int fb_h, int x, int y, const char *s) {
    int cx = x;
    while (s && *s) {
        const char ch = (char)toupper((unsigned char)*s);
        draw_glyph(fb, fb_w, fb_h, cx, y, ch);
        cx += glyph_for(ch)->xAdvance;
        ++s;
    }
}

void font_org01_fit_string(const char *s, int max_w, char *out, size_t out_len) {
    if (!out || out_len == 0) {
        return;
    }
    out[0] = '\0';
    if (!s || max_w <= 0) {
        return;
    }

    size_t n = 0;
    int w = 0;
    for (; s[n] != '\0' && n + 1 < out_len; ++n) {
        const char ch = (char)toupper((unsigned char)s[n]);
        const int adv = glyph_for(ch)->xAdvance;
        if (w + adv > max_w) {
            break;
        }
        out[n] = ch;
        w += adv;
    }
    out[n] = '\0';
}
