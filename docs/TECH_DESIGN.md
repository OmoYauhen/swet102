# Swet102 — Technical Design

Status: draft v0.2 · 2026-10-04 · implements [`PRODUCT.md`](PRODUCT.md)

This document describes **how** Swet102 is built. The product description says
**what** it does; when the two disagree, the product description wins and this
document gets fixed.

Facts about the hardware and the reference firmware come from Swang Stodva
(`bafang-uart-sw102-firmware`, referred to below as **SS**) and are cited as `SS:file:line`.

---

## 1. Principles

1. **Rust core, thin C platform.** All behavior lives in a `no_std` Rust crate,
   `swet-heart`: UI, gestures, motor protocol, trips, lock, power policy, storage
   layout, BLE payloads. The C side is limited to what the Nordic SDK forces on
   us: startup, S130, FDS, app_timer, UART/SPI drivers and the main loop.
   - An all-Rust firmware isn't possible: BLE on the nRF51 needs the S130
     SoftDevice, which is only usable through the C SDK, and `nrf-softdevice` and
     Embassy support nRF52 only.
2. **The HAL is a trait.** The core is generic over `trait Hal`. The firmware
   implements it by calling C functions over FFI. The emulator implements it in
   plain Rust, with no FFI at all.
3. **Single-threaded and tick-driven.** The core runs only from the main loop, in one
   `tick()` every 20 ms. Interrupts only move bytes or set flags. SS runs
   `rt_processing()` inside the timer ISR (`SS:main.c:273-287`); Swet102 doesn't.
4. **Constants, not settings.** Everything the product hardcodes is a `const` in
   `swet_heart::config`. No runtime options.
5. **Small and predictable on a Cortex-M0:**
   - no heap, no `alloc`
   - no floats (`clippy::float_arithmetic` denied)
   - no `core::fmt` in the firmware image (checked in CI, §13)
   - `panic = "abort"`
6. **Testable without hardware.** Every feature can be exercised by an emulator
   test on a virtual clock. Every hardware-only assumption is listed in §16.

---

## 2. Repository layout

```
swet102/
├── Cargo.toml               # workspace
├── rust-toolchain.toml      # pinned stable + thumbv6m-none-eabi
├── crates/
│   ├── swet-heart/          # no_std, no unsafe: all logic
│   │   └── src/
│   │       ├── lib.rs       # App<H: Hal>, init(), tick()
│   │       ├── hal.rs       # trait Hal + small value types
│   │       ├── config.rs    # all product constants (wheel, limits, timings…)
│   │       ├── input.rs     # debounce + gesture engine
│   │       ├── motor/       # Bafang protocol: schedule, framing, codec, link state
│   │       ├── ride.rs      # speed, power, odo/trip/battery-trip/ride counters
│   │       ├── lock.rs      # lock state, PIN check → speed limit
│   │       ├── power.rs     # auto-off, power-off sequence
│   │       ├── store.rs     # persistent record, encode/decode, save policy, migration
│   │       ├── blep.rs      # BLE payloads (telemetry, commands, control)
│   │       ├── ui/          # screen stack, screens, widgets, anim
│   │       └── gfx/         # landscape 1-bpp framebuffer, primitives, fonts, assets
│   ├── swet-fw/             # no_std staticlib for thumbv6m: impl Hal via extern "C", exports swet_*
│   ├── swet-sim/            # std: SimHal, virtual clock, fake motor/BLE/storage, test helpers
│   └── swet-emu/            # desktop window binary (minifb)
├── platform/nrf51/          # C: main.c, hal_*.c, ble.c, SDK config, linker script
├── assets/                  # XBM icons, TTF sources
├── tools/                   # font/asset generator (emits .rs), flashing helpers
├── prebuilt/                # casainho bootloader hex, S130 hex, DFU signing key (from SS)
├── docs/
├── Makefile                 # firmware: cargo build swet-fw → arm-gcc link with SDK
├── version.mk
└── flake.nix
```

---

## 3. Architecture

```
┌──────────── C: platform/nrf51 ─────────────┐        ┌────────────── Rust ──────────────┐
│ main(), startup, linker script             │        │ swet-heart (no_std, no unsafe)   │
│ SDK 12.3: S130, FDS, app_timer, UART, SPI  │        │   App<H: Hal>: input → screens   │
│ hal_*.c  implements the C HAL functions ───┼─ FFI ─►│   → gfx; motor; ride; lock;      │
│ main loop calls swet_init() / swet_tick() ─┼───────►│   store; blep                    │
└────────────────────────────────────────────┘        │ swet-fw (staticlib): FwHal:      │
                                                      │   impl Hal by extern "C" calls   │
                                                      └──────────────────────────────────┘
        emulator / tests:  swet-heart + SimHal (pure Rust, no FFI, many App instances)
```

### 3.1 The HAL trait (`swet-heart/src/hal.rs`)

```rust
pub trait Hal {
    // display: 64 rows × 16 bytes, see §6
    fn display_flush(&mut self, fb: &Frame);
    fn display_contrast(&mut self, level: u8);

    // buttons, polarity already handled
    fn buttons(&mut self) -> Buttons;                  // bitflags LEFT|RIGHT|M|PWR

    // motor UART, 1200 8N1
    fn uart_write(&mut self, bytes: &[u8]);
    fn uart_read(&mut self) -> Option<u8>;
    fn uart_take_errors(&mut self) -> u8;

    // persistent storage: one small blob, async save
    fn store_load(&mut self, buf: &mut [u8; STORE_LEN]) -> bool;
    fn store_save(&mut self, buf: &[u8; STORE_LEN]);
    fn store_busy(&self) -> bool;

    // BLE
    fn ble_state(&self) -> BleState;                   // connected, telemetry_sub, command_sub
    fn ble_notify(&mut self, ch: BleChannel, data: &[u8]);
    fn ble_address(&self) -> [u8; 6];

    // system
    fn power_off(&mut self);                           // never returns on HW
    fn reboot_to_dfu(&mut self);                       // never returns on HW
}
```

**Time is passed into `tick(now_ms)`**, not read through the HAL. The watchdog is
fed by the C main loop, because it isn't the core's concern.

### 3.2 The FFI boundary

These are C functions that Rust calls. They're declared in
`platform/nrf51/swet_hal.h` and mirrored by `extern "C"` in `swet-fw`:

