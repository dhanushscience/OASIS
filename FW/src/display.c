#include "display.h"
#include "font_org01.h"
#include "logo_oasis.h"
#include "oasis_pins.h"
#include "ui_draw.h"

#include "driver/i2c.h"
#include "esp_log.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#include <stdio.h>
#include <string.h>

static const char *TAG = "display";

#define I2C_PORT I2C_NUM_0
#define I2C_HZ   100000
#define FB_SIZE  (OLED_WIDTH * OLED_HEIGHT / 8)

#define TITLE_BASELINE_Y  10
#define BAT_BODY_W       48
#define BAT_BODY_H        5
#define BAT_Y            20
#define THINK_CYCLE_MS   280
#define OLED_IDLE_MS    30000

static char s_title[64] = "OASIS";
static uint8_t s_fb[FB_SIZE];
static uint8_t s_prev_fb[FB_SIZE];
static bool s_ready = false;
static bool s_oled_on = true;
static bool s_dirty = true;
static bool s_ble_connected = false;
static int s_battery_percent = -1;
static bool s_ai_thinking = false;
static bool s_think_pulse = false;
static uint32_t s_think_last_ms = 0;
static uint32_t s_last_change_ms = 0;

static esp_err_t i2c_write_cmd(const uint8_t *data, size_t len) {
    i2c_cmd_handle_t cmd = i2c_cmd_link_create();
    i2c_master_start(cmd);
    i2c_master_write_byte(cmd, (OLED_I2C_ADDR << 1) | I2C_MASTER_WRITE, true);
    i2c_master_write_byte(cmd, 0x00, true);
    i2c_master_write(cmd, (uint8_t *)data, len, true);
    i2c_master_stop(cmd);
    const esp_err_t err = i2c_master_cmd_begin(I2C_PORT, cmd, pdMS_TO_TICKS(100));
    i2c_cmd_link_delete(cmd);
    return err;
}

static esp_err_t i2c_write_data(const uint8_t *data, size_t len) {
    i2c_cmd_handle_t cmd = i2c_cmd_link_create();
    i2c_master_start(cmd);
    i2c_master_write_byte(cmd, (OLED_I2C_ADDR << 1) | I2C_MASTER_WRITE, true);
    i2c_master_write_byte(cmd, 0x40, true);
    i2c_master_write(cmd, (uint8_t *)data, len, true);
    i2c_master_stop(cmd);
    const esp_err_t err = i2c_master_cmd_begin(I2C_PORT, cmd, pdMS_TO_TICKS(100));
    i2c_cmd_link_delete(cmd);
    return err;
}

static void oled_cmd1(uint8_t c) {
    i2c_write_cmd(&c, 1);
}

static void oled_cmd2(uint8_t c, uint8_t a) {
    const uint8_t buf[2] = {c, a};
    i2c_write_cmd(buf, 2);
}

static bool probe_oled(void) {
    i2c_cmd_handle_t cmd = i2c_cmd_link_create();
    i2c_master_start(cmd);
    i2c_master_write_byte(cmd, (OLED_I2C_ADDR << 1) | I2C_MASTER_WRITE, true);
    i2c_master_stop(cmd);
    const esp_err_t err = i2c_master_cmd_begin(I2C_PORT, cmd, pdMS_TO_TICKS(50));
    i2c_cmd_link_delete(cmd);
    ESP_LOGI(TAG, "I2C probe 0x%02X -> %s", OLED_I2C_ADDR, esp_err_to_name(err));
    return err == ESP_OK;
}

static bool i2c_bus_init(void) {
    i2c_config_t cfg = {
        .mode = I2C_MODE_MASTER,
        .sda_io_num = PIN_I2C_SDA,
        .scl_io_num = PIN_I2C_SCL,
        .sda_pullup_en = GPIO_PULLUP_ENABLE,
        .scl_pullup_en = GPIO_PULLUP_ENABLE,
        .master.clk_speed = I2C_HZ,
    };
    ESP_ERROR_CHECK(i2c_param_config(I2C_PORT, &cfg));
    esp_err_t err = i2c_driver_install(I2C_PORT, I2C_MODE_MASTER, 0, 0, 0);
    if (err == ESP_ERR_INVALID_STATE) {
        err = ESP_OK;
    }
    return err == ESP_OK;
}

