/*
 * Swet102 — restart into the bootloader's BLE DFU mode (TECH_DESIGN §9.5).
 *
 * casainho's bootloader (github.com/geeksville/SW102_LCD_Bluetooth-bootloader,
 * nRF5 SDK 12) enters DFU when PWR is held (M not) for 5 s from its start, or
 * when `enter_buttonless_dfu` is set in its settings page. It asserts the
 * power latch only after deciding, and a reset releases ours: without PWR held
 * the board loses power in between. On hardware the settings-flag route did
 * exactly that (display off, flag consumed, normal boot next time), and a
 * power cut during the bootloader's own write of that page could leave the
 * app marked invalid.
 *
 * So the core resets only while the rider holds PWR (Update screen), and the
 * bootloader's 5 s button path does the rest. Nothing here writes flash.
 * Released under the GPL License, Version 3.
 */
#include "swet_hal.h"

#include "nrf_nvic.h"

void hal_reboot_to_dfu(void)
{
    (void)sd_nvic_SystemReset();
    for (;;) {
    }
}