```c
void     hal_display_flush(const uint8_t *fb);           // 1024 B
void     hal_display_contrast(uint8_t level);
uint8_t  hal_buttons(void);                               // bit0 LEFT, bit1 RIGHT, bit2 M, bit3 PWR
void     hal_uart_write(const uint8_t *buf, uint8_t len);
int16_t  hal_uart_read(void);                             // -1 = empty
uint8_t  hal_uart_take_errors(void);
bool     hal_store_load(uint8_t *buf, uint16_t len);
void     hal_store_save(const uint8_t *buf, uint16_t len);
bool     hal_store_busy(void);
uint8_t  hal_ble_state(void);                             // bit0 conn, bit1 tel sub, bit2 cmd sub
void     hal_ble_notify(uint8_t ch, const uint8_t *buf, uint8_t len);
void     hal_ble_address(uint8_t out[6]);
void     hal_power_off(void);
void     hal_reboot_to_dfu(void);
void     hal_panic(void);                                 // reset; called by Rust's panic handler
```

These are Rust functions that C calls, exported by `swet-fw` with `#[no_mangle]`:

```c
void swet_init(uint32_t now_ms);
void swet_tick(uint32_t now_ms);                         // every 20 ms, main loop only
void swet_ble_control(const uint8_t *data, uint8_t len); // phone wrote the control char
```

**Rules for the boundary:**
- Only fixed-width integers, `bool` and pointer + length cross it. No enums or structs.
- `swet-fw` holds the single `App<FwHal>` in a `static`. It's only touched from
  the three exported functions, which run only in the main loop, and a debug
  re-entrancy flag asserts that.
- **Unsafe code lives only in `swet-fw`:** the FFI calls, the static app and the
  panic handler. `swet-heart` is `#![forbid(unsafe_code)]`.
- **Panic handler:** `hal_panic()`, which resets the chip. Panics are bugs; the
  watchdog and the reset are the safety net.

### 3.3 Build info

`VERSION_STRING`, `VERSION_NUM`, `GIT_HASH`, `BUILD_DATE`, `PIN_CITY` and
`PIN_SPORT` come from environment variables that `swet-heart`'s `build.rs` turns into
consts (`env!` style). The firmware build passes the real values. The emulator and
tests get dev values (§12).

---

## 4. Execution model

### 4.1 nRF51 main loop (C)

```c
int main(void) {
    platform_init();            // clocks, SoftDevice + app_scheduler, GPIO, power latch,
                                // SPI+LCD, UART, app_timer, BLE, FDS
    swet_init(0);
    watchdog_start(2000);
    for (;;) {
        app_sched_execute();    // SoftDevice / FDS / BLE events → main-loop context
        while (ticks_done != ticks_isr) {   // catch up on missed 20 ms ticks
            swet_tick(++ticks_done * 20);   // core flushes the display via the HAL when dirty
        }
        hal_watchdog_feed();
        sd_app_evt_wait();
    }
}
```

- **Tick source:** an `app_timer` at 20 ms. Its handler only increments `ticks_isr`.
- **SoftDevice events** are routed through `app_scheduler`
  (`SOFTDEVICE_HANDLER_APPSH_INIT`), so BLE and FDS callbacks run in the main loop.
  A control-characteristic write calls `swet_ble_control()` from there.
- **UART RX ISR** pushes bytes into the SDK FIFO, and `hal_uart_read()` pops them.
- **Watchdog:** 2 s, fed once per loop, paused while halted by the debugger. SS has it
  disabled (`SS:main.c:189`).
- **Faults:** HardFault and `app_error_fault_handler` reset the chip. A stack canary
  is checked every 100 ms, as in SS (`SS:main.c:193-223`).

### 4.2 Per-tick order in `App::tick()`

1. `input.poll()`: sample buttons, debounce, emit gesture events.
2. `motor.step()`: drain RX, parse replies, handle timeouts, send the next request or a queued write.
3. `ride.step()`: integrate distance, update speed, power and SoC, check the battery-trip rule.
4. Deliver events to the popup layer, then to the top screen (§5.1).
5. `power.step()`, `store.step()`, `blep.step()` (telemetry 1 Hz).
6. If anything changed or an animation is running, render into the frame and call `hal.display_flush()`.

### 4.3 Timing budget (Cortex-M0 @ 16 MHz)

| Work | Cost (estimate) |
|---|---|
| Full-frame render | SS: **13.8 ms avg, 18.7 ms max** (probe) — Swet102 to be measured on the M0/M1 diag screen |
| SPI flush of 1 KB, blocking via `nrf_drv_spi` (SS code) | SS: **8.4 ms avg, 9.2 ms max** (probe) |
| Everything else | < 1 ms |
| **SS total per 20 ms tick** | **27 ms max; 79 % of frames over 20 ms; 634 ticks missed in 64 s** |

The probe shows SS does **not** hold 50 fps: the flush alone eats 42 % of a tick.
The 1 KB itself is only ~2 ms at 4 MHz; the rest is per-byte driver overhead on
the nRF51 SPI (no DMA) plus 64 separate column-address commands. Plan:

1. **Flush only when the frame changed** (core compares with the last sent frame,
   ~0.1 ms). A still riding screen then costs no SPI time at all.
2. **Register-level SPI** in `lcd.c`: feed `NRF_SPI0->TXD` with its double buffer
   and poll `EVENTS_READY`, instead of one `nrf_drv_spi_transfer` per row. Target ≤ 3 ms.
3. Keep the render small: byte-aligned blits for fonts and fills instead of
   per-pixel loops. Measure on the diag screen before optimising further.

### 4.4 Nothing in a tick waits

`tick()` never blocks on I/O. In particular, the 50–60 ms a motor exchange spends
on the wire is **not** CPU time:

- **TX:** `uart_write()` copies 2–5 bytes into the TX FIFO and returns. The UART
  shifts them out at 1200 baud, and the TX interrupt refills it (µs per byte).
- **RX:** the RX interrupt pushes each byte into a FIFO (µs per byte). Each tick,
  `motor.step()` drains whatever has arrived and returns.
- The motor module is a state machine (`Bus::Idle` / `Bus::Waiting { op, need, got, deadline }`).
  A reply is decoded on the tick where its last byte has arrived.

```
ms:     0        20       40       60       80       100
tick:   │T0      │T1      │T2      │T3      │T4      │T5
wire:   [11 20]────[ctrl thinks]──[hi][lo][ck]
CPU:    send(µs) render  render    render    decode   send next
```

The only things that can stretch a tick past 20 ms are rare and short:

