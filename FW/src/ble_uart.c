#include "ble_uart.h"
#include "protocol.h"
#include "display.h"
#include "oasis_pins.h"

#include "esp_log.h"
#include "nimble/nimble_port.h"
#include "nimble/nimble_port_freertos.h"
#include "host/ble_hs.h"
#include "host/ble_uuid.h"
#include "host/util/util.h"
#include "services/gap/ble_svc_gap.h"
#include "services/gatt/ble_svc_gatt.h"

#include <string.h>

static const char *TAG = "ble";

static uint16_t s_conn_handle = BLE_HS_CONN_HANDLE_NONE;
static uint16_t s_tx_val_handle;
static bool s_started = false;
static bool s_advertising = false;
static bool s_notify_enabled = false;
static ble_adv_mode_t s_adv_mode = BLE_ADV_SLOW;
static ble_conn_profile_t s_conn_profile = BLE_CONN_PROFILE_IDLE;
static ble_conn_profile_t s_pending_conn_profile = BLE_CONN_PROFILE_IDLE;
static ble_uart_notify_subscribed_cb_t s_notify_subscribed_cb;
static ble_uart_connection_cb_t s_connection_cb;
static ble_uart_sync_cb_t s_sync_cb;

static const ble_uuid128_t kSvcUuid = BLE_UUID128_INIT(
    0x9e, 0xca, 0xdc, 0x24, 0x0e, 0xe5, 0xa9, 0xe0,
    0x93, 0xf3, 0xa3, 0xb5, 0x01, 0x00, 0x40, 0x6e);

static const ble_uuid128_t kRxUuid = BLE_UUID128_INIT(
    0x9e, 0xca, 0xdc, 0x24, 0x0e, 0xe5, 0xa9, 0xe0,
    0x93, 0xf3, 0xa3, 0xb5, 0x02, 0x00, 0x40, 0x6e);

static const ble_uuid128_t kTxUuid = BLE_UUID128_INIT(
    0x9e, 0xca, 0xdc, 0x24, 0x0e, 0xe5, 0xa9, 0xe0,
    0x93, 0xf3, 0xa3, 0xb5, 0x03, 0x00, 0x40, 0x6e);

static int gap_event(struct ble_gap_event *event, void *arg);

static int gatt_access_rx(uint16_t conn_handle, uint16_t attr_handle,
                          struct ble_gatt_access_ctxt *ctxt, void *arg) {
    (void)conn_handle;
    (void)attr_handle;
    (void)arg;

    if (ctxt->op == BLE_GATT_ACCESS_OP_WRITE_CHR) {
        const uint16_t om_len = OS_MBUF_PKTLEN(ctxt->om);
        uint8_t buf[128];
        const uint16_t copy = om_len > sizeof(buf) ? sizeof(buf) : om_len;
        os_mbuf_copydata(ctxt->om, 0, copy, buf);
        protocol_feed_rx(buf, copy);
    }
    return 0;
}

static int gatt_access_tx(uint16_t conn_handle, uint16_t attr_handle,
                          struct ble_gatt_access_ctxt *ctxt, void *arg) {
    (void)conn_handle;
    (void)attr_handle;
    (void)arg;

    if (ctxt->op == BLE_GATT_ACCESS_OP_READ_CHR) {
        const uint8_t empty = 0;
        return os_mbuf_append(ctxt->om, &empty, sizeof(empty));
    }
    return BLE_ATT_ERR_READ_NOT_PERMITTED;
}

static void gatt_register_cb(struct ble_gatt_register_ctxt *ctxt, void *arg) {
    (void)arg;
    char buf[BLE_UUID_STR_LEN];

    switch (ctxt->op) {
        case BLE_GATT_REGISTER_OP_SVC:
            ESP_LOGI(TAG, "GATT svc %s handle=%u",
                     ble_uuid_to_str(ctxt->svc.svc_def->uuid, buf), ctxt->svc.handle);
            break;
        case BLE_GATT_REGISTER_OP_CHR:
            ESP_LOGI(TAG, "GATT chr %s def_handle=%u val_handle=%u",
                     ble_uuid_to_str(ctxt->chr.chr_def->uuid, buf),
                     ctxt->chr.def_handle, ctxt->chr.val_handle);
            break;
        default:
            break;
    }
}

static void fill_conn_params(ble_conn_profile_t profile, struct ble_gap_upd_params *params) {
    params->supervision_timeout = BLE_CONN_SUPERVISION;
    if (profile == BLE_CONN_PROFILE_ACTIVE) {
        params->itvl_min = CONN_ACTIVE_ITVL_MIN;
        params->itvl_max = CONN_ACTIVE_ITVL_MAX;
        params->latency = CONN_ACTIVE_LATENCY;
    } else {
        params->itvl_min = CONN_IDLE_ITVL_MIN;
        params->itvl_max = CONN_IDLE_ITVL_MAX;
        params->latency = CONN_IDLE_LATENCY;
    }
}

