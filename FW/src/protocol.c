#include "protocol.h"
#include "display.h"
#include "ble_uart.h"

#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"

#include <ctype.h>
#include <stdio.h>
#include <string.h>

#define RX_BUF_SIZE 512

static char s_rx[RX_BUF_SIZE];
static size_t s_rx_len = 0;
static SemaphoreHandle_t s_mutex;

static void trim_inplace(char *s) {
    if (!s) {
        return;
    }
    size_t len = strlen(s);
    while (len > 0 && isspace((unsigned char)s[len - 1])) {
        s[--len] = '\0';
    }
    size_t start = 0;
    while (s[start] && isspace((unsigned char)s[start])) {
        ++start;
    }
    if (start > 0) {
        memmove(s, s + start, strlen(s + start) + 1);
    }
}

static int str_ieq(const char *a, const char *b) {
    while (*a && *b) {
        if (tolower((unsigned char)*a) != tolower((unsigned char)*b)) {
            return 0;
        }
        ++a;
        ++b;
    }
    return *a == *b;
}

static void handle_line(char *line) {
    trim_inplace(line);
    if (line[0] == '\0') {
        return;
    }
    if (str_ieq(line, "SCR:CLR")) {
        display_clear_screen_state();
        return;
    }
    if (strncmp(line, "SCR:TITLE:", 10) == 0) {
        display_set_title(line + 10);
        return;
    }
    if (strncmp(line, "SCR:B0:", 7) == 0) {
        display_set_button_label(0, line + 7);
        return;
    }
    if (strncmp(line, "SCR:B1:", 7) == 0) {
        display_set_button_label(1, line + 7);
        return;
    }
    if (strncmp(line, "SCR:B2:", 7) == 0) {
        display_set_button_label(2, line + 7);
        return;
    }
    if (str_ieq(line, "SCR:THINK:1") || str_ieq(line, "SCR:THINK:ON")) {
        display_set_ai_thinking(true);
        return;
    }
    if (str_ieq(line, "SCR:THINK:0") || str_ieq(line, "SCR:THINK:OFF")) {
        display_set_ai_thinking(false);
        return;
    }
    if (str_ieq(line, "PING")) {
        const int64_t t0 = esp_timer_get_time();
        char msg[24];
        const int lat_ms = (int)((esp_timer_get_time() - t0) / 1000);
        snprintf(msg, sizeof(msg), "LAT:%d\n", lat_ms);
        ble_uart_notify((const uint8_t *)msg, strlen(msg));
        return;
    }
}

void protocol_init(void) {
    if (s_mutex == NULL) {
        s_mutex = xSemaphoreCreateMutex();
    }
}

void protocol_feed_rx(const uint8_t *data, size_t len) {
    if (!data || len == 0 || s_mutex == NULL) {
        return;
    }
    display_wake();
    display_mark_dirty();
    if (xSemaphoreTake(s_mutex, pdMS_TO_TICKS(20)) != pdTRUE) {
        return;
    }
    if (s_rx_len + len >= RX_BUF_SIZE) {
        s_rx_len = 0;
    }
    memcpy(s_rx + s_rx_len, data, len);
    s_rx_len += len;
    s_rx[s_rx_len] = '\0';
    xSemaphoreGive(s_mutex);
}

void protocol_update(void) {
    if (s_mutex == NULL) {
        return;
    }
    if (xSemaphoreTake(s_mutex, pdMS_TO_TICKS(20)) != pdTRUE) {
        return;
    }
    char *start = s_rx;
    for (;;) {
        char *nl = strchr(start, '\n');
        if (!nl) {
            break;
        }
        *nl = '\0';
        handle_line(start);
        start = nl + 1;
    }
    const size_t remain = strlen(start);
    memmove(s_rx, start, remain + 1);
    s_rx_len = remain;
    xSemaphoreGive(s_mutex);
}
