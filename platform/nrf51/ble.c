/*
 * Swet102 — BLE peripheral (TECH_DESIGN §9): advertising policy, the Swet102
 * GATT service, Device Information, connection parameters.
 *
 * The C side owns the stack; the Rust core decides what goes into the
 * characteristics (hal_ble_notify) and handles control writes
 * (swet_ble_control). Every function here runs in the main loop: SoftDevice
 * events arrive through app_scheduler.
 *
 * No pairing, no bonding, no Peer Manager: any phone may connect and
 * subscribe (PRODUCT §9). One connection at a time; advertising restarts as
 * soon as it drops.
 * Released under the GPL License, Version 3.
 */
#include <string.h>

#include "swet_hal.h"
#include "platform.h"

#include "app_error.h"
#include "app_timer.h"
#include "app_util.h"
#include "ble.h"
#include "ble_advdata.h"
#include "ble_conn_params.h"
#include "ble_dis.h"
#include "ble_gap.h"
#include "ble_gatts.h"
#include "ble_hci.h"
#include "ble_srv_common.h"

#define DEVICE_NAME       "swet102"
#define MODEL_NAME        "SW102"
#define MANUFACTURER_NAME "PET"

/* Advertising: fast for 30 s after boot or a disconnect, then slow forever. */
#define ADV_FAST_INTERVAL MSEC_TO_UNITS(100, UNIT_0_625_MS)
#define ADV_FAST_TIMEOUT  30 /* s */
#define ADV_SLOW_INTERVAL MSEC_TO_UNITS(1000, UNIT_0_625_MS)

/* Relaxed parameters so a phone in a pocket keeps the link. */
#define MIN_CONN_INTERVAL MSEC_TO_UNITS(50, UNIT_1_25_MS)
#define MAX_CONN_INTERVAL MSEC_TO_UNITS(100, UNIT_1_25_MS)
#define SLAVE_LATENCY     0
#define CONN_SUP_TIMEOUT  MSEC_TO_UNITS(4000, UNIT_10_MS)
#define FIRST_UPDATE_DELAY APP_TIMER_TICKS(5000, 0)
#define NEXT_UPDATE_DELAY  APP_TIMER_TICKS(30000, 0)

/* 8f03xxxx-da4c-453d-a163-41a592a0e9fd, little-endian; xxxx = bytes 12..13. */
static const ble_uuid128_t m_base_uuid = {{
    0xfd, 0xe9, 0xa0, 0x92, 0xa5, 0x41, 0x63, 0xa1,
    0x3d, 0x45, 0x4c, 0xda, 0x00, 0x00, 0x03, 0x8f,
}};
#define UUID_SERVICE   0x0001
#define UUID_TELEMETRY 0x0002
#define UUID_COMMAND   0x0003
#define UUID_CONTROL   0x0004
#define UUID_TRIPS     0x0005

#define MAX_VALUE_LEN 20 /* default ATT payload: no MTU exchange on S130 */

/* hal_ble_state() bits, as swet_heart::BleState */
#define ST_CONNECTED 1
#define ST_TEL_SUB   2
#define ST_CMD_SUB   4
#define ST_TRIPS_SUB 8

/* Indexed by swet_heart::BleChannel: telemetry, command, trips. */
#define CHANNELS 3
static ble_gatts_char_handles_t m_chars[CHANNELS];
static const uint8_t m_sub_bit[CHANNELS] = {ST_TEL_SUB, ST_CMD_SUB, ST_TRIPS_SUB};
static ble_gatts_char_handles_t m_control;

static uint16_t m_conn = BLE_CONN_HANDLE_INVALID;
static uint8_t  m_state;
static uint8_t  m_uuid_type;

static void adv_start(bool fast)
{
    ble_gap_adv_params_t p;
    memset(&p, 0, sizeof p);
    p.type     = BLE_GAP_ADV_TYPE_ADV_IND;
    p.fp       = BLE_GAP_ADV_FP_ANY;
    p.interval = fast ? ADV_FAST_INTERVAL : ADV_SLOW_INTERVAL;
    p.timeout  = fast ? ADV_FAST_TIMEOUT : BLE_GAP_ADV_TIMEOUT_GENERAL_UNLIMITED;
    uint32_t err = sd_ble_gap_adv_start(&p);
    if (err != NRF_ERROR_INVALID_STATE) { /* already advertising: fine */
        APP_ERROR_CHECK(err);
    }
}

static void gap_init(void)
{
    ble_gap_conn_sec_mode_t open;
    BLE_GAP_CONN_SEC_MODE_SET_OPEN(&open);
    APP_ERROR_CHECK(sd_ble_gap_device_name_set(&open, (const uint8_t *)DEVICE_NAME,
                                               strlen(DEVICE_NAME)));

    ble_gap_conn_params_t cp = {
        .min_conn_interval = MIN_CONN_INTERVAL,
        .max_conn_interval = MAX_CONN_INTERVAL,
        .slave_latency     = SLAVE_LATENCY,
        .conn_sup_timeout  = CONN_SUP_TIMEOUT,
    };
    APP_ERROR_CHECK(sd_ble_gap_ppcp_set(&cp));
}

