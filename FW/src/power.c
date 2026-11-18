#include "power.h"

#include "esp_log.h"

#if __has_include("esp_pm.h")
#include "esp_pm.h"
#define OASIS_HAS_ESP_PM 1
#else
#define OASIS_HAS_ESP_PM 0
#endif

static const char *TAG = "power";

void power_init(void) {
#if OASIS_HAS_ESP_PM && CONFIG_PM_ENABLE
    esp_pm_config_t cfg = {
        .max_freq_mhz = CONFIG_ESP_DEFAULT_CPU_FREQ_MHZ,
        .min_freq_mhz = 40,
        .light_sleep_enable = true,
    };
    const esp_err_t err = esp_pm_configure(&cfg);
    if (err == ESP_OK) {
        ESP_LOGI(TAG, "Light sleep enabled (40-%d MHz)", cfg.max_freq_mhz);
        return;
    }
    ESP_LOGW(TAG, "PM configure failed: %s", esp_err_to_name(err));
#else
    ESP_LOGI(TAG, "CPU-only power profile (no light sleep)");
#endif
}