int ble_uart_request_conn_profile(ble_conn_profile_t profile) {
    if (!ble_uart_is_connected()) {
        return -1;
    }
    if (s_pending_conn_profile == profile && s_conn_profile == profile) {
        return 0;
    }

    struct ble_gap_upd_params params = {0};
    fill_conn_params(profile, &params);
    const int rc = ble_gap_update_params(s_conn_handle, &params);
    if (rc == 0) {
        s_pending_conn_profile = profile;
        ESP_LOGI(TAG, "Requested conn profile %s",
                 profile == BLE_CONN_PROFILE_ACTIVE ? "active" : "idle");
        return 0;
    }

    ESP_LOGW(TAG, "Conn param request failed: %d", rc);
    return rc;
}

static const struct ble_gatt_svc_def kGattSvcs[] = {
    {
        .type = BLE_GATT_SVC_TYPE_PRIMARY,
        .uuid = &kSvcUuid.u,
        .characteristics = (struct ble_gatt_chr_def[]){
            {
                .uuid = &kRxUuid.u,
                .access_cb = gatt_access_rx,
                .flags = BLE_GATT_CHR_F_WRITE | BLE_GATT_CHR_F_WRITE_NO_RSP,
            },
            {
                .uuid = &kTxUuid.u,
                .access_cb = gatt_access_tx,
                .val_handle = &s_tx_val_handle,
                .flags = BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_NOTIFY,
            },
            {0},
        },
    },
    {0},
};

static int gatt_svr_init(void) {
    ble_svc_gap_init();
    ble_svc_gatt_init();

    int rc = ble_gatts_count_cfg(kGattSvcs);
    if (rc != 0) {
        ESP_LOGE(TAG, "gatts count cfg failed: %d", rc);
        return rc;
    }

    rc = ble_gatts_add_svcs(kGattSvcs);
    if (rc != 0) {
        ESP_LOGE(TAG, "gatts add svcs failed: %d", rc);
        return rc;
    }

    return 0;
}

int ble_uart_start_advertising(ble_adv_mode_t mode) {
    if (ble_uart_is_connected()) {
        return 0;
    }

    if (s_advertising) {
        ble_gap_adv_stop();
        s_advertising = false;
    }

    struct ble_gap_adv_params adv_params = {0};
    struct ble_hs_adv_fields fields = {0};
    struct ble_hs_adv_fields rsp_fields = {0};
    const char *name = "OASIS_INTERFACE";

    fields.flags = BLE_HS_ADV_F_DISC_GEN | BLE_HS_ADV_F_BREDR_UNSUP;
    fields.name = (uint8_t *)name;
    fields.name_len = strlen(name);
    fields.name_is_complete = 1;

    int rc = ble_gap_adv_set_fields(&fields);
    if (rc != 0) {
        ESP_LOGE(TAG, "adv fields failed: %d", rc);
        return rc;
    }

    rsp_fields.uuids128 = &kSvcUuid;
    rsp_fields.num_uuids128 = 1;
    rsp_fields.uuids128_is_complete = 1;
    rc = ble_gap_adv_rsp_set_fields(&rsp_fields);
    if (rc != 0) {
        ESP_LOGE(TAG, "adv scan rsp failed: %d", rc);
        return rc;
    }

    adv_params.conn_mode = BLE_GAP_CONN_MODE_UND;
    adv_params.disc_mode = BLE_GAP_DISC_MODE_GEN;
    if (mode == BLE_ADV_FAST) {
        adv_params.itvl_min = BLE_GAP_ADV_ITVL_MS(ADV_FAST_MIN_MS);
        adv_params.itvl_max = BLE_GAP_ADV_ITVL_MS(ADV_FAST_MAX_MS);
    } else {
        adv_params.itvl_min = BLE_GAP_ADV_ITVL_MS(ADV_SLOW_MIN_MS);
        adv_params.itvl_max = BLE_GAP_ADV_ITVL_MS(ADV_SLOW_MAX_MS);
    }

    rc = ble_gap_adv_start(BLE_OWN_ADDR_PUBLIC, NULL, BLE_HS_FOREVER,
                           &adv_params, gap_event, NULL);
    if (rc != 0) {
        ESP_LOGE(TAG, "adv start failed: %d", rc);
        return rc;
    }

    s_advertising = true;
    s_adv_mode = mode;
    ESP_LOGI(TAG, "Advertising %s as %s",
             mode == BLE_ADV_FAST ? "fast" : "slow", name);
    return 0;
}

int ble_uart_stop_advertising(void) {
    if (!s_advertising) {
        return 0;
    }
    const int rc = ble_gap_adv_stop();
    if (rc == 0) {
        s_advertising = false;
        ESP_LOGI(TAG, "Advertising stopped");
    } else {
        ESP_LOGW(TAG, "adv stop failed: %d", rc);
    }
    return rc;
}

