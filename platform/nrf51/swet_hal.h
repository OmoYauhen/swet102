/*
 * Swet102 — the FFI boundary between the C platform layer and the Rust core.
 * Mirrors `mod ffi` in crates/swet-fw/src/lib.rs (TECH_DESIGN §3.2).
 * Only fixed-width integers, bool and pointer + length cross it.
 * Released under the GPL License, Version 3.
 */
#pragma once

#include <stdbool.h>
#include <stdint.h>

/* Implemented in C (platform/nrf51/hal.c), called by Rust. */
void     hal_display_flush(const uint8_t *fb);      /* 64 rows × 16 bytes */
void     hal_display_contrast(uint8_t level);
uint8_t  hal_buttons(void);                         /* bit0 LEFT, bit1 RIGHT, bit2 M, bit3 PWR */
void     hal_uart_write(const uint8_t *buf, uint8_t len);
int16_t  hal_uart_read(void);                       /* -1 = empty */
bool     hal_store_load(uint8_t *buf, uint16_t len);
void     hal_store_save(const uint8_t *buf, uint16_t len);
bool     hal_store_busy(void);
uint8_t  hal_ble_state(void);
void     hal_ble_notify(uint8_t ch, const uint8_t *buf, uint8_t len);
void     hal_ble_address(uint8_t *out);             /* 6 bytes */
void     hal_power_off(void);
void     hal_reboot_to_dfu(void);
uint32_t hal_diag(uint8_t id);                      /* swet_heart::Diag */
void     hal_panic(void) __attribute__((noreturn));

/* Implemented in Rust (crates/swet-fw), called by the main loop only. */
void swet_init(uint32_t now_ms);
void swet_tick(uint32_t now_ms);
void swet_ble_control(const uint8_t *data, uint8_t len);
/* Copies the version string (no NUL) into out; returns its length. Pure: may
 * be called before swet_init(). */
uint8_t swet_version(uint8_t *out, uint8_t cap);