static void oled_init_chip(void) {
    oled_cmd1(0xAE);
    oled_cmd2(0xD5, 0x80);
    oled_cmd2(0xA8, OLED_MUX_RATIO);
    oled_cmd2(0xD3, OLED_DISPLAY_OFFSET);
    oled_cmd1(0x40);
    oled_cmd2(0x8D, 0x14);
    oled_cmd2(0x20, 0x00);
    oled_cmd1(0xA1);
    oled_cmd1(0xC8);
    oled_cmd2(0xDA, OLED_COM_PINS);
    oled_cmd2(0x81, 0xCF);
    oled_cmd2(0xD9, 0x22);
    oled_cmd2(0xDB, 0x00);
    oled_cmd1(0x2E);
    oled_cmd1(0xA4);
    oled_cmd1(0xA6);
    oled_cmd1(0xAF);
    s_oled_on = true;
    ESP_LOGI(TAG, "SSD1315 init mux=0x%02X com=0x%02X col=%d off=%d",
             OLED_MUX_RATIO, OLED_COM_PINS, OLED_COL_OFFSET, OLED_DISPLAY_OFFSET);
}

static void fb_clear(void) {
    memset(s_fb, 0, sizeof(s_fb));
}

static void truncate_copy(char *dst, size_t dst_len, const char *src) {
    if (dst_len == 0) {
        return;
    }
    strncpy(dst, src ? src : "", dst_len - 1);
    dst[dst_len - 1] = '\0';
}

static void touch_activity(void) {
    s_last_change_ms = (uint32_t)(xTaskGetTickCount() * portTICK_PERIOD_MS);
    if (!s_oled_on) {
        display_wake();
    }
}

void display_mark_dirty(void) {
    s_dirty = true;
    touch_activity();
}

static void update_think_animation(void) {
    if (!s_ai_thinking) {
        s_think_pulse = false;
        return;
    }

    const uint32_t now_ms = (uint32_t)(xTaskGetTickCount() * portTICK_PERIOD_MS);
    if (now_ms - s_think_last_ms >= THINK_CYCLE_MS) {
        s_think_last_ms = now_ms;
        s_think_pulse = !s_think_pulse;
        s_dirty = true;
    }
}

static void draw_title(void) {
    char fit[32];
    font_org01_fit_string(s_title, OLED_WIDTH, fit, sizeof(fit));
    const int tw = font_org01_string_width(fit);
    const int tx = (OLED_WIDTH - tw) / 2;
    font_org01_draw_str(s_fb, OLED_WIDTH, OLED_HEIGHT, tx, TITLE_BASELINE_Y, fit);
}

static int battery_fill_width(int pct) {
    if (s_battery_percent < 0) {
        return 0;
    }

    if (pct < 0) {
        pct = 0;
    }
    if (pct > 100) {
        pct = 100;
    }

    int fill_w = (pct * BAT_BODY_W + 99) / 100;
    if (fill_w <= 0) {
        fill_w = 1;
    }
    if (fill_w > BAT_BODY_W) {
        fill_w = BAT_BODY_W;
    }

    if (s_ai_thinking) {
        fill_w += s_think_pulse ? 2 : -2;
        if (fill_w < 1) {
            fill_w = 1;
        }
        if (fill_w > BAT_BODY_W) {
            fill_w = BAT_BODY_W;
        }
    }

    return fill_w;
}

static void draw_battery_bar(void) {
    const int bx = (OLED_WIDTH - BAT_BODY_W) / 2;
    const int by = BAT_Y;

    ui_draw_rect(s_fb, OLED_WIDTH, OLED_HEIGHT, bx, by, BAT_BODY_W, BAT_BODY_H);

    const int fill_w = battery_fill_width(s_battery_percent);
    if (fill_w > 0) {
        ui_fill_rect(s_fb, OLED_WIDTH, OLED_HEIGHT, bx, by, fill_w, BAT_BODY_H);
    }
}

static void draw_brand(void) {
    fb_clear();
    const int lx = (OLED_WIDTH - OASIS_LOGO_W) / 2;
    const int ly = (OLED_HEIGHT - OASIS_LOGO_H) / 2;
    logo_oasis_draw(s_fb, OLED_WIDTH, OLED_HEIGHT, lx, ly);
}

