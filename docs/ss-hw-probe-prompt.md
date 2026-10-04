# Prompt for the Swang Stodva dev agent: hardware probe build

> Copy everything below the line into a Claude session opened in
> `~/git/bafang-uart-sw102-firmware`.

---

## Goal

Add a **hardware probe** to Swang Stodva: a set of diagnostic screens and test
actions that answer open hardware questions for a new firmware (Swet102) that will
run on this same SW102 + stock BBSHD. The owner will flash the probe build, run the
tests on the bike (on a stand where noted), and photograph the screens.

## Ground rules

- Put everything behind a compile-time flag, `HW_PROBE=1` (Makefile variable → `-DHW_PROBE`). With the flag off, the firmware must build byte-for-byte the same as today, so the normal build stays unaffected.
- With the flag on, add one entry **"HW probe"** at the top level of the existing config menu, opening a list of probe pages. Use the existing UI/font helpers and keep the portrait orientation, except for the display-orientation test (item 9).
- Text only, no graphs. Numbers in hex where noted, otherwise decimal. Every page should fit one screen, or page with UP/DOWN.
- Don't change normal behavior (motor polling, BLE, power) except where a test explicitly asks for it, and then only while that test's page is open.
- **DFU version:** the device has **downgrade prevention** and currently sits at application version **200 or higher**. Build the probe with `VERSION_NUM` set to a date-based number (e.g. `26100501`), or the OTA will be refused. After the DFU, power off and long-press PWR, or the bootloader stays in `SW102_DFU`.
- Work on a branch, follow the repo's commit style, open a PR. Also add `docs/hw-probe.md` explaining each page and the test procedure (the "Procedure" lines below).

## Probe pages

### 1. Chip identity (FICR / UICR)

Show these raw values in hex:

| Register | Address |
|---|---|
| FICR `CODEPAGESIZE` | 0x10000010 |
| FICR `CODESIZE` | 0x10000014 |
| FICR `CLENR0` | 0x10000028 |
| FICR `PPFC` | 0x1000002C |
| FICR `NUMRAMBLOCK` | 0x10000034 |
| FICR `SIZERAMBLOCKS` / `SIZERAMBLOCK[0..3]` | 0x10000038–0x10000044 |
| FICR `CONFIGID` | 0x1000005C |
| FICR `DEVICEID[0..1]` | 0x10000060–0x10000064 |
| FICR `DEVICEADDRTYPE` | 0x100000A0 |
| FICR `DEVICEADDR[0..1]` | 0x100000A4–0x100000A8 |
| FICR `INFO.PART`, `INFO.VARIANT`, `INFO.PACKAGE`, `INFO.RAM`, `INFO.FLASH` | 0x10000100–0x10000110 (may read 0xFFFFFFFF on old silicon; show as-is) |
| UICR `BOOTLOADERADDR` | 0x10001014 |

Also show a computed line: **total RAM (KB)** = NUMRAMBLOCK × block size, and **flash (KB)** = CODEPAGESIZE × CODESIZE / 1024.

*Why:* Notes from an earlier SWD session say the chip is **QFAA**, which normally means 16 KB RAM, but this firmware links for 32 KB and runs. We need the real RAM size.

### 2. RAM map

- Linker symbols: `__data_start__`, `__bss_end__`, `__HeapBase`, `__HeapLimit`, `__StackLimit`, `__StackTop`.
- The **minimum app RAM base** the SoftDevice asks for. Call `sd_ble_enable()` with the current params and report the `app_ram_base` it returns, or log what `softdevice_enable()` reports.
- **Stack high-water mark:** at boot, paint the free stack area with a pattern, e.g. `0xA5A5A5A5`, below the current SP. The page shows how many bytes were ever used. Procedure: ride or use the UI for 5+ minutes with BLE connected, then read it.

### 3. Frame timing

Use TIMER1 at 1 MHz (the M0 has no DWT) to measure each UI tick:

| Measurement | Report |
|---|---|
| `ui_update()` excluding the LCD flush | avg / max µs |
| `lcd_refresh()` (blocking SPI flush) | avg / max µs |
| ticks missed (the existing missed-tick counter, if any) | count |

Reset the stats with UP+DOWN. Procedure: read after 1 minute on the main screen while riding (or with the wheel spinning on a stand).

### 4. Flash (FDS) timing

