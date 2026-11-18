#include "buttons.h"
#include "oasis_pins.h"

#include "driver/gpio.h"
#include "esp_err.h"
#include "esp_sleep.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "freertos/event_groups.h"

static const gpio_num_t kPins[BUTTON_COUNT] = {
    PIN_BTN_0, PIN_BTN_1, PIN_BTN_2,
};

static int s_stable[BUTTON_COUNT] = {1, 1, 1};
static int s_last[BUTTON_COUNT] = {1, 1, 1};
static TickType_t s_debounce[BUTTON_COUNT] = {0};
static EventGroupHandle_t s_wake_events;
static bool s_gpio_wake_enabled = false;

static void IRAM_ATTR button_isr(void *arg) {
    (void)arg;
    if (s_wake_events) {
        BaseType_t hp = pdFALSE;
        xEventGroupSetBitsFromISR(s_wake_events, BUTTON_WAKE_BIT, &hp);
        if (hp) {
            portYIELD_FROM_ISR();
        }
    }
}

static void configure_button_gpio(int index, gpio_int_type_t intr_type) {
    gpio_config_t cfg = {
        .pin_bit_mask = 1ULL << kPins[index],
        .mode = GPIO_MODE_INPUT,
        .pull_up_en = GPIO_PULLUP_ENABLE,
        .pull_down_en = GPIO_PULLDOWN_DISABLE,
        .intr_type = intr_type,
    };
    gpio_config(&cfg);
}

uint32_t buttons_millis(void) {
    return (uint32_t)(xTaskGetTickCount() * portTICK_PERIOD_MS);
}

EventGroupHandle_t buttons_event_group(void) {
    return s_wake_events;
}

void buttons_enable_gpio_wake(bool enable) {
    if (enable == s_gpio_wake_enabled) {
        return;
    }

    s_gpio_wake_enabled = enable;
    if (enable) {
        for (int i = 0; i < BUTTON_COUNT; ++i) {
            configure_button_gpio(i, GPIO_INTR_LOW_LEVEL);
            gpio_wakeup_enable(kPins[i], GPIO_INTR_LOW_LEVEL);
        }
        esp_sleep_enable_gpio_wakeup();
    } else {
        esp_sleep_disable_wakeup_source(ESP_SLEEP_WAKEUP_GPIO);
        for (int i = 0; i < BUTTON_COUNT; ++i) {
            gpio_wakeup_disable(kPins[i]);
            configure_button_gpio(i, GPIO_INTR_NEGEDGE);
        }
    }
}

void buttons_init(void) {
    s_wake_events = xEventGroupCreate();

    const esp_err_t isr_err = gpio_install_isr_service(0);
    if (isr_err != ESP_OK && isr_err != ESP_ERR_INVALID_STATE) {
        return;
    }

    for (int i = 0; i < BUTTON_COUNT; ++i) {
        configure_button_gpio(i, GPIO_INTR_NEGEDGE);
        s_stable[i] = gpio_get_level(kPins[i]);
        s_last[i] = s_stable[i];
        gpio_isr_handler_add(kPins[i], button_isr, NULL);
    }
}

int buttons_held_index(void) {
    for (int i = 0; i < BUTTON_COUNT; ++i) {
        if (s_stable[i] == 0) {
            return i;
        }
    }
    return -1;
}

int buttons_stable_level(int index) {
    if (index < 0 || index >= BUTTON_COUNT) {
        return 1;
    }
    return s_stable[index];
}

void buttons_update(void) {
    const TickType_t now = xTaskGetTickCount();
    for (int i = 0; i < BUTTON_COUNT; ++i) {
        const int reading = gpio_get_level(kPins[i]);
        if (reading != s_last[i]) {
            s_debounce[i] = now;
        }
        s_last[i] = reading;
        if ((now - s_debounce[i]) < pdMS_TO_TICKS(DEBOUNCE_MS)) {
            continue;
        }
        s_stable[i] = reading;
    }
}
