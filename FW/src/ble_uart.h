#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

typedef void (*ble_uart_notify_subscribed_cb_t)(void);
typedef void (*ble_uart_connection_cb_t)(bool connected);
typedef void (*ble_uart_sync_cb_t)(void);

typedef enum {
    BLE_ADV_FAST = 0,
    BLE_ADV_SLOW,
} ble_adv_mode_t;

typedef enum {
    BLE_CONN_PROFILE_ACTIVE = 0,
    BLE_CONN_PROFILE_IDLE,
} ble_conn_profile_t;

void ble_uart_start(void);
bool ble_uart_is_connected(void);
bool ble_uart_is_advertising(void);
void ble_uart_set_notify_subscribed_cb(ble_uart_notify_subscribed_cb_t cb);
void ble_uart_set_connection_cb(ble_uart_connection_cb_t cb);
void ble_uart_set_sync_cb(ble_uart_sync_cb_t cb);
int ble_uart_start_advertising(ble_adv_mode_t mode);
int ble_uart_stop_advertising(void);
int ble_uart_request_conn_profile(ble_conn_profile_t profile);
int ble_uart_notify(const uint8_t *data, size_t len);