| Source | Duration | Effect |
|---|---|---|
| Flash page erase (FDS GC) | ~20 ms, CPU halted by hardware | one dropped frame, a few times a day, mostly at stops (§8.3) |
| BLE radio events (SoftDevice, top priority) | 0.1–1 ms | none visible |

**Late ticks are harmless by design:**

1. **Catch-up:** the timer ISR keeps counting. The main loop runs `tick()` once per
   missed tick, each with its own timestamp (§4.1), so gesture timings stay exact.
2. **Animations** compute position from `now − t0`. A late frame jumps to the right
   place: one frame is skipped, and the slide still ends on time.
3. **Distance and moving time** integrate over **elapsed time**
   (`now − last_now`) on every tick, never per tick or per slot count (§7.4).

---

## 5. UI framework

### 5.1 Screen stack

```rust
enum Screen { Boot(BootState), Ride, Pin(PinState), Menu(MenuState),
              Detail(DetailKind), Confirm(ConfirmKind) }
enum Popup  { Error(ErrorCode), BatteryTrip { km_x10: u16 }, Toast(ToastKind) }

struct Ui { stack: heapless::Vec<Screen, 4>, popup: Option<Popup>, … }
```

- Screens are **enum variants with their own state**, dispatched with `match`.
  There are no function-pointer tables, and the compiler checks that every screen
  handles every event kind.
- **Base screen** is `Boot`, then either `Ride` or `Pin` (when `locked`). `Menu` is
  pushed over `Ride`; `Detail` and `Confirm` are pushed over `Menu`.
- **Popups** are drawn over everything and take all input while shown. Priority: error > battery-trip message.
- **Error popup:** M dismisses it. It comes back after 10 s if the condition is
  still present, or straight away on a new code.
- **Battery-trip popup:** any button dismisses it.
- **PWR hold** (power off) is handled before screen dispatch, so it works everywhere.

### 5.2 Ride screen composition

| Widget | Area (x, y, w, h) | Notes |
|---|---|---|
| Tile | 0, 0, 26, 64 | White rounded rect (r = 4), black glyph centered |
| Pane | 28, 0, 86, 64 | One of 6 views; label + value |
| Battery | 116, 0, 12, 64 | Icon (fill = SoC), XOR bolt in sport, `%` below |

These pixel splits are a starting point to tune in the emulator.

```rust
#[derive(Clone, Copy)] enum Page { Pas, Lights, Player, Gate }      // ring = declaration order
#[derive(Clone, Copy)] enum View { Speed, Power, Trip, BattTrip, Ride, Odo }

impl Page {
    fn gestures(self) -> GestureCfg { … }   // Player: RIGHT double + LEFT/RIGHT hold
    fn needs_ble(self) -> bool { matches!(self, Page::Player | Page::Gate) }
}
```

Pages that need BLE draw a dithered glyph and ignore LEFT/RIGHT unless the phone
is subscribed to the command characteristic.

### 5.3 Input and gestures (`input.rs`)

- **Sampling:** every 20 ms tick.
- **Debounce:** a state change counts after 2 identical samples (40 ms). SS has
  no debounce (`SS:buttons.c`).
- **Boot lockout:** the PWR press that powered the board on is still held when the
  firmware starts. All buttons are ignored until every button has been released once.
- **Events per button:** `Down`, `Up`, `Click`, `Double`, `Hold` (fires once at the
  hold threshold while still pressed), `HoldEnd`.
- **Per-button config** comes from the top screen or page via `GestureCfg`:
  - **Double-click off:** `Click` fires on release, so there's no lag.
  - **Double-click on:** after the release, wait up to `DBL_MS` (350) for a second
    press. A second press emits `Double` right away. A timeout emits `Click`.
  - **Hold:** `Hold` fires at `HOLD_MS` (1000), and that press then emits no `Click`.
- **Global:** PWR `Hold` means power off.
- **Constants** live in `config.rs`.

### 5.4 Animations (`ui/anim.rs`)

```rust
struct Tween { t0: u32, dur: u16, from: i16, to: i16 }
impl Tween {
    fn value(&self, now: u32) -> i16;   // ease-out 1-(1-p)^2 in Q8 fixed point
    fn done(&self, now: u32) -> bool;
    fn snap(&mut self);                  // jump to the end on input
}
```

- **Page and pane slides:** render the old and new content with an offset,
  clipped to the widget rect. Both are rendered **live** each frame.
- **Input during an animation:** any input event first calls `snap()` on every
  active tween, then gets handled. Nothing queues.
- **Boot:** the sparkles show for 600 ms, then `SWET102 v<version>` scrolls at
  ~2 px/frame until it's off screen. After that it waits for motor link-up. Any
  button skips ahead.

### 5.5 Event routing

"Which page or menu is active" is not a separate variable. It's the top of the
screen stack, plus `RideScreen.page` inside the ride screen. Every gesture event
goes through one fixed chain, and the first handler that matches consumes it:

| Step | Handler | Takes |
|---|---|---|
| 1 | **Global** | PWR `Hold` → power off (any screen); PWR `Double` → lock + off (ride screen only) |
| 2 | **Animations** | nothing; every input first calls `snap()` on running tweens (§5.4) |
| 3 | **Popup**, if shown | everything: M dismisses an error, any button dismisses the battery-trip message |
| 4 | **Top of stack** | `Boot`: any button skips · `Pin`: L/R digit, M next, PWR backspace · `Menu`: L/R item, M enter, PWR esc · `Detail`: PWR back, L/R page · `Confirm`: M yes, PWR cancel |
| 5 | **Ride → current page** | the ride screen keeps M (page ring, pane ring, menu) and PWR click (go to PAS); **LEFT/RIGHT go to `page.on_event()`** (PAS, Lights, Player, Gate) |

```rust
fn dispatch(&mut self, ev: Event, now: u32) {
    if let Some(done) = self.global(ev) { return done; }          // 1
    self.ui.snap_tweens();                                        // 2
    if let Some(p) = &mut self.ui.popup { return p.on_event(ev, &mut self.cx); }  // 3
    match self.ui.stack.top_mut() { /* 4: one arm per Screen variant */ }
}
```

**Navigation:**
- M hold pushes `Menu`. M on an item pushes `Detail` or `Confirm`. PWR pops one
  level. Popping `Menu` returns to `Ride` with its page and view unchanged,
  because they live inside `Ride`.
- At boot: `[Boot]` → `[Ride]` or `[Pin]`. A correct PIN replaces `Pin` with `Ride`.
- Popups live in their own slot, not on the stack, so they can appear over any screen.

