/*
 * Swet102 — reboot into the bootloader's BLE DFU mode (TECH_DESIGN §9.5).
 *
 * casainho's bootloader is the nRF5 SDK 12 secure bootloader. It does NOT
 * look at GPREGRET (that was SDK 11's 0xB1); nrf_dfu_enter_check() enters DFU
 * on its button or when the settings page says `enter_buttonless_dfu == 1`.
 * So we set that flag in the bootloader settings page (0x3FC00), with the
 * CRC the bootloader checks, and reset.
 *
 * The settings page is CRC-protected: a wrong CRC makes the bootloader wipe
 * it, which marks the app invalid. So we only touch a page whose CRC checks
 * out, and change nothing but the flag. Layout (SDK 12.3 nrf_dfu_types.h,
 * verified against `nrfutil settings generate` output): word 0 = CRC-32 of
 * bytes 4..91, word 22 (offset 88) = enter_buttonless_dfu.
 * Released under the GPL License, Version 3.
 */
#include <stdbool.h>
#include <stdint.h>
#include <string.h>

#include "swet_hal.h"

#include "nrf.h"
#include "nrf_sdm.h"

#define SETTINGS_ADDR      0x0003FC00u
#define PAGE_WORDS         256u  /* 1 KB code page on nRF51 */
#define CRC_FROM           4u    /* the CRC covers bytes 4..91 */
#define CRC_LEN            88u
#define WORD_CRC           0u
#define WORD_ENTER_DFU     22u   /* offset 88 */

static uint32_t m_page[PAGE_WORDS];

/* CRC-32 (IEEE, reflected), same as the SDK's crc32_compute(). */
static uint32_t crc32(const uint8_t *p, uint32_t n)
{
    uint32_t crc = 0xFFFFFFFFu;
    while (n--) {
        crc ^= *p++;
        for (int k = 0; k < 8; k++) {
            crc = (crc >> 1) ^ (0xEDB88320u & (0u - (crc & 1u)));
        }
    }
    return ~crc;
}

static uint32_t settings_crc(void)
{
    return crc32((const uint8_t *)m_page + CRC_FROM, CRC_LEN);
}

static void nvmc_wait(void)
{
    while (NRF_NVMC->READY == NVMC_READY_READY_Busy) {
    }
}

/* Plain NVMC access: only valid with the SoftDevice disabled. */
static void rewrite_settings_page(void)
{
    NRF_NVMC->CONFIG = NVMC_CONFIG_WEN_Een << NVMC_CONFIG_WEN_Pos;
    nvmc_wait();
    NRF_NVMC->ERASEPAGE = SETTINGS_ADDR;
    nvmc_wait();
    NRF_NVMC->CONFIG = NVMC_CONFIG_WEN_Wen << NVMC_CONFIG_WEN_Pos;
    nvmc_wait();
    volatile uint32_t *dst = (volatile uint32_t *)SETTINGS_ADDR;
    for (uint32_t i = 0; i < PAGE_WORDS; i++) {
        if (m_page[i] != 0xFFFFFFFFu) { /* erased already */
            dst[i] = m_page[i];
            nvmc_wait();
        }
    }
    NRF_NVMC->CONFIG = NVMC_CONFIG_WEN_Ren << NVMC_CONFIG_WEN_Pos;
    nvmc_wait();
}

void hal_reboot_to_dfu(void)
{
    memcpy(m_page, (const void *)SETTINGS_ADDR, sizeof m_page);
    bool valid = m_page[WORD_CRC] != 0xFFFFFFFFu && settings_crc() == m_page[WORD_CRC];
    if (valid && m_page[WORD_ENTER_DFU] != 1u) {
        m_page[WORD_ENTER_DFU] = 1u;
        m_page[WORD_CRC] = settings_crc();
        (void)sd_softdevice_disable(); /* frees the NVMC; BLE stops here */
        rewrite_settings_page();
    }
    /* Without a valid page we don't touch it and only reset: the M+PWR
     * button combo remains the way in. */
    NVIC_SystemReset();
}
