/*
 * Swet102 — C side of the HAL (swet_hal.h).
 * Released under the GPL License, Version 3.
 */
#include <string.h>

#include "swet_hal.h"
#include "platform.h"

#include "board.h"
#include "app_uart.h"
#include "nrf_gpio.h"
#include "nrf_nvic.h"
#include "nrf_soc.h"

static void uart_evt(app_uart_evt_t *evt)
{
    if (evt->evt_type == APP_UART_COMMUNICATION_ERROR ||
        evt->evt_type == APP_UART_FIFO_ERROR) {
        g_diag.uart_errors++;
    }
    /* RX bytes stay in the FIFO until the core polls hal_uart_read(). */
}

void hal_init(void)
{
    nrf_gpio_cfg_input(PIN_BTN_LEFT, NRF_GPIO_PIN_PULLUP);
    nrf_gpio_cfg_input(PIN_BTN_RIGHT, NRF_GPIO_PIN_PULLUP);
    nrf_gpio_cfg_input(PIN_BTN_M, NRF_GPIO_PIN_PULLUP);
    nrf_gpio_cfg_input(PIN_BTN_PWR, NRF_GPIO_PIN_NOPULL);

    static const app_uart_comm_params_t params = {
        .rx_pin_no    = PIN_UART_RX,
        .tx_pin_no    = PIN_UART_TX,
        .rts_pin_no   = 0xFF,
        .cts_pin_no   = 0xFF,
        .flow_control = APP_UART_FLOW_CONTROL_DISABLED,
        .use_parity   = false,
        .baud_rate    = UART_BAUDRATE_BAUDRATE_Baud1200,
    };
    uint32_t err;
    APP_UART_FIFO_INIT(&params, 64, 64, uart_evt, APP_IRQ_PRIORITY_LOW, err);
    APP_ERROR_CHECK(err);
}

void hal_display_flush(const uint8_t *fb)
{
    static uint32_t sum_us, n;
    uint32_t t0 = platform_ticks();
    lcd_flush(fb);
    uint32_t us = platform_us_since(t0);
    if (us > g_diag.flush_max_us) {
        g_diag.flush_max_us = us;
    }
    sum_us += us;
    if (++n == 16) {
        g_diag.flush_avg_us = sum_us / n;
        sum_us = n = 0;
    }
}
void hal_display_contrast(uint8_t level) { lcd_contrast(level); }

uint8_t hal_buttons(void)
{
    uint8_t b = 0;
    if (!nrf_gpio_pin_read(PIN_BTN_LEFT))  b |= 1;
    if (!nrf_gpio_pin_read(PIN_BTN_RIGHT)) b |= 2;
    if (!nrf_gpio_pin_read(PIN_BTN_M))     b |= 4;
    if (nrf_gpio_pin_read(PIN_BTN_PWR))    b |= 8;
    return b;
}

void hal_uart_write(const uint8_t *buf, uint8_t len)
{
    for (uint8_t i = 0; i < len; i++) {
        (void)app_uart_put(buf[i]); /* 64-byte TX FIFO; requests are ≤ 5 bytes */
    }
}

int16_t hal_uart_read(void)
{
    uint8_t b;
    return app_uart_get(&b) == NRF_SUCCESS ? b : -1;
}

/* hal_store_* live in store.c (FDS), hal_ble_* in ble.c, hal_reboot_to_dfu in dfu.c. */

void hal_power_off(void)
{
    nrf_gpio_pin_clear(PIN_POWER_HOLD);
    for (;;) {
        /* the regulator drops out; the watchdog covers a stuck latch */
    }
}

uint32_t hal_diag(uint8_t id)
{
    switch (id) {
    case 0: return g_diag.tick_avg_us;
    case 1: return g_diag.tick_max_us;
    case 2: return g_diag.missed_ticks;
    case 3: return g_diag.stack_free;
    case 4: return g_diag.ram_kb;
    case 5: return g_diag.sd_ram_base;
    case 6: return g_diag.uart_errors;
    case 7: return g_diag.flush_avg_us;
    case 8: return g_diag.flush_max_us;
    case 9: return g_diag.store_errors;
    case 10: return g_diag.ble_dropped;
    default: return 0;
    }
}

void hal_panic(void)
{
    (void)sd_nvic_SystemReset();
    for (;;) {}
}
