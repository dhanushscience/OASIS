idf_component_register(
    SRCS
        "main.c"
        "display_stub.c"
        "buttons.c"
        "battery.c"
        "protocol.c"
        "ble_uart.c"
        "power.c"
        "power_mgr.c"
        "power_debug.c"
    INCLUDE_DIRS "."
    REQUIRES driver nvs_flash bt esp_timer esp_pm esp_hw_support esp_adc
)