**Gesture config follows the same lookup.** The input engine needs to know whether a
button has double-click or hold before it can classify a press. `App::gesture_cfg()`
asks the same chain:
- a popup, menu, detail, confirm, PIN or boot screen gives `SIMPLE` (instant clicks only)
- the ride screen gives M and PWR double + hold, plus LEFT/RIGHT from `page.gesture_cfg()`, e.g. RIGHT double only on Player

The config is **latched when a button goes down** and kept until that press
finishes. If the screen changes mid-gesture (M hold opens the menu while M is still
held), the release isn't reinterpreted under the new screen's rules, and no stray
click reaches the menu.

---

## 6. Graphics (`gfx/`)

### 6.1 Framebuffer and orientation

SS's framebuffer is `u8[x*16 + y/8]`, bit `y & 7`, with x from 0 to 63 and y from
0 to 127 in portrait (`SS:lcd.h:17-27`). Seen in landscape, that is exactly **64
rows × 16 bytes, 8 horizontal pixels per byte, LSB = leftmost**:

```rust
pub struct Frame(pub [[u8; 16]; 64]);   // frame.0[y][x >> 3], bit (x & 7); 1 = lit
```

- **Flush** (C, `hal_display_flush`) works as in SS: for each of the 64 controller
  columns, send the column address, then that row's 16 bytes (`SS:lcd.c:116-140`).
  No rotation is done in software.
- **Rotation direction:** which physical edge counts as "top" is set with the SH1107
  segment-remap and COM-scan-direction commands (`0xA0/0xA1`, `0xC0/0xC8`). Verify on hardware (§16).
- **Pixel format:** this is also the native **XBM** bit order, so assets drawn
  upright load directly. SS's transpose trick (`SS:gfx.h:9-14`) is no longer needed.
- **Polarity:** 1 = lit. SS uses 0 = lit (`SS:gfx.c:55`).

### 6.2 Primitives

This is a small module of our own, not `embedded-graphics`. We need XOR, clip
stacks and per-row byte blits, and the generic version costs more flash than it saves.

- `clear`, `pixel`, `hline`, `vline`, `fill_rect(mode)`
- `round_rect(mode, r)`, `dither_rect`
- `blit(&Image, x, y, mode)`
- `text(&Font, &str, x, y, mode)`, `text_width`
- `with_clip(rect, |g| …)`

Modes are `Set`, `Clear` and `Xor`.

**Numbers are drawn without `core::fmt`:** a small `u32 → [u8; 10]` digit
formatter, plus a decimal-point helper for `x_10` values.

### 6.3 Fonts and assets

- `tools/gen-assets` turns XBM icons and TTF fonts into `.rs` files with `const`
  byte arrays and glyph tables. It's a Rust `xtask`, or a port of SS's
  `ttf2font.py`. The generated files are committed, so the firmware build needs no generator.
- **Fonts:**
  - speed: 04B_30, 34 px, digits only
  - labels: ~7 px, uppercase
  - tile number: 04B_30, ~40 px
  - popup/menu text: ~8–10 px
- **Icons:** sparkles (from SS), bulb, music note, key, up arrow, padlock, circular
  arrow, Bluetooth, wrench, "i", download arrow, "!".

---

## 7. Motor protocol (`motor/`)

The display is the bus master, at 1200 baud 8N1 (~8.3 ms per byte). These facts
come from SS (`SS:state.c`, `SS:uart.c`).

### 7.1 Read schedule

There is one request per **100 ms slot** (5 ticks):

```
slot:   0      1        2      3        4      5
       SPEED  CURRENT  SPEED  BATTERY  SPEED  STATUS      (repeat)
```

That gives speed every 200 ms and the rest every 600 ms. MOVING (0x31) and BRAKE
(0x0F) aren't polled: "moving" means speed > 0, and the product doesn't show brake.

| Op | Request | Reply bytes | Check | Decode |
|---|---|---|---|---|
| 0x20 SPEED | `11 20` | 3 | `(hi+lo+0x20) & 0xFF` | wheel rpm = hi:lo |
| 0x0A CURRENT | `11 0A` | 2 | b1 == b0 | current units as decoded in SS |
| 0x11 BATTERY | `11 11` | 2 | b1 == b0 | SoC % |
| 0x08 STATUS | `11 08` | 1 | none | 0x01 normal, 0x03 braking, anything else = error code |

- **Codec:** pure functions (`encode_*`, `decode_*`) with unit tests, separate from
  the scheduler state machine.
- **Framing:** after sending a request, expect N bytes. Stray bytes are dropped. A
  bad checksum drops the reply and counts an error. The reply timeout is the end of the slot.
- **Diagnostics:** per-opcode OK, timeout and bad-checksum counters, plus the UART
  line-error count, for the diagnostics screen.

### 7.2 Writes

Writes take the next slot ahead of the scheduled read, coalesced (only the latest value of each kind is sent).

| Write | Frame | When |
|---|---|---|
| PAS | `16 0B code sum` | on change; codes for levels 0–9: `00 01 0B 0C 0D 02 15 16 17 03`, walk = `06` (`SS:state.c:75-87`) |
| Lights | `16 1A F1/F0` | on change |
| Speed limit | `16 1F hi lo sum` | at link-up, when the stored limit changes, at re-link |

```rust
const fn speed_limit_wire(kmh: u32) -> u16 {
    if config::SPEED_LIMIT_AS_RPM {                  // default, pending §16
        (kmh * 1_000_000 / 60 / config::WHEEL_MM) as u16   // 25 km/h → 192
    } else {
        (kmh * 10) as u16                            // SS encoding
    }
}
```

- The value sent is `speed_limit_wire(store.speed_limit)`: 25 after the city PIN, 99 after the sport PIN.
- **Walk assist:** PAS code 06 is sent on `Hold` and the level is restored on `HoldEnd`.
  If the controller times walk assist out, re-send it every 500 ms while held (§16).

### 7.3 Link state

```rust
enum Link { Down, Up { last_ok: u32 } }
```

- **Link up:** the first valid reply.
- **Link lost:** no valid reply for **2 s**. Show the error popup with `--`.
- **On every link-up:** force-send the speed limit, PAS and lights, since the controller may have rebooted.
- **Errors:** a STATUS value other than 01/03 shows `!` plus the code as two hex
  digits. It clears after 3 consecutive normal replies.

### 7.4 Derived values (`ride.rs`)

