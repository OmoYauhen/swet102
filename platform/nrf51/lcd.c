/*
 * Swet102 — SH1107 OLED over SPI0, blocking.
 *
 * Init sequence and SPI setup from Swang Stodva's lcd.c
 * (Copyright (C) lowPerformer, 2019; init bytes sampled by casainho from the
 * stock SW102 firmware). Released under the GPL License, Version 3.
 *
 * Blocking SPI on purpose: the non-blocking transaction manager's IRQ rate
 * stalled the CPU just as much (see Swang Stodva lcd.c).
 */
#include "platform.h"

#include "board.h"
#include "app_error.h"
#include "nrf_delay.h"
#include "nrf_drv_spi.h"
#include "nrf_gpio.h"

static const nrf_drv_spi_t m_spi = NRF_DRV_SPI_INSTANCE(0);

static const uint8_t m_init[] = {
    0xAE,       /* display off */
    0xA8, 0x3F, /* multiplex ratio */
    0xD5, 0x50, /* clock divide / oscillator */
    0xC0,       /* COM scan direction (overridden by lcd_orient) */
    0xD3, 0x60, /* display offset */
    0xDC, 0x00, /* display start line */
    0x21,       /* memory addressing mode */
    0x81, 0xFF, /* contrast */
    0xA0,       /* segment remap (overridden by lcd_orient) */
    0xA4,       /* display follows RAM */
    0xA6,       /* not inverted */
    0xAD, 0x8A, /* DC-DC */
    0xD9, 0x1F, /* discharge / precharge */
    0xDB, 0x30, /* VCOM deselect level */
    0xAF,       /* display on */
};

static void send_cmd(const uint8_t *cmds, uint8_t n)
{
    nrf_gpio_pin_clear(PIN_LCD_DC);
    APP_ERROR_CHECK(nrf_drv_spi_transfer(&m_spi, cmds, n, NULL, 0));
}

void lcd_init(void)
{
    nrf_drv_spi_config_t cfg = NRF_DRV_SPI_DEFAULT_CONFIG;
    cfg.ss_pin    = PIN_LCD_CS;
    cfg.mosi_pin  = PIN_LCD_MOSI;
    cfg.sck_pin   = PIN_LCD_SCK;
    cfg.miso_pin  = NRF_DRV_SPI_PIN_NOT_USED;
    cfg.frequency = NRF_DRV_SPI_FREQ_4M; /* SH1107 datasheet p. 52, Vdd 3.3 V */
    cfg.mode      = NRF_DRV_SPI_MODE_0;
    cfg.bit_order = NRF_DRV_SPI_BIT_ORDER_MSB_FIRST;
    APP_ERROR_CHECK(nrf_drv_spi_init(&m_spi, &cfg, NULL));

    /* The reset line has been held low since gpio init; release it. */
    nrf_delay_us(20);
    nrf_gpio_pin_set(PIN_LCD_RES);
    nrf_delay_us(4);

    send_cmd(m_init, sizeof m_init);
}

/* The framebuffer's 64 rows of 16 bytes are the controller's 64 columns. */
void lcd_flush(const uint8_t *fb)
{
    uint8_t page[3] = {0xB0, 0x00, 0x10};
    for (uint8_t i = 0; i < 64; i++) {
        page[1] = i & 0x0F;
        page[2] = 0x10 | (i >> 4);
        send_cmd(page, sizeof page);
        nrf_gpio_pin_set(PIN_LCD_DC);
        APP_ERROR_CHECK(nrf_drv_spi_transfer(&m_spi, fb + 16 * i, 16, NULL, 0));
    }
}

void lcd_contrast(uint8_t level)
{
    const uint8_t cmd[2] = {0x81, level};
    send_cmd(cmd, sizeof cmd);
}

void lcd_orient(uint8_t mode)
{
    const uint8_t cmd[2] = {
        (uint8_t)(0xA0 | (mode & 1)),          /* segment remap */
        (uint8_t)((mode & 2) ? 0xC8 : 0xC0),   /* COM scan direction */
    };
    send_cmd(cmd, sizeof cmd);
}