static void draw_connected(void) {
    fb_clear();
    draw_title();
    draw_battery_bar();
}

static void oled_flush(void) {
    if (!s_ready || !s_oled_on) {
        return;
    }
    const uint8_t col = OLED_COL_OFFSET;
    const int pages = OLED_HEIGHT / 8;
    for (int page = 0; page < pages; ++page) {
        oled_cmd1(0x10 | ((col >> 4) & 0x0F));
        oled_cmd1(0x00 | (col & 0x0F));
        oled_cmd1(0xB0 | (uint8_t)page);
        i2c_write_data(&s_fb[page * OLED_WIDTH], OLED_WIDTH);
    }
    memcpy(s_prev_fb, s_fb, sizeof(s_fb));
}

static void maybe_sleep_oled(void) {
    if (!s_ready || !s_oled_on || !s_ble_connected || s_ai_thinking) {
        return;
    }

    const uint32_t now_ms = (uint32_t)(xTaskGetTickCount() * portTICK_PERIOD_MS);
    if (now_ms - s_last_change_ms >= OLED_IDLE_MS) {
        oled_cmd1(0xAE);
        s_oled_on = false;
        ESP_LOGD(TAG, "OLED sleep");
    }
}

bool display_init(void) {
    if (!i2c_bus_init()) {
        ESP_LOGE(TAG, "I2C init failed");
        return false;
    }

    vTaskDelay(pdMS_TO_TICKS(50));
    if (!probe_oled()) {
        ESP_LOGE(TAG, "OLED not found on I2C");
        return false;
    }

    oled_init_chip();
    s_ready = true;
    s_last_change_ms = (uint32_t)(xTaskGetTickCount() * portTICK_PERIOD_MS);
    ESP_LOGI(TAG, "SSD1315 64x32 ready");
    return true;
}

void display_wake(void) {
    if (s_ready && !s_oled_on) {
        oled_cmd1(0xAF);
        s_oled_on = true;
        s_dirty = true;
    }
}

void display_set_ble_connected(bool connected) {
    if (s_ble_connected != connected) {
        s_ble_connected = connected;
        if (!connected) {
            s_ai_thinking = false;
            s_think_pulse = false;
        }
        display_mark_dirty();
    }
}

void display_set_battery_percent(int percent) {
    if (s_battery_percent != percent) {
        s_battery_percent = percent;
        display_mark_dirty();
    }
}

void display_set_title(const char *title) {
    char next[sizeof(s_title)];
    truncate_copy(next, sizeof(next), title);
    if (strcmp(next, s_title) != 0) {
        strcpy(s_title, next);
        display_mark_dirty();
    }
}

void display_set_button_label(int index, const char *label) {
    (void)index;
    (void)label;
}

void display_clear_screen_state(void) {
    strcpy(s_title, "OASIS");
    s_ai_thinking = false;
    s_think_pulse = false;
    display_mark_dirty();
}

void display_set_held_button(int index) {
    (void)index;
}

void display_set_ai_thinking(bool thinking) {
    if (s_ai_thinking != thinking) {
        s_ai_thinking = thinking;
        if (thinking) {
            s_think_last_ms = (uint32_t)(xTaskGetTickCount() * portTICK_PERIOD_MS);
        } else {
            s_think_pulse = false;
        }
        display_mark_dirty();
    }
}

bool display_is_animating(void) {
    return s_ai_thinking;
}

uint32_t display_get_ui_delay_ms(void) {
    if (display_is_animating()) {
        return THINK_CYCLE_MS;
    }
    if (s_dirty) {
        return 50;
    }
    return 500;
}

void display_refresh(void) {
    if (!s_ready) {
        return;
    }

    update_think_animation();

    if (!s_dirty && !display_is_animating()) {
        maybe_sleep_oled();
        return;
    }

    if (!s_ble_connected) {
        draw_brand();
    } else {
        draw_connected();
    }

    if (memcmp(s_fb, s_prev_fb, FB_SIZE) != 0 || s_dirty) {
        oled_flush();
        s_dirty = false;
        touch_activity();
    } else {
        s_dirty = false;
    }

    maybe_sleep_oled();
}