- **Speed:** `kph_x10 = rpm * WHEEL_MM * 6 / 10000`.
- **Power:** `W = current_A × 52`.
- **Distance:** every tick adds `rpm × WHEEL_MM × (now − last_now)` (using the latest
  decoded rpm) into an accumulator in mm × ms/min. Each whole meter goes to `ride_m`,
  `trip_m`, `trip_batt_m` and `odo_m`. It's the same accumulator idea as
  `SS:state.c:391-399`, but **time-based** instead of per-slot, so late ticks and
  a changed poll schedule can't skew it.

**Trip statistics:** the three trips share one type:

```rust
struct Trip { m: u32, moving_ms: u32, max_x10: u16, charge: Charge }
struct Charge { mah: u32, rem_ma_ms: u32 }        // whole mAh + remainder (< 3.6e6 mA·ms)

impl Trip {
    fn avg_x10(&self) -> u16 {                     // km/h × 10 = m / s × 36
        if self.moving_ms < 1000 { 0 } else { (self.m as u64 * 36_000 / self.moving_ms as u64) as u16 }
    }
}
```

- **Moving time:** `+= now − last_now` on every tick where the latest rpm > 0.
- **Max:** when two consecutive speed readings differ by ≤ 5 km/h, `max = max(max, min(a, b))`. One corrupt sample can't set a record.
- **Charge (Ah):** every tick adds `current_ma × (now − last_now)` (latest decoded current) to `rem_ma_ms`; each full 3 600 000 mA·ms moves 1 mAh into `mah`. Integer only; nothing is lost between ticks. Current comes from CURRENT (0x0A) every 600 ms, so this is an estimate. Its scale gets confirmed by the probe (§16).
- **Odometer max:** `odo_max_x10` uses the same two-sample rule and never resets.
- **Instances:** `trip` and `trip_batt` are persisted, with moving time in whole seconds and charge in whole mAh. `ride` lives in RAM only.
- **Resets:** a reset clears every field of that trip.

**Battery trip:**
- `soc_min = min(soc_min, soc)`.
- If `soc >= soc_min + 10`, then:
  1. Capture the km value.
  2. Reset the battery `Trip` (distance, moving time, max) and set `soc_min = soc`.
  3. Save immediately.
  4. Show the popup "Trip on last battery charge was X km!".
- The check runs only after link-up, with at least 3 consistent SoC readings.

---

## 8. Persistence (`store.rs`)

### 8.1 Record

The record is encoded **explicitly**, little-endian, into a `[u8; 48]`. There's no
`#[repr(C)]` transmute, so no `unsafe` and no layout surprises.

| Offset | Field | Type |
|---|---|---|
| 0 | version | u8 |
| 1 | pas | u8 (0–9) |
| 2 | speed_limit | u8, km/h (25 = city, > 25 = sport) |
| 3 | locked | u8 |
| 4 | soc_min | u8 |
| 5..8 | reserved | — |
| 8 | odo_m | u32 |
| 12 | trip_m | u32 |
| 16 | trip_moving_s | u32 |
| 20 | trip_batt_m | u32 |
| 24 | trip_batt_moving_s | u32 |
| 28 | trip_max_x10 | u16 |
| 30 | trip_batt_max_x10 | u16 |
| 32 | trip_mah | u32 |
| 36 | trip_batt_mah | u32 |
| 40 | odo_max_x10 | u16 |
| 42 | reserved | u16 |
| 44 | reserved | u32 |

**No mode field:** the mode is derived. `fn is_sport(&self) -> bool { self.speed_limit > 25 }`.

**Change from the product description:** distances are stored in **meters**
instead of 100 m units. It's the same size, and short rides don't lose up to 99 m
at every power-off. The product doc is updated to match.

**Defaults:** `pas = 0`, `speed_limit = 25`, `locked = 0`, counters and stats 0.

**Migration:** `decode()` matches on `version`. Known old versions keep at least
`odo_m`; anything unknown falls back to defaults. SS's EEPROM content isn't migrated: the odometer starts at 0 on Swet102 (accepted by the owner).

### 8.2 Platform: FDS (C)

- **Storage engine:** SDK FDS, with 3 virtual pages of 1 KB directly below the
  bootloader (0x37C00–0x3ABFF; the bootloader starts at 0x3AC00). One file/key holds one record, with `FDS_CRC_ENABLED`.
- **The linker script reserves those pages.** SS's doesn't (`SS:gcc_nrf51.ld:8-12`).
- **No Peer Manager** (no bonding).
- **Saves are asynchronous:** `fds_record_update`, with GC only when FDS reports no
  space. SS runs GC before every write (`SS:eeprom_hw.c:103-148`).

### 8.3 Save policy

| Trigger | What |
|---|---|
| `pas`/`speed_limit`/`locked` changed | save 3 s after the last change |
| distances | save when stopped (speed 0 for 5 s) and ≥ 100 m changed since the last save; also every 1 km while riding |
| manual trip reset, battery-trip reset | save immediately |
| power-off, lock | save, **wait until `store_busy()` is false** (timeout 500 ms), then `power_off()` |

- SS powers off without waiting for the write (`SS:main.c:64-81` FIXME). Swet102 waits.
- **Abrupt power loss** loses at most ~1 km and the last 3 s of setting changes. That's accepted.
- **Wear:** a 48-byte record plus its FDS header is about 60 bytes, so roughly 30
  writes fit between GCs (one of the 3 pages is the GC swap page). At 200 saves a
  day, that's about 7 GCs a day spread over 3 pages: around 5 erases per page
  per day, against a rated 20k cycles. More than 10 years.

---

## 9. BLE

The C side (`platform/nrf51/ble.c`) owns the stack. The Rust side
(`blep.rs`) owns the payloads.

### 9.1 Stack

- S130 2.0.1, SoftDevice API v2, LF clock from the RC oscillator (`SS:custom_board.h:81-84`).
- One peripheral link. One vendor UUID base. Services: custom + DIS.
- No Peer Manager, no bonding, `SEC_OPEN`.
- **Address:** the default static random address from FICR, never overridden.

### 9.2 Advertising

| | Value |
|---|---|
| Advertising data | flags (general discoverable, LE only) + complete name `swet102` |
| Scan response | 128-bit custom service UUID |
| Fast | 100 ms for 30 s after boot or a disconnect |
| Slow | 1 s, **no timeout** (`BLE_GAP_ADV_TIMEOUT_GENERAL_UNLIMITED`) |
| After disconnect | restart fast at once |

SS advertises fast only, times out after 180 s and never restarts (`SS:ble_services.c:42-46, 754-776`).

### 9.3 Connection parameters

