/*
 * Swet102 — persistent record in flash via SDK 12 FDS (TECH_DESIGN §8.2).
 *
 * One record (file 0x5E70, key 0x0001), 48 bytes = 12 words, CRC-checked.
 * Saves are asynchronous: hal_store_save() queues an update and returns;
 * hal_store_busy() stays true until FDS reports the write. Garbage collection
 * runs only when FDS says the pages are full, then the write is retried.
 * The FDS pages sit right below the bootloader (0x37C00–0x3ABFF) and are
 * reserved in swet102.ld.
 *
 * Released under the GPL License, Version 3.
 */
#include <string.h>

#include "swet_hal.h"
#include "platform.h"

#include "app_scheduler.h"
#include "fds.h"
#include "fstorage.h"
#include "nrf_soc.h"

#define FILE_ID   0x5E70
#define REC_KEY   0x0001
#define REC_WORDS 12 /* swet_heart::STORE_LEN / 4 */

static volatile bool m_init_done;
static volatile bool m_busy;
static bool m_retry_after_gc;
static uint32_t m_buf[REC_WORDS]; /* must stay valid until the write event */

static void start_write(void);

static void fds_evt(fds_evt_t const *e)
{
    switch (e->id) {
    case FDS_EVT_INIT:
        m_init_done = true;
        break;
    case FDS_EVT_WRITE:
    case FDS_EVT_UPDATE:
        if (e->result != FDS_SUCCESS) {
            g_diag.store_errors++;
        }
        m_busy = false;
        break;
    case FDS_EVT_GC:
        if (m_retry_after_gc) {
            m_retry_after_gc = false;
            start_write();
        } else {
            m_busy = false;
        }
        break;
    default:
        break;
    }
}

static void start_write(void)
{
    fds_record_chunk_t chunk = {.p_data = m_buf, .length_words = REC_WORDS};
    fds_record_t rec = {
        .file_id = FILE_ID,
        .key = REC_KEY,
        .data = {.p_chunks = &chunk, .num_chunks = 1},
    };
    fds_record_desc_t desc;
    fds_find_token_t token;
    memset(&token, 0, sizeof token);

    ret_code_t r = (fds_record_find(FILE_ID, REC_KEY, &desc, &token) == FDS_SUCCESS)
                       ? fds_record_update(&desc, &rec)
                       : fds_record_write(&desc, &rec);
    if (r == FDS_ERR_NO_SPACE_IN_FLASH && !m_retry_after_gc) {
        m_retry_after_gc = true;
        r = fds_gc();
    }
    if (r != FDS_SUCCESS) {
        m_retry_after_gc = false;
        g_diag.store_errors++;
        m_busy = false;
    }
}

/* fstorage (under FDS) completes flash operations on SoftDevice system events. */
void store_sys_evt(uint32_t sys_evt)
{
    fs_sys_event_handler(sys_evt);
}

void store_init(void)
{
    if (fds_register(fds_evt) != FDS_SUCCESS || fds_init() != FDS_SUCCESS) {
        g_diag.store_errors++;
        return;
    }
    /* Mounting the pages may need flash writes on first boot; their events
     * arrive through the scheduler. Wait here, before the main loop starts. */
    while (!m_init_done) {
        app_sched_execute();
        (void)sd_app_evt_wait();
    }
}

bool hal_store_load(uint8_t *buf, uint16_t len)
{
    fds_record_desc_t desc;
    fds_find_token_t token;
    fds_flash_record_t rec;
    memset(&token, 0, sizeof token);
    if (!m_init_done || len != REC_WORDS * 4 ||
        fds_record_find(FILE_ID, REC_KEY, &desc, &token) != FDS_SUCCESS) {
        return false;
    }
    if (fds_record_open(&desc, &rec) != FDS_SUCCESS) { /* also fails on a bad CRC */
        return false;
    }
    bool ok = rec.p_header->tl.length_words == REC_WORDS;
    if (ok) {
        memcpy(buf, rec.p_data, len);
    }
    (void)fds_record_close(&desc);
    return ok;
}

void hal_store_save(const uint8_t *buf, uint16_t len)
{
    if (!m_init_done || m_busy || len != REC_WORDS * 4) {
        return; /* the core only saves when not busy */
    }
    memcpy(m_buf, buf, len);
    m_busy = true;
    start_write();
}

bool hal_store_busy(void)
{
    return m_busy;
}
