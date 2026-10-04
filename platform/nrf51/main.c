/*
 * Swet102 — nRF51 entry point and main loop (TECH_DESIGN §4.1).
 *
 * The 20 ms timer ISR only counts ticks. Everything else — SoftDevice events
 * through app_scheduler and every swet_tick() — runs here in the main loop.
 * Released under the GPL License, Version 3.
 */
#include <stdbool.h>
#include <stdint.h>

#include "swet_hal.h"
#include "platform.h"

#include "board.h"
#include "app_error.h"
#include "app_scheduler.h"
#include "app_timer.h"
#include "ble.h"
#include "nrf.h"
#include "nrf_drv_wdt.h"
#include "nrf_gpio.h"
#include "nrf_nvic.h"
#include "softdevice_handler.h"
#include "softdevice_handler_appsh.h"

#define TICK_MS                 20
#define APP_TIMER_PRESCALER     0
#define APP_TIMER_OP_QUEUE_SIZE 4
#define SCHED_EVT_SIZE          0   /* SoftDevice events carry no payload; the tick timer bypasses the scheduler */
#define SCHED_QUEUE_SIZE        16
#define STACK_PAINT             0xDEADBEEFu
#define MAX_CATCH_UP            5   /* ticks run back-to-back at most */

struct platform_diag g_diag;

APP_TIMER_DEF(m_tick_timer);
static volatile uint32_t m_ticks_isr;
static nrf_drv_wdt_channel_id m_wdt;

extern uint32_t __StackLimit;
extern uint32_t __StackTop;
extern uint32_t __data_start__;

static void tick_isr(void *ctx)
{
    (void)ctx;
    m_ticks_isr++;
}

static void wdt_evt(void) {}

/* Fill the unused stack so we can measure the high-water mark later. */
static void stack_paint(void)
{
    uint32_t *p = &__StackLimit;
    uint32_t *sp = (uint32_t *)__get_MSP();
    while (p < sp - 16) {
        *p++ = STACK_PAINT;
    }
}

static uint32_t stack_free_bytes(void)
{
    const uint32_t *p = &__StackLimit;
    while (p < &__StackTop && *p == STACK_PAINT) {
        p++;
    }
    return (uint32_t)((uintptr_t)p - (uintptr_t)&__StackLimit);
}

static void ble_evt(ble_evt_t *evt) { (void)evt; }

static void softdevice_init(void)
{
    nrf_clock_lf_cfg_t lf = NRF_CLOCK_LFCLKSRC;
    SOFTDEVICE_HANDLER_APPSH_INIT(&lf, true);

    ble_enable_params_t p;
    APP_ERROR_CHECK(softdevice_enable_get_default_config(0, 1, &p));
    p.common_enable_params.vs_uuid_count = 1;

    /* Ask the SoftDevice directly so we learn the RAM base it needs. */
    uint32_t ram_base = (uint32_t)&__data_start__;
    uint32_t err = sd_ble_enable(&p, &ram_base);
    g_diag.sd_ram_base = ram_base;
    APP_ERROR_CHECK(err);
    APP_ERROR_CHECK(softdevice_ble_evt_handler_set(ble_evt));
}

static void gpio_init(void)
{
    nrf_gpio_cfg_output(PIN_POWER_HOLD);
    nrf_gpio_pin_set(PIN_POWER_HOLD); /* keep the regulator on after PWR is released */

    nrf_gpio_cfg_output(PIN_LCD_DC);
    nrf_gpio_pin_set(PIN_LCD_DC);
    nrf_gpio_cfg_output(PIN_LCD_RES);
    nrf_gpio_pin_clear(PIN_LCD_RES); /* hold the OLED in reset until lcd_init */
}

static void measure(uint32_t t0)
{
    static uint32_t sum_us, n;
    uint32_t ticks;
    (void)app_timer_cnt_diff_compute(app_timer_cnt_get(), t0, &ticks);
    uint32_t us = (uint32_t)(((uint64_t)ticks * 1000000u) >> 15); /* RTC1 @ 32768 Hz */
    if (us > g_diag.tick_max_us) {
        g_diag.tick_max_us = us;
    }
    sum_us += us;
    if (++n == 50) { /* average over one second */
        g_diag.tick_avg_us = sum_us / n;
        sum_us = n = 0;
    }
}

int main(void)
{
    stack_paint();
    gpio_init();

    g_diag.ram_kb = NRF_FICR->NUMRAMBLOCK * NRF_FICR->SIZERAMBLOCKS / 1024;

    APP_SCHED_INIT(SCHED_EVT_SIZE, SCHED_QUEUE_SIZE);
    softdevice_init();
    APP_TIMER_INIT(APP_TIMER_PRESCALER, APP_TIMER_OP_QUEUE_SIZE, NULL);

    lcd_init();
    hal_init();

    swet_init(0);

    nrf_drv_wdt_config_t wdt_cfg = NRF_DRV_WDT_DEAFULT_CONFIG; /* 2 s, paused while halted */
    APP_ERROR_CHECK(nrf_drv_wdt_init(&wdt_cfg, wdt_evt));
    APP_ERROR_CHECK(nrf_drv_wdt_channel_alloc(&m_wdt));
    nrf_drv_wdt_enable();

    APP_ERROR_CHECK(app_timer_create(&m_tick_timer, APP_TIMER_MODE_REPEATED, tick_isr));
    APP_ERROR_CHECK(app_timer_start(m_tick_timer, APP_TIMER_TICKS(TICK_MS, APP_TIMER_PRESCALER), NULL));

    uint32_t done = 0;
    for (;;) {
        app_sched_execute();
        uint32_t target = m_ticks_isr;
        uint32_t behind = target - done;
        if (behind > 1) {
            g_diag.missed_ticks += behind - 1;
        }
        /* Never spiral: if ticks run long, drop the oldest instead of chasing
         * them forever. The core works on timestamps, so skipped ticks only
         * cost smoothness, not correctness. */
        if (behind > MAX_CATCH_UP) {
            done = target - MAX_CATCH_UP;
        }
        while (done != target) {
            done++;
            uint32_t t0 = app_timer_cnt_get();
            swet_tick(done * TICK_MS);
            measure(t0);
            if (done % 5 == 0) {
                g_diag.stack_free = stack_free_bytes();
            }
            /* Each finished tick is progress. Feeding only after catching up
             * starved the watchdog whenever ticks took ~20 ms (M1 on hardware:
             * reset → power latch released → display off after 2 s). A core
             * stuck inside swet_tick() still trips it. */
            nrf_drv_wdt_channel_feed(m_wdt);
        }
        (void)sd_app_evt_wait();
    }
}

/* Any SDK error: reset. Panics in Rust end up in hal_panic() the same way. */
void app_error_fault_handler(uint32_t id, uint32_t pc, uint32_t info)
{
    (void)id;
    (void)pc;
    (void)info;
    hal_panic();
}