- Preferred: 50–100 ms interval, latency 0, supervision timeout 4 s.
- Ask for an update once, 5 s after connecting.
- **Never disconnect** when the phone rejects the parameters. SS does (`SS:ble_services.c:532-574`).

### 9.4 GATT

Base UUID: `8f03xxxx-da4c-453d-a163-41a592a0e9fd`

| Attribute | UUID | Properties |
|---|---|---|
| Swet102 service | `8f030001-…` | |
| Telemetry | `8f030002-…` | read, notify |
| Command | `8f030003-…` | notify |
| Control | `8f030004-…` | write (§9.5) |
| Trips | `8f030005-…` | read, notify |
| Device Information | 0x180A | firmware rev = `VERSION_STRING`, model `SW102`, manufacturer `PET` |

**Telemetry**: 14 bytes, little-endian, notified at 1 Hz while subscribed.

| Offset | Field | Type | Notes |
|---|---|---|---|
| 0 | version | u8 | 2 |
| 1 | speed_x10 | u16 | km/h × 10 |
| 3 | power_w | u16 | |
| 5 | soc | u8 | % |
| 6 | pas | u8 | 0–9 |
| 7 | speed_limit | u8 | km/h; > 25 = sport |
| 8 | flags | u8 | bit0 lights, bit1 walk, bit2 motor link up |
| 9 | error | u8 | 0 = none, else STATUS code; 0xFF = link lost |
| 10 | odo_hm | u32 | 100 m units |

**Trips**: one **18-byte record per notification**, identified by `id`. All four records
are sent one after another every 5 s while subscribed, and the affected one straight
after a reset. Both characteristics fit the default 20-byte ATT payload, so no MTU
exchange is needed (S130 on nRF51 is safest at the default MTU).

| Offset | Field | Type | Notes |
|---|---|---|---|
| 0 | version | u8 | 1 |
| 1 | id | u8 | 0 trip, 1 battery trip, 2 ride, 3 odometer |
| 2 | dist_m | u32 | meters |
| 6 | max_x10 | u16 | km/h × 10; odometer = all-time max |
| 8 | avg_x10 | u16 | km/h × 10; 0 for odometer |
| 10 | mah | u32 | charge used; 0 for odometer |
| 14 | moving_s | u32 | 0 for odometer |

**Command**: `[seq, code]`, notified once per press. `seq` increments per command, so the app can drop duplicates.

| Code | Command |
|---|---|
| 0x01 | volume + |
| 0x02 | volume − |
| 0x03 | next track |
| 0x04 | previous track |
| 0x05 | play / pause |
| 0x10 | gate A |
| 0x11 | gate B |

### 9.5 DFU entry

SS has **no** way into DFU from the app. Today it's entered with the bootloader's
own button combo, **M + PWR held ~8 s**, and that keeps working as a fallback.

- **Menu "Reboot to DFU":**
  1. Save and wait.
  2. `hal_reboot_to_dfu()`, which does `sd_power_gpregret_set(0xB1)` and then `sd_nvic_SystemReset()`.

  Verify that casainho's bootloader honours GPREGRET (§16). If it doesn't, the menu
  item shows "hold M+PWR 8 s" instead.
- **From the phone:** writing `"DFU!"` to the control characteristic does the same.
  It's only accepted after the wheel has been stopped for 5 s.
- **Security:** unauthenticated, and the DFU signing key is public
  (`prebuilt/private.key`). That's accepted, consistent with product §9.

**After an update** the bootloader keeps advertising `SW102_DFU` until the display
is powered off and started with a long PWR press (soft power latch).

---

## 10. nRF51 platform

### 10.1 Memory map

| Region | Range | Size |
|---|---|---|
| MBR + S130 2.0.1 | 0x00000–0x1AFFF | 108 KB |
| **Swet102 app** (C + Rust) | 0x1B000–0x37BFF | **115 KB** (SS uses ~46 KB) |
| FDS pages (3 × 1 KB) | 0x37C00–0x3ABFF | 3 KB, reserved in the linker script |
| casainho bootloader | 0x3AC00–0x3FBFF | 20 KB (`UICR.BOOTLOADERADDR` = 0x3AC00, probe) |
| bootloader settings | 0x3FC00–0x3FFFF | 1 KB |
| **RAM** (after S130) | 0x20002C00–0x20007FFF | 21 KB of 32 KB (probe: 4 × 8 KB; S130 needs ≥ 0x20001FE8) |

**RAM budget:**
- framebuffer 1 KB
- UART FIFOs 256 B
- `App` state ~1.5 KB
- **stack 4 KB**: Rust at `opt-level = "z"` can use more stack than hand-written
  C, and the stack canary plus a CI stack-usage report (`-Z emit-stack-sizes` or
  `cargo call-stack`) keep it honest
- heap 0

### 10.2 Rust on thumbv6m

| Topic | Approach |
|---|---|
| Target | `thumbv6m-none-eabi` (Cortex-M0, no FPU, no atomic CAS; single-threaded core, so no atomics needed) |
| Profile | `opt-level = "z"`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `debug = true` (debug info stays out of the flash image) |
| Size control | no `core::fmt` (CI checks the map for `core::fmt` symbols); no floats; `cargo bloat` in CI |
| Crates | `heapless` (fixed-size Vec), `bitflags`; nothing else in the firmware unless justified |
| Linking | `cargo build -p swet-fw --target thumbv6m-none-eabi --release` → `libswet_fw.a` → added to the SDK Makefile's `LIB_FILES`; arm-gcc links with `--gc-sections` |
| Builtins | Rust's `compiler_builtins` and newlib/libgcc both define `memcpy`, `__aeabi_*` and friends. The trial (§15) confirms there are no duplicate-symbol or wrong-pick issues. |

### 10.3 C drivers (adapted from SS)

| Area | Source in SS | Change |
|---|---|---|
| SPI + SH1107 init | `lcd.c` | landscape remap commands; fix the contrast overflow (`SS:lcd.c:121`) |
| UART | `app_uart_fifo_mod.c`, `uart.c` | plain FIFO; framing moves into Rust `motor/` |
| Power latch | `main.c` (P0.09) | `hal_power_off()` only drops the latch; the core waits for the store first |
| Buttons | `custom_board.h` pins | raw read only; LEFT/RIGHT/M pull-up active-low, PWR active-high |
| BLE | `ble_services.c` | rewritten: §9; no Peer Manager, no CSC/BAS |
| Storage | `eeprom_hw.c` | async, GC only on demand |

---

## 11. Emulator

