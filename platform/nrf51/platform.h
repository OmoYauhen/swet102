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
void lcd_orient(uint8_t mode);

/* hal.c */
void hal_init(void);

/* main.c: measurements reported through hal_diag() */
struct platform_diag {
    uint32_t tick_avg_us;
    uint32_t tick_max_us;
    uint32_t missed_ticks;
    uint32_t stack_free;
    uint32_t ram_kb;
    uint32_t sd_ram_base;
    uint32_t uart_errors;
};
extern struct platform_diag g_diag;