static int gap_event(struct ble_gap_event *event, void *arg) {
    (void)arg;
    switch (event->type) {
        case BLE_GAP_EVENT_CONNECT:
            if (event->connect.status == 0) {
                s_conn_handle = event->connect.conn_handle;
                s_notify_enabled = false;
                s_conn_profile = BLE_CONN_PROFILE_IDLE;
                s_pending_conn_profile = BLE_CONN_PROFILE_IDLE;
                s_advertising = false;
                display_set_ble_connected(true);
                ESP_LOGI(TAG, "Connected handle=%u", s_conn_handle);
                ble_uart_request_conn_profile(BLE_CONN_PROFILE_ACTIVE);
                if (s_connection_cb) {
                    s_connection_cb(true);
                }
            } else {
                ESP_LOGW(TAG, "Connect failed: %d", event->connect.status);
                ble_uart_start_advertising(s_adv_mode);
            }
            break;
        case BLE_GAP_EVENT_MTU:
            ESP_LOGI(TAG, "MTU updated conn=%u mtu=%u",
                     event->mtu.conn_handle, event->mtu.value);
            break;
        case BLE_GAP_EVENT_CONN_UPDATE:
            if (event->conn_update.status == 0) {
                s_conn_profile = s_pending_conn_profile;
                ESP_LOGI(TAG, "Conn update ok profile=%s",
                         s_conn_profile == BLE_CONN_PROFILE_ACTIVE ? "active" : "idle");
            } else {
                ESP_LOGW(TAG, "Conn update status=%d", event->conn_update.status);
            }
            break;
        case BLE_GAP_EVENT_DISCONNECT:
            s_conn_handle = BLE_HS_CONN_HANDLE_NONE;
            s_notify_enabled = false;
            s_conn_profile = BLE_CONN_PROFILE_IDLE;
            s_pending_conn_profile = BLE_CONN_PROFILE_IDLE;
            display_set_ble_connected(false);
            ESP_LOGI(TAG, "Disconnected");
            if (s_connection_cb) {
                s_connection_cb(false);
            }
            ble_uart_start_advertising(BLE_ADV_FAST);
            break;
        case BLE_GAP_EVENT_SUBSCRIBE:
            ESP_LOGI(TAG, "Notify subscribe handle=%u tx=%u cur=%d",
                     event->subscribe.attr_handle, s_tx_val_handle,
                     event->subscribe.cur_notify);
            if (event->subscribe.cur_notify) {
                s_notify_enabled = true;
                if (s_notify_subscribed_cb) {
                    s_notify_subscribed_cb();
                }
                ble_uart_request_conn_profile(BLE_CONN_PROFILE_ACTIVE);
            } else {
                s_notify_enabled = false;
            }
            break;
        default:
            break;
    }
    return 0;
}

static void on_sync(void) {
    ble_hs_util_ensure_addr(0);
    if (s_sync_cb) {
        s_sync_cb();
    } else {
        ble_uart_start_advertising(BLE_ADV_FAST);
    }
}

static void host_task(void *param) {
    (void)param;
    nimble_port_run();
    nimble_port_freertos_deinit();
}

void ble_uart_start(void) {
    if (s_started) {
        return;
    }

    ESP_ERROR_CHECK(nimble_port_init());

    ble_hs_cfg.sync_cb = on_sync;
    ble_hs_cfg.gatts_register_cb = gatt_register_cb;
    ble_svc_gap_device_name_set("OASIS_INTERFACE");
    if (gatt_svr_init() != 0) {
        ESP_LOGE(TAG, "GATT server init failed");
        return;
    }

    nimble_port_freertos_init(host_task);
    s_started = true;
    ESP_LOGI(TAG, "NimBLE UART started");
}

bool ble_uart_is_connected(void) {
    return s_conn_handle != BLE_HS_CONN_HANDLE_NONE;
}

bool ble_uart_is_advertising(void) {
    return s_advertising;
}

void ble_uart_set_notify_subscribed_cb(ble_uart_notify_subscribed_cb_t cb) {
    s_notify_subscribed_cb = cb;
}

void ble_uart_set_connection_cb(ble_uart_connection_cb_t cb) {
    s_connection_cb = cb;
}

void ble_uart_set_sync_cb(ble_uart_sync_cb_t cb) {
    s_sync_cb = cb;
}

int ble_uart_notify(const uint8_t *data, size_t len) {
    if (!ble_uart_is_connected() || !data || len == 0) {
        return -1;
    }
    struct os_mbuf *om = ble_hs_mbuf_from_flat(data, len);
    if (!om) {
        return -1;
    }
    const int rc = ble_gatts_notify_custom(s_conn_handle, s_tx_val_handle, om);
    return rc == 0 ? (int)len : -1;
}