/* One characteristic of the Swet102 service. Values are variable length,
 * kept by the stack; notify characteristics get an open CCCD. */
static void char_add(uint16_t service, uint16_t uuid, bool read, bool notify, bool write,
                     ble_gatts_char_handles_t *out)
{
    ble_gatts_attr_md_t cccd_md;
    memset(&cccd_md, 0, sizeof cccd_md);
    BLE_GAP_CONN_SEC_MODE_SET_OPEN(&cccd_md.read_perm);
    BLE_GAP_CONN_SEC_MODE_SET_OPEN(&cccd_md.write_perm);
    cccd_md.vloc = BLE_GATTS_VLOC_STACK;

    ble_gatts_char_md_t char_md;
    memset(&char_md, 0, sizeof char_md);
    char_md.char_props.read          = read;
    char_md.char_props.notify        = notify;
    char_md.char_props.write         = write;
    char_md.char_props.write_wo_resp = write;
    char_md.p_cccd_md                = notify ? &cccd_md : NULL;

    ble_gatts_attr_md_t attr_md;
    memset(&attr_md, 0, sizeof attr_md);
    if (read) {
        BLE_GAP_CONN_SEC_MODE_SET_OPEN(&attr_md.read_perm);
    } else {
        BLE_GAP_CONN_SEC_MODE_SET_NO_ACCESS(&attr_md.read_perm);
    }
    if (write) {
        BLE_GAP_CONN_SEC_MODE_SET_OPEN(&attr_md.write_perm);
    } else {
        BLE_GAP_CONN_SEC_MODE_SET_NO_ACCESS(&attr_md.write_perm);
    }
    attr_md.vloc = BLE_GATTS_VLOC_STACK;
    attr_md.vlen = 1;

    ble_uuid_t ble_uuid = {.uuid = uuid, .type = m_uuid_type};
    static uint8_t zero[1];
    ble_gatts_attr_t value = {
        .p_uuid    = &ble_uuid,
        .p_attr_md = &attr_md,
        .init_len  = 1,
        .max_len   = MAX_VALUE_LEN,
        .p_value   = zero,
    };
    APP_ERROR_CHECK(sd_ble_gatts_characteristic_add(service, &char_md, &value, out));
}

static void services_init(void)
{
    APP_ERROR_CHECK(sd_ble_uuid_vs_add(&m_base_uuid, &m_uuid_type));
    ble_uuid_t svc_uuid = {.uuid = UUID_SERVICE, .type = m_uuid_type};
    uint16_t svc;
    APP_ERROR_CHECK(sd_ble_gatts_service_add(BLE_GATTS_SRVC_TYPE_PRIMARY, &svc_uuid, &svc));
    char_add(svc, UUID_TELEMETRY, true, true, false, &m_chars[0]);
    char_add(svc, UUID_COMMAND, false, true, false, &m_chars[1]);
    char_add(svc, UUID_CONTROL, false, false, true, &m_control);
    char_add(svc, UUID_TRIPS, true, true, false, &m_chars[2]);

    /* Device Information: firmware version from the Rust core's build info. */
    static char fw_rev[24];
    uint8_t n = swet_version((uint8_t *)fw_rev, sizeof fw_rev);
    ble_dis_init_t dis;
    memset(&dis, 0, sizeof dis);
    dis.fw_rev_str.length         = n;
    dis.fw_rev_str.p_str          = (uint8_t *)fw_rev;
    ble_srv_ascii_to_utf8(&dis.model_num_str, MODEL_NAME);
    ble_srv_ascii_to_utf8(&dis.manufact_name_str, MANUFACTURER_NAME);
    BLE_GAP_CONN_SEC_MODE_SET_OPEN(&dis.dis_attr_md.read_perm);
    BLE_GAP_CONN_SEC_MODE_SET_NO_ACCESS(&dis.dis_attr_md.write_perm);
    APP_ERROR_CHECK(ble_dis_init(&dis));
}

/* Advertising: flags + name; the service UUID goes in the scan response
 * (both together would overflow 31 bytes). */
static void advdata_init(void)
{
    ble_advdata_t adv;
    memset(&adv, 0, sizeof adv);
    adv.name_type = BLE_ADVDATA_FULL_NAME;
    adv.flags     = BLE_GAP_ADV_FLAGS_LE_ONLY_GENERAL_DISC_MODE;

    ble_uuid_t uuid = {.uuid = UUID_SERVICE, .type = m_uuid_type};
    ble_advdata_t srsp;
    memset(&srsp, 0, sizeof srsp);
    srsp.uuids_complete.uuid_cnt = 1;
    srsp.uuids_complete.p_uuids  = &uuid;

    APP_ERROR_CHECK(ble_advdata_set(&adv, &srsp));
}