Add a "Write EEPROM now" action that times `eeprom_write_variables()` end to end: GC + update + wait. Show the duration in ms, the max over all writes, and the FDS stats (`fds_stat`: open/valid/dirty records, words used, freeable words).

### 5. Motor raw values

While this page is open, show the latest **raw** reply bytes for each polled opcode (STATUS 0x08, CURRENT 0x0A, BATTERY 0x11, SPEED 0x20, MOVING 0x31), next to the decoded value the firmware currently derives:

- **CURRENT:** raw byte and the amps the firmware computes. *Why:* confirm the current scale for the power calculation.
- **SPEED:** raw rpm, plus the max rpm seen.
- **STATUS:** a histogram of **every distinct STATUS value seen since boot**, as `value: count`, up to 12 entries. *Why:* we need the real error codes stock BBSHD sends. Procedure: also unplug the speed sensor / brake sensor briefly (bike on a stand) and note new values.

### 6. Speed-limit encoding test (stand test)

A page where:
- the **encoding** is chosen: `km/h × 10` (current behavior) or `wheel RPM`, with RPM = km/h × 1000 / 60 / perimeter_m and perimeter 2165 mm
- the **value in km/h** is chosen: 15, 20, 25, 30
- **M** sends `16 1F hi lo checksum` immediately
- the screen shows the exact bytes sent and the live speed

Procedure: rear wheel on a stand, PAS 9, pedal or throttle up, and note the speed where assist cuts out for each encoding at 20 and 25 km/h. *Why:* bbs-fw's decoder reads this field as wheel RPM, while Swang Stodva sends km/h × 10.

### 7. Walk-assist keep-alive test

A page with two modes, toggled with M:
- **Once:** send PAS code `0x06` once when DOWN is held, as today.
- **Repeat:** re-send `0x06` every 500 ms while DOWN is held.

It shows the seconds since walk assist started and the live speed. Procedure: on a stand, hold DOWN for 30 s in each mode and note whether and when the motor stops.

### 8. DFU entry via GPREGRET

Add an action **"Reboot to DFU (GPREGRET)"**: `sd_power_gpregret_set(0xB1)` (SDK 12 `BOOTLOADER_DFU_START`), then `sd_nvic_SystemReset()`. Procedure: run it and check whether the phone sees `SW102_DFU` advertising. Note the result, and how to get back (power off, then long-press PWR).

Also show the GPREGRET value read at boot, before anything clears it.

### 9. Display landscape orientation test

A page that cycles, with M, through the 4 combinations of SH1107 **segment remap** (`0xA0`/`0xA1`) and **COM scan direction** (`0xC0`/`0xC8`), sending the commands live. For each combination, fill the 1024-byte framebuffer as if it were **landscape `fb[64 rows][16 bytes]`**, row-major, bit 0 = leftmost pixel:

- an arrow pointing **up** with the text `TOP` (any small font, drawn in landscape coordinates)
- a single lit pixel at landscape (0,0) and a 3×3 block at (127,63)
- the combination's name, e.g. `A1 C8`

Procedure: hold the display as mounted on the bars (buttons on the left, PWR on the right) and note which combination shows the arrow pointing up, the text readable, and (0,0) at the top-left. Restore the normal init when leaving the page.

### 10. BLE identity

Show the address from `sd_ble_gap_address_get()`, with its type, and compare it to FICR `DEVICEADDR`. *Why:* the new firmware relies on a static address the phone can remember across reboots.

### 11. Button bounce (optional, low priority)

On this page, sample the 4 button pins at 1 kHz (TIMER1) and count edges per press. Show the max edges seen in one press for each button, to size the debounce window.

## Quick cross-check over SWD (no firmware change needed)

If the owner has the WCH-LinkE connected, page 1 can also be read with OpenOCD
(`init; halt; mdw 0x10000000 0x48; mdw 0x10000100 5; mdw 0x10001014 1; resume`).
Mention this in `docs/hw-probe.md`.

## Deliverables

1. PR on a branch with the probe code behind `HW_PROBE`.
2. `docs/hw-probe.md`: what each page shows and the procedure for each test.
3. `docs/hw-probe-results.md`: an empty results template with one section per page, for the owner to fill in from photos.
4. A built DFU zip with the probe enabled and a date-based version, plus the exact `nrfutil` command used.
