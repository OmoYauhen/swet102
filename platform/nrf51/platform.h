/*
 * Swet102 — functions shared between the platform C files.
 * Released under the GPL License, Version 3.
 */
#pragma once

#include <stdint.h>

/* lcd.c */
void lcd_init(void);
void lcd_flush(const uint8_t *fb);
void lcd_contrast(uint8_t level);

/* hal.c */
void hal_init(void);

/* store.c */
void store_init(void);
void store_sys_evt(uint32_t sys_evt);

/* main.c: measurements reported through hal_diag() */
struct platform_diag {
    uint32_t tick_avg_us;
    uint32_t tick_max_us;
    uint32_t missed_ticks;
    uint32_t stack_free;
    uint32_t ram_kb;
    uint32_t sd_ram_base;
    uint32_t uart_errors;
    uint32_t flush_avg_us;
    uint32_t flush_max_us;
    uint32_t store_errors;
};
extern struct platform_diag g_diag;

/* RTC1 timestamps for measurements (32768 Hz). */
uint32_t platform_ticks(void);
uint32_t platform_us_since(uint32_t t0);