/* Never disconnect over rejected parameters (SS did, TECH_DESIGN §9.3). */
static void conn_params_error(uint32_t err) { (void)err; }

static void conn_params_init(void)
{
    ble_conn_params_init_t cp;
    memset(&cp, 0, sizeof cp);
    cp.p_conn_params                  = NULL; /* the PPCP set in gap_init */
    cp.first_conn_params_update_delay = FIRST_UPDATE_DELAY;
    cp.next_conn_params_update_delay  = NEXT_UPDATE_DELAY;
    cp.max_conn_params_update_count   = 1; /* ask once */
    cp.start_on_notify_cccd_handle    = BLE_GATT_HANDLE_INVALID;
    cp.disconnect_on_fail             = false;
    cp.error_handler                  = conn_params_error;
    APP_ERROR_CHECK(ble_conn_params_init(&cp));
}

void ble_init(void)
{
    gap_init();
    services_init();
    advdata_init();
    conn_params_init();
    adv_start(true);
}

static void on_write(const ble_gatts_evt_write_t *w)
{
    if (w->handle == m_control.value_handle) {
        swet_ble_control(w->data, (uint8_t)(w->len > 255 ? 255 : w->len));
        return;
    }
    for (int i = 0; i < CHANNELS; i++) {
        if (w->handle == m_chars[i].cccd_handle && w->len == 2) {
            if (ble_srv_is_notification_enabled(w->data)) {
                m_state |= m_sub_bit[i];
            } else {
                m_state &= (uint8_t)~m_sub_bit[i];
            }
        }
    }
}

void ble_on_evt(ble_evt_t *evt)
{
    ble_conn_params_on_ble_evt(evt);

    switch (evt->header.evt_id) {
    case BLE_GAP_EVT_CONNECTED:
        m_conn  = evt->evt.gap_evt.conn_handle;
        m_state = ST_CONNECTED; /* no bonding: subscriptions start empty */
        break;

    case BLE_GAP_EVT_DISCONNECTED:
        m_conn  = BLE_CONN_HANDLE_INVALID;
        m_state = 0;
        adv_start(true);
        break;

    case BLE_GAP_EVT_TIMEOUT:
        if (evt->evt.gap_evt.params.timeout.src == BLE_GAP_TIMEOUT_SRC_ADVERTISING) {
            adv_start(false); /* fast phase over: slow, without a timeout */
        }
        break;

    case BLE_GAP_EVT_SEC_PARAMS_REQUEST:
        (void)sd_ble_gap_sec_params_reply(m_conn, BLE_GAP_SEC_STATUS_PAIRING_NOT_SUPP,
                                          NULL, NULL);
        break;

    case BLE_GATTS_EVT_SYS_ATTR_MISSING:
        (void)sd_ble_gatts_sys_attr_set(m_conn, NULL, 0, 0);
        break;

    case BLE_GATTS_EVT_WRITE:
        on_write(&evt->evt.gatts_evt.params.write);
        break;

    case BLE_EVT_USER_MEM_REQUEST: /* no long writes */
        (void)sd_ble_user_mem_reply(m_conn, NULL);
        break;

    case BLE_GATTS_EVT_TIMEOUT:
        (void)sd_ble_gap_disconnect(m_conn, BLE_HCI_REMOTE_USER_TERMINATED_CONNECTION);
        break;

    default:
        break;
    }
}

uint8_t hal_ble_state(void) { return m_state; }

/* Set the value (what a read returns) and notify a subscribed phone.
 * Fire-and-forget: a notification that finds no free buffer is dropped. */
void hal_ble_notify(uint8_t ch, const uint8_t *buf, uint8_t len)
{
    if (ch >= CHANNELS || len > MAX_VALUE_LEN) {
        return;
    }
    uint16_t handle = m_chars[ch].value_handle;
    ble_gatts_value_t v = {.len = len, .offset = 0, .p_value = (uint8_t *)buf};
    (void)sd_ble_gatts_value_set(BLE_CONN_HANDLE_INVALID, handle, &v);

    if (m_conn == BLE_CONN_HANDLE_INVALID || !(m_state & m_sub_bit[ch])) {
        return;
    }
    uint16_t n = len;
    ble_gatts_hvx_params_t hvx = {
        .handle = handle,
        .type   = BLE_GATT_HVX_NOTIFICATION,
        .offset = 0,
        .p_len  = &n,
        .p_data = (uint8_t *)buf,
    };
    if (sd_ble_gatts_hvx(m_conn, &hvx) != NRF_SUCCESS) {
        g_diag.ble_dropped++;
    }
}

void hal_ble_address(uint8_t *out)
{
    ble_gap_addr_t a;
    if (sd_ble_gap_address_get(&a) == NRF_SUCCESS) {
        memcpy(out, a.addr, 6);
    } else {
        memset(out, 0, 6);
    }
}