### 11.1 Structure

- **`swet-sim`:** `SimHal` implements `Hal` in plain Rust. `Sim` owns
  `App<SimHal>`, the virtual clock, a fake motor, fake storage and a fake BLE.
  - `Sim::tick()` advances 20 ms.
  - **No FFI and no globals**, so any number of `Sim`s can exist at once and
    `cargo test` runs them in parallel.
- **`swet-emu`:** the desktop window. It's a thin shell around `Sim`.
- **Same code as the device:** both builds use exactly the same `swet-heart`
  source; only the `Hal` implementation differs. Dev builds of the core may enable a
  `debug-overlay` feature; firmware builds never do.

### 11.2 Fake motor

- A BBSHD stock-protocol model fed by `uart_write`. Replies are delivered **with
  real 1200-baud timing** (8.33 ms per byte, plus ~10 ms controller latency) on the virtual clock.
- **State:** rpm, current, SoC, status code. It records the last PAS code, lights and speed limit received.
- **Fault injection:** drop the link, corrupt checksums, delay replies, controller reboot.
- **Ride model** (optional): speed follows PAS and the decoded speed limit, so city mode visibly caps at 25.

### 11.3 Desktop window (`swet-emu`)

- **Window:** `minifb`, 128×64 scaled ×6 (768×384), OLED white, with a 1 px pixel gap.
- **Status:** shown in the window title and printed to the terminal: motor, link, BLE, commands and store writes.

| Key | Action |
|---|---|
| ← / → | LEFT / RIGHT |
| ↓ or Space | M |
| P or Esc | PWR |
| W / S | motor speed ± 1 km/h |
| E / D | current ± 1 A |
| R / F | SoC ± 5 % |
| 1..9, 0 | inject error code / clear |
| L | toggle motor link |
| B | toggle "phone connected + subscribed" |
| F12 | PNG screenshot |
| F11 | GIF recording |

- **Real hold:** key-down and key-up map straight to press and release.
- **Storage** lives in `emu-store.bin`. `--fresh` starts empty.
- **`--motor-port=/dev/ttyUSB0`** talks to the real controller over a USB-UART, as SS does (`SS:serial.rs`).

### 11.4 Tests

```rust
#[test]
fn m_double_click_switches_pane_without_switching_page() {
    let mut s = Sim::new().motor_speed_kmh(27).boot_to_ride();
    s.double_click(Btn::M);
    s.run_ms(300);
    assert_eq!(s.app().page(), Page::Pas);
    s.assert_frame("pane_power");            // golden PNG in crates/swet-sim/golden/
}
```

- **Unit tests** live next to the code in `swet-heart` (`#[cfg(test)]`, run on the
  host): codec, checksums, gesture engine, tween maths, store encode/decode/migration.
- **Scenario tests** live in `swet-sim`:
  - helpers: `click`, `double_click`, `hold`, `run_ms`, motor and BLE knobs
  - probes: `app()` (read-only accessors), `store_bytes()`, `uart_log()`, `ble_notifies()`
- **Golden frames** are compared pixel-exactly. `UPDATE_GOLDEN=1` rewrites them, and the diffs get reviewed in PRs.
- **Coverage target:**
  - every gesture row in product §4
  - every menu item
  - lock/unlock with both PINs and a wrong PIN
  - the battery-trip popup
  - link loss and recovery
  - the save policy
  - a mid-animation frame
  - the speed-limit wire value for both modes

---

## 12. Build, versioning, release

- **`nix develop`:** pinned Rust (`rust-toolchain.toml` through fenix or
  rust-overlay, with `thumbv6m-none-eabi`), arm-none-eabi-gcc 15.2, openocd, wlink, srecord and nrfutil 6.1.7.
- **`nix build .#firmware --impure`:** `make` runs `cargo build -p swet-fw` and then
  the SDK link. Output: app hex, full hex and DFU zip.
  - `--impure` is required because the PINs come from the environment.
- **`nix run .#emu`:** the emulator, with dev values.
- **`cargo test`:** all unit and scenario tests, no Nix needed.
- **PINs:**
  - `SWET_PIN_CITY` and `SWET_PIN_SPORT` must each be 4 digits and must differ;
    `swet-heart`'s `build.rs` fails the build otherwise.
  - If unset outside a release build, the dev PINs `1111`/`2222` apply and the
    version gets `-dev` (`SWET102 v0.0.1-dev`).
  - **`make release` refuses dev PINs.**
- **Version:**
  - `VERSION_STRING` is semantic (`0.0.1`).
  - `VERSION_NUM` is the DFU application version, date-based `YYMMDDNN`. The
    resident bootloader has downgrade prevention, and the device is currently at version 200.
- **DFU zip:** `nrfutil pkg generate --application app.hex --key-file prebuilt/private.key --application-version $(VERSION_NUM) --hw-version 51 --sd-req 0x87`, the same as SS.
- **SWD:** OpenOCD over CMSIS-DAP (WCH-LinkE in DAP mode), run inside a privileged
  Docker container on this NixOS host.
  - `flash-full` does a mass erase and writes the bootloader, S130, the app and settings.
  - `flash-app` writes the app plus the settings page. The settings page is required, or the bootloader stays in DFU.
- **Releases:** PIN-bearing images are built locally and **not** published.

---

## 13. Testing and CI

| Layer | How |
|---|---|
| Core logic | `cargo test -p swet-heart` (unit) + `cargo test -p swet-sim` (scenarios, golden frames) |
| Lints | `cargo clippy -- -D warnings` with `float_arithmetic` and `unwrap_used` denied in `swet-heart`; `cargo fmt --check` |
| Firmware build | `nix build .#firmware` with dev PINs |
| Size gates | app ≤ 115 KB; RAM + stack ≤ budget; **no `core::fmt` symbols** in the map; `cargo bloat` top-20 published in the CI log |
| C side | `-Wall -Wextra -Werror` for `platform/nrf51` |
| Hardware | manual checklist per release (§16 items, then a smoke ride) |

**CI:** GitHub Actions on push and PR runs `nix flake check`, which covers all of the above except hardware.

---

## 14. Reuse from Swang Stodva

GPL-3.0 on both sides.

| Reused C (adapted) | Rewritten in Rust |
|---|---|
| SH1107 init sequence, SPI setup | gfx, fonts, assets pipeline |
| UART FIFO driver | motor framing, schedule, codec, link state |
| FDS glue (made async) | UI, screens, gestures, animations |
| SoftDevice init, BLE skeleton | store layout, save policy |
| Linker script (plus the FDS reservation), Makefile skeleton | BLE payloads |
| flake.nix packaging (toolchain, nrfutil) | emulator (now pure Rust, no `cc`) |
| prebuilt bootloader, S130, signing key | |
| sparkles.xbm, 04B_30 TTF (as asset sources) | |

