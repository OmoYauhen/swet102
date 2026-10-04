/*
 * Swet102 — SW102 board pins (from Swang Stodva's custom_board.h).
 * Released under the GPL License, Version 3.
 */
#pragma once

#define PIN_POWER_HOLD   9

#define PIN_LCD_RES      5
#define PIN_LCD_DC       30
#define PIN_LCD_CS       4
#define PIN_LCD_SCK      6
#define PIN_LCD_MOSI     7

/* Landscape names; Swang Stodva's portrait UP/DOWN are RIGHT/LEFT here. */
#define PIN_BTN_RIGHT    2   /* pull-up, active low */
#define PIN_BTN_LEFT     19  /* pull-up, active low */
#define PIN_BTN_M        14  /* pull-up, active low */
#define PIN_BTN_PWR      10  /* no pull, active high */

#define PIN_UART_TX      12
#define PIN_UART_RX      11

/* LF clock for the SoftDevice: internal RC, as on the stock display. */
#define NRF_CLOCK_LFCLKSRC {.source        = NRF_CLOCK_LF_SRC_RC, \
                            .rc_ctiv       = 16,                  \
                            .rc_temp_ctiv  = 2,                   \
                            .xtal_accuracy = 0}