The new GATT service and advertising policy (§9) are C, but written fresh.

---

## 15. Milestones

| # | Milestone | Done when |
|---|---|---|
| M0 | Skeleton + Rust-on-nRF51 trial | workspace, flake, CI green. **Trial:** `swet-fw` linked with the SDK skeleton, flashed; it draws a test pattern through the HAL and survives the watchdog. Record flash, RAM, stack and frame time. Emulator window shows the same pattern. |
| M1 | Ride screen | 3 columns, PAS page, speed/power views, fake motor, gestures; on hardware: landscape orientation correct |
| M2 | Motor I/O on bike | schedule, writes, link loss/error popup, speed-limit stand test (§16) |
| M3 | Persistence and lock | store, save policy, PIN screen, modes, battery icon bolt, power-off/auto-off |
| M4 | Trips and menu | 3 counters, battery-trip popup, menu with all items and confirmations |
| M5 | BLE | advertising policy, GATT, commands from Player/Gate, DFU entry |
| M6 | Polish | boot animation, page/pane slides, PAS roll, GIF capture; first daily-use release |

---

## 16. Risks and hardware checks

Most of these are answered by a **probe build of Swang Stodva**, specified in
[`ss-hw-probe-prompt.md`](ss-hw-probe-prompt.md), which can run before any Swet102 code exists.

| # | Item | Why it matters | How to check |
|---|---|---|---|
| 1 | ~~RAM size~~ | **Resolved by the probe:** 32 KB (4 × 8 KB). The "QFAA = 16 KB" note was wrong. | — |
| 2 | **Rust + SDK link** | duplicate builtins, code size, stack use | M0 trial |
| 3 | **Speed-limit unit** | RPM vs km/h × 10 (product open question 1) | Stand test, both encodings |
| 4 | **GPREGRET DFU entry** in casainho's bootloader | Menu and phone DFU entry depend on it | Write 0xB1 + reset, watch for `SW102_DFU` |
| 5 | **SH1107 landscape remap** | which edge is the top | First flash in M1 |
| 6 | **Walk-assist keep-alive** | Does the stock controller time out PAS 06? | Hold walk for 30 s on the stand |
| 7 | **Error codes** | Which STATUS values stock BBSHD really sends | Unplug the speed sensor and see what STATUS reports |
| 8 | **Auto-connect in the field** | Phone background behavior varies by vendor | Ride with the app on your own phone |
| 9 | **DFU security** | public key + unauthenticated DFU control | Accepted |

---

## 17. Decision log

| Date | Decision |
|---|---|
| 2026-10-04 | **Rust core** (`swet-heart`, `no_std`, no unsafe) + thin C platform for SDK/S130; HAL is a Rust trait, implemented over FFI on hardware and natively in the emulator. Replaces "C main language" from the product interview. |
| 2026-10-04 | Single-threaded 20 ms tick; ISRs only move bytes or set flags; SoftDevice events via app_scheduler |
| 2026-10-04 | Only fixed-width ints, bool and ptr+len cross the FFI; unsafe confined to `swet-fw` |
| 2026-10-04 | No heap, no floats, no `core::fmt` in the firmware; `panic = "abort"` → reset |
| 2026-10-04 | Own small gfx module (not embedded-graphics); framebuffer `[[u8;16];64]` = landscape on SS's controller layout; 1 = lit |
| 2026-10-04 | Motor read schedule SPEED/CURRENT/SPEED/BATTERY/SPEED/STATUS, 100 ms slots; MOVING and BRAKE not polled |
| 2026-10-04 | Link lost after 2 s; re-send limit, PAS and lights at every link-up |
| 2026-10-04 | Store distances in meters, explicit little-endian encoding |
| 2026-10-04 | No import of SS data; odometer starts at 0 on Swet102 |
| 2026-10-04 | Store `speed_limit` instead of a mode; sport = limit > 25 |
| 2026-10-04 | `Trip { m, moving_ms, max_x10, charge }` for trip, battery trip and ride; avg over moving time; max needs two agreeing samples; charge = integrated current (mAh) |
| 2026-10-04 | Odometer all-time max speed (`odo_max_x10`); store record 48 B |
| 2026-10-04 | Motor polling: fixed 100 ms slots (event-driven with `BUS_GAP_MS` considered and deferred) |
| 2026-10-04 | FDS pages reserved in the linker script; saves async; power-off waits for the write |
| 2026-10-04 | No Peer Manager; advertising fast 30 s → slow 1 s forever; never disconnect on conn-param rejection |
| 2026-10-04 | GATT base `8f03xxxx-da4c-453d-a163-41a592a0e9fd`; telemetry 14 B @ 1 Hz; trips = 4 × 18 B records (trip, battery, ride, odo) every 5 s; command `[seq, code]`; control char for DFU |
| 2026-10-04 | DFU entry via GPREGRET 0xB1; phone-triggered only when stopped ≥ 5 s; M+PWR 8 s remains the fallback |
| 2026-10-04 | `VERSION_NUM` = YYMMDDNN (downgrade prevention; device currently at 200) |
| 2026-10-04 | Emulator: minifb window over `swet-sim`; parallel `cargo test` with golden PNG frames |
| 2026-10-04 | Watchdog on (2 s) from day one; stack budget 4 KB |
| 2026-10-04 | tick() never blocks: UART is FIFO + ISR, motor is a state machine; distance and moving time integrate over elapsed time, not slots |
| 2026-10-04 | Event routing: global → snap animations → popup → top of stack → (ride) current page for LEFT/RIGHT; gesture config latched at button-down |
| 2026-10-04 | M0: Rust staticlib + SDK 12.3 link cleanly (no builtin clashes); first image 11.1 KB flash / 1.8 KB RAM + 4 KB stack |
| 2026-10-05 | HW probe: 32 KB RAM; bootloader at 0x3AC00 → app region ends at 0x37C00 (115 KB), FDS 0x37C00–0x3ABFF; SS flush 8.4 ms → flush only on change + register-level SPI planned |
| 2026-10-04 | Reset_Handler jumps straight to `main` (`__START=main`, `__STARTUP_CLEAR_BSS`, `-nostartfiles`): no newlib `_start`/`exit`/stdio in the image |
