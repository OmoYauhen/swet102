# Swet102 — Product Description

> **Swet102** = **SW102** + **PET** (Personal Electrical Transport).
> Opinionated firmware for the Bafang SW102 handlebar display, built for exactly
> one e-bike: PET.

Status: requirements · 2026-10-04 · technical design: [`TECH_DESIGN.md`](TECH_DESIGN.md)

---

## 1. Vision

Swet102 is a from-scratch replacement firmware for the SW102 display. It targets a
single known bike, so nearly everything that other firmwares make configurable
is **baked in at compile time**. The result should be a display that is quick to
read at a glance, quick to operate with a thumb, and does only what PET needs.

Swang Stodva [https://github.com/OmoYauhen/swang-stodva] is the **reference** for low-level
work: the display driver, the nRF51822 board bring-up, the Bafang UART protocol
and the build/flash tooling. The application logic, UI and state model are **new**.

### Goals

- Show speed, battery, assist and power at a glance, in landscape orientation.
- Switch PAS with one thumb, fast enough for city traffic.
- Anti-theft lock for short stops, using two PINs that also select a riding mode (city/sport).
- Phone integration over BLE (media control, gate opener, telemetry).
- A desktop emulator good enough to build the whole UI without hardware.

### Non-goals

- Supporting other bikes, motors, controller firmwares or display orientations.
- Configuring motor parameters on the device (wheel size, limits and PINs are compile-time).
- bbs-fw specific features (motor temperature, live voltage).
- Building the companion phone app (only its BLE protocol is defined here; see §9).

---

## 2. Target hardware (hardcoded)

| Item | Value |
|---|---|
| Display unit | Bafang SW102 (nRF51822, 128×64 monochrome OLED) |
| Orientation | **Landscape**, 128 wide × 64 high |
| Motor | Bafang **BBSHD** |
| Controller firmware | **Stock Bafang** (standard display protocol only) |
| Battery | **52 V nominal (14S)** |
| Wheel circumference | **2165 mm** |
| Units | Metric (km/h, km) |

Physical layout as seen by the rider:

```
 [LEFT] [RIGHT] [M]  ┌──────── 128 × 64 ────────┐  [PWR]
                     │                          │
                     └──────────────────────────┘ 
```

Mapping from Swang Stodva's portrait orientation: old **DOWN → LEFT**, old **UP → RIGHT**.
Pins (from `custom_board.h`): UP/RIGHT = P0.02, DOWN/LEFT = P0.19, PWR = P0.10, M = P0.14.

### Compile-time parameters

All of these live in one config header. Changing one means rebuilding the firmware.

| Parameter | Value |
|---|---|
| Motor / protocol | BBSHD, stock Bafang UART |
| Wheel circumference | 2165 mm |
| Nominal pack voltage (for watts) | 52 V |
| City speed limit | 25 km/h |
| Sport speed limit | protocol max (99) |
| City PIN / Sport PIN | 4 digits each, passed at build time via Nix build arg / env (`SWET_PIN_CITY`, `SWET_PIN_SPORT`); never stored in the repo. The build fails if they're unset. |
| Auto power-off idle time | 5 min |
| Assist levels | 0–9, plus walk assist |
| M double-click window / hold time | 350 ms / 1 s |

---

## 3. Screen layout

The screen is split into three columns:

```
┌──────────────────────────────────────────┐
│╭─────╮                               ▮   │
││ ███ │                              ▮▮▮  │
││ █5█ │           27                 ▮▮▮  │
││ ███ │          km/h                ▯▯▯  │
│╰─────╯                              78%  │
└──────────────────────────────────────────┘
  ~20%              ~70%               ~10%
  (█ = lit white tile, glyph drawn black)
```

| Column | Width | Content |
|---|---|---|
| 1 — Page | ~20% | The active **page**, drawn as a **filled white tile with rounded corners**. The page glyph is drawn **black** (inverted) on top. LEFT/RIGHT act on this page. |
| 2 — Info pane | ~70% | Riding data. Default: **speed** in a big, fun font. |
| 3 — Battery | ~10% | Battery icon + percentage. |

### 3.1 Pages (column 1)

**M short press** cycles pages in a ring. **PAS is the default page** at boot.

| Page | LEFT | RIGHT | Notes |
|---|---|---|---|
| **PAS** | assist −1 | assist +1 | Shows the current level 0–9. Saved to EEPROM. **Hold LEFT at level 0 → walk assist** for as long as it's held. |
| **Lights** | lights off | lights on | Uses the motor's light output. Screen brightness follows the light state (dimmer when lights are on). |
| **Player** | click: volume −<br>hold: previous track | click: volume +<br>hold: next track<br>double-click: play / pause | Sent over BLE to the companion app. |
| **Gate** | open gate A | open gate B | Short presses, two gates. Sent over BLE to the companion app. |

The page ring order is fixed at compile time. Pages that need BLE are shown as unavailable when no phone is connected.

#### Page tile glyphs

The tile is about 25 px wide and nearly the full 64 px tall. The glyph is black, centered on the white tile.

| Page | Glyph |
|---|---|
| PAS | The **PAS level number** 0–9, large, in the **W95FA** font (Windows 95 style pixel font) |
| PAS, walk assist active | **Up arrow** (↑), replacing the number while LEFT is held |
| Lights | **Light bulb** |
| Player | **Music note** |
| Gate | **Key** |

Icons are 1-bit bitmaps baked into the firmware, sized to fill the tile.
Unavailable BLE pages (no phone connected) draw the icon with a strike-through
or a dithered pattern. The exact look gets settled in the emulator.

### 3.2 Info pane (column 2)

**M double-click** cycles the info pane in a ring:

1. **Speed** (default): **04B_30 digits, 34 px** (reused from Swang Stodva, generated with `tools/ttf2font.py`), km/h. Start with this font and judge it in the emulator; a bigger font can come later.
2. **Power**: watts = motor current × 52 V (an estimate, since stock firmware doesn't report live voltage)
3. **Trip**: manual trip, since the last reset in the menu
4. **Battery trip**: since the last charge (auto-reset, see below)
5. **Ride**: since this power-on (RAM only, starts at 0 on every boot)
6. **Odometer**: total km, plus all-time max speed

Each view shows a short label (e.g. `TRIP`, `BAT`, `RIDE`, `ODO`) so the three distance counters can't be confused.

#### Trip statistics

Each of the three trips (Trip, Battery trip, Ride) tracks **distance, max speed,
average speed and charge used (Ah)**. Its view shows the distance large, with the
stats in a small font underneath:

```
┌──────────────────────────────┐
│ TRIP                         │
│        42.7 km               │
│ MAX 38.4      AVG 21.6       │
│ 7.85 Ah                      │
└──────────────────────────────┘
```

- **Average speed** = distance ÷ **moving time**, where moving time counts only while the wheel turns. Stops at traffic lights don't drag the average down, as on most bike computers.
- **Max speed** ignores implausible jumps: a new max only counts once two consecutive speed readings agree within 5 km/h, so one bad sample can't set a 90 km/h record.
- **Charge used (Ah)** = motor current integrated over time (amp-hours drawn from the battery, as the controller reports it). The current is sampled every 600 ms, so it's a good estimate rather than a lab measurement; it doesn't include the display's own small draw.
- All three stats reset together with their trip (menu reset, battery-charge reset, power-on).
- The **odometer** keeps an **all-time max speed** (same plausibility rule) and shows it under the total. No average and no Ah.

All distances are counted by the display from wheel RPM × 2165 mm, using the same
accumulator approach as Swang Stodva's `rt_calc_odometer()`.

#### Battery trip auto-reset

- EEPROM stores `trip_batt` (km) and `trip_batt_soc_min`, the lowest battery % seen during this battery trip.
- While riding, `trip_batt_soc_min` follows the battery % down.
- When battery % rises to **`trip_batt_soc_min` + 10 % or more**, the pack has been charged: `trip_batt` resets to 0 and `trip_batt_soc_min` is set to the current %.
- The 10 % threshold also ignores small top-ups. That's intended.
- before reset the battery trip, show the message "popup" (same logic as for error) "Trip on last battery charge was X km!". any button to continue

### 3.3 Battery (column 3)

Battery icon plus the % reported by the stock controller. Swang Stodva has shown this value to be stable on PET, so it's used as-is, with no extra filtering or voltage-based estimate.

The **battery icon also shows the riding mode**, so no separate mode badge is needed:

```
  City          Sport
  ▄█▄           ▄█▄
 ┌───┐         ┌───┐
 │   │         │   │      plain outline, fill = charge level   (city)
 │███│         │█⚡│      same, plus a lightning bolt drawn    (sport)
 │███│         │█╱█│      with XOR, so it's visible on both the
 └───┘         └───┘      filled and empty parts
  78%           78%
```

- **City**: plain battery outline, filled from the bottom up to the charge level.
- **Sport**: the same icon with a **lightning bolt** drawn in XOR over the body. A filled area shows a black bolt and an empty area shows a white one, so it reads at any charge level.
- The icon shows sport whenever the stored speed limit is > 25. The limit changes only at unlock (§6), so the icon is fixed for the whole ride.

### 3.4 Animations

The UI should feel alive, not just redraw. Swang Stodva's renderer already runs at
~50 fps on the SW102, so short animations fit the frame budget.

| Animation | When | Proposal |
|---|---|---|
| **Boot** | Power-on, before the riding screen (or the PIN screen if locked) | 1. The **sparkles** icon from Swang Stodva (`assets/sparkles.xbm`) is shown centered, **64 px tall** (full screen height). It's already 64×64, so no crop or scale is needed; if the asset changes, crop or scale it to 64 px tall.<br>2. The icon clears, then two rows, a big **`SWET102`** over the version in smaller text (**`v0.1.0`**), slide in from the right, stay still for ~1.2 s so they can be read, and slide out to the left. The version comes from the build, not hardcoded.<br>3. Then the riding screen (or the PIN screen). If the motor link isn't up by then, the "--" error screen (§8) says so; it waits for the animation to end instead of interrupting it. Any button skips the animation (that press does nothing else). |
| **Page switch** | M short, PWR short (jump to PAS) | ~150 ms horizontal slide inside the white tile: the old glyph slides out to the left and the new one comes in from the right (the other way round for PWR back to PAS). The tile itself stays put. |
| **Info-pane switch** | M double-click | ~200 ms vertical slide: the old view slides up and out, the new view comes up from the bottom. Column 1 and the battery don't move. |
| **PAS change** | LEFT/RIGHT on the PAS page | The number slides sideways like the pages (~100 ms): a higher level comes in from the right, a lower one from the left. |

Rules:

- **Animations never block input.** A button press during an animation snaps it
  to its end state at once and handles the press. Fast PAS taps or page flips
  must not queue up behind animations.
- **Data stays live.** Speed and the other values keep updating during a pane
  slide; the animation moves live content, not a frozen snapshot.
- Durations are compile-time constants, tuned in the emulator. Easing is a
  simple ease-out (fast start, soft stop), done in integer math.
- The emulator's scripted tests run on a fixed simulated clock, so mid-animation
  frames can be asserted exactly (§10).
- Out of scope for now: animations for the menu, error screen and lock. The
  lock already has its padlock flash (§4).

---

## 4. Controls

| Gesture | Action |
|---|---|
| LEFT short | Current page's "minus/left" action |
| LEFT hold (PAS page, level 0) | Walk assist while held |
| LEFT hold (Player page) | Previous track |
| RIGHT short | Current page's "plus/right" action |
| RIGHT hold (Player page) | Next track |
| RIGHT double-click (Player page) | Play / pause |
| M short | Next page |
| M double-click (within 350 ms) | Next info-pane view |
| M hold (1 s) | Open menu |
| PWR hold | Power on / off (works everywhere, including the menu) |
| PWR double-click (within 350 ms) | **Lock**: flash a padlock for ~0.5 s, set `locked = 1`, save, power off (see §6) |
| PWR short | Jump to the **PAS page** (from any page) |

Menu navigation: see §5.

An M short press only takes effect after the 350 ms double-click window closes,
so page switching lags by ~350 ms. That's accepted in exchange for being
forgiving with gloves. The timings are constants and can be tuned in the emulator.

The same rule applies to RIGHT on the Player page only: volume + waits for the
double-click window to close. On every other page RIGHT acts immediately, so PAS
changes stay instant. Gesture detection is per page: a page declares which
gestures it uses, and a button with no double-click handler fires right away.

PWR short has the same lag: it waits 350 ms to make sure it isn't the start of a
lock double-click. Jumping back to PAS is not time-critical, so that's acceptable.

---

## 5. Menu (M hold)

**M hold (1 s)** opens the menu from the riding screen. The menu shows **one item
per screen**: a big 1-bit icon with a short label underneath, using the full
128×64. Riding data is hidden while the menu is open, but the motor keeps working
normally (assist, lights and speed limit don't change).

```
┌──────────────────────────────────────────┐
│ ‹                 ╭───╮                › │
│                   │ ▣ │   (big icon)     │
│                   ╰───╯                  │
│                RESET TRIP                │
│                  ● ○ ○ ○ ○               │  ← position dots
└──────────────────────────────────────────┘
```

### Navigation

| Gesture | Action |
|---|---|
| LEFT / RIGHT | Previous / next item (wraps around) |
| M short | **Enter / set**: open the item, or confirm |
| PWR short | **Esc**: back one level. From the item list, it exits the menu to the riding screen. |
| PWR hold | Power off |

- **No timeout.** The menu stays open until you exit it, even while riding.
- It opens on the first item every time; the last position isn't remembered.
- **Detail screens** (BLE status, diagnostics, firmware version): PWR (esc) returns to the item. Inside diagnostics, LEFT/RIGHT page through the values when there's more than one screen of them.

### Items (in ring order)

| # | Item | Icon | Function | Confirm |
|---|---|---|---|---|
| 1 | **Reset trip** | circular arrow | Zeroes the manual trip counter. Battery trip and ride counters reset themselves. | **Yes**: "Reset trip?" M = yes, PWR = cancel. After a reset, a short "done" toast returns to the item. |
| 2 | **BLE status** | Bluetooth rune | Detail screen: phone connected or not, whether commands are subscribed, the display's BLE address (for the app's first-run setup). | — |
| 3 | **Motor diagnostics** | wrench | Detail screen: raw protocol values, per-opcode reply timeouts, UART error count. | — |
| 4 | **Firmware version** | "i" | Detail screen: version, git hash, build date. | — |
| 5 | **Reboot to DFU** | download arrow | Reboots into casainho's bootloader in BLE DFU mode, as a fallback if the phone can't trigger an update over the air. Motor assist stops until the display comes back up, hence the confirmation. | **Yes**: "Reboot to DFU?" M = yes, PWR = cancel. |

Not in the menu, on purpose: **Lock** (it's PWR double-click, §4). PINs, wheel size, speed limits and motor type are compile-time
constants. Brightness follows the light state automatically. The speed-limit encoding test (§6) is done by reflashing, not with a menu item.

---

## 6. Riding modes and anti-theft lock

### Modes

| Mode | Speed limit | UI |
|---|---|---|
| **City** | 25 km/h | plain battery icon |
| **Sport** | protocol max (99) | battery icon with lightning bolt |

The mode itself is **not stored**. What is stored is the **speed limit** (km/h): the
city PIN sets 25, the sport PIN sets 99. Everything else derives from it: **sport
means speed limit > 25**. That keeps one source of truth, and a third limit (e.g. 32)
would only need another PIN and nothing else.

The speed limit is sent to the controller with the Bafang `WRITE_SPEED_LIM` command:
`[0x16, 0x1F, hi, lo, checksum = sum of the first 4 bytes]`, no reply.

**How Swang Stodva does it** (`src/state.c`, `bafang_send_write_speed_limit()`):
it sends the frame **once**, after the UART link stabilizes (~1 s, in
`rt_first_time_management()`), with the value encoded as **km/h × 10**
(25 km/h → 250). (The "wire later" note in its `eeprom_internal.h` is outdated:
the command is already wired up.)

⚠ **The unit is unverified.** bbs-fw's decoder for the same opcode (commented
out in `extcom.c`, because bbs-fw ignores the display's limit) reads the value as
**wheel RPM** (`app_set_wheel_max_speed_rpm(value)`). If stock Bafang also reads
RPM, Swang Stodva's 250 means 250 rpm × 2.165 m ≈ **32.5 km/h**, not 25.

Swet102 plan:

- Compute the value from the hardcoded wheel: **RPM = km/h × 1000 / 60 / 2.165**. City 25 km/h → **192**.
  Keep the encoding behind one function, so switching to km/h × 10 is a one-line change if the hardware test says so.
- Sport: send a value well above any reachable speed (e.g. 99 km/h equivalent). The controller's own config-tool limit still applies on top.
- Send at link-up, **whenever the stored limit changes** (i.e. after unlock), and again after any UART link loss and recovery, in case the controller rebooted.
- **Hardware test before relying on city mode:** bike on a stand, city mode, check where assist cuts out. Do this with both encodings.

### Lock

The lock is an anti-theft measure for **short stops**. It controls **assist only**: a locked bike can still be pedaled, just with no motor help.

- By default the bike **does not ask for a PIN**.
- **PWR double-click**: flash a padlock, set `locked = 1`, save, power off. This is the only way to lock; there is no menu item.
- Next power-on with `locked = 1`: show the **PIN entry screen**. Assist is forced to 0 until a PIN is accepted.
- PIN entry is a **digit picker**, 4 digits: LEFT/RIGHT change the digit, M confirms it and moves on, **PWR short = backspace** (back to the previous digit; on the first digit it does nothing). PWR hold powers off as usual.
- **City PIN** → unlock with speed limit 25 (city). **Sport PIN** → unlock with speed limit 99 (sport).
  So locking and unlocking is also **how you switch modes**.
- On success: `locked = 0`, `speed_limit` saved, normal operation.
- Wrong PIN: clear the entry and retry, with no penalty and no delay. A locked
  bike can still be pedaled, so brute-force protection isn't worth the hassle.

### Persistent state (EEPROM / flash)

| Variable | Type | Purpose |
|---|---|---|
| `version` | u8 | Layout version. A mismatch resets to defaults (except odo, see below). |
| `pas` | u8 | Last assist level 0–9, restored at boot |
| `speed_limit` | u8 | km/h sent to the controller; 25 = city, > 25 = sport |
| `locked` | u8 | Ask for a PIN at next boot |
| `odo_m` | u32 | Total distance, meters |
| `trip_m` | u32 | Manual trip, meters |
| `trip_moving_s` | u32 | Manual trip moving time, seconds (for the average) |
| `trip_max_x10` | u16 | Manual trip max speed, km/h × 10 |
| `trip_batt_m` | u32 | Battery trip, meters |
| `trip_batt_moving_s` | u32 | Battery trip moving time, seconds |
| `trip_mah` | u32 | Manual trip charge used, mAh |
| `trip_batt_max_x10` | u16 | Battery trip max speed, km/h × 10 |
| `trip_batt_mah` | u32 | Battery trip charge used, mAh |
| `odo_max_x10` | u16 | All-time max speed, km/h × 10 |
| `trip_batt_soc_min` | u8 | Lowest battery % during the current battery trip |

The ride counter and its max/avg (since boot) live in RAM only. Lights are **not** persisted: they're off at boot.

The odometer should survive a layout-version bump. Migration code copies
`odo_m` across versions, because losing the odometer on a firmware update is
the one reset that would actually hurt.

#### What Swang Stodva stores today, and whether Swet102 needs it

From `src/eeprom_internal.h` (layout `0x47`):

| Swang Stodva field | Swet102 |
|---|---|
| `eeprom_version` | **Keep** (`version`) |
| `ui8_assist_level` | **Keep** (`pas`) |
| `ui32_odometer_x10` | **Keep** (as `odo_m`, in meters; SS's stored data itself is not migrated) |
| `ui16_wheel_perimeter` | Drop: hardcoded 2165 mm |
| `ui8_units_type` | Drop: metric only |
| `ui8_time_field_enable` | Drop: no clock on screen |
| `ui8_number_of_assist_levels` | Drop: fixed 0–9 |
| `ui8_lcd_power_off_time_minutes` | Drop: fixed 5 min |
| `ui8_lcd_backlight_on/off_brightness` | Drop: fixed constants, chosen by light state |
| `ui8_walk_assist_feature_enabled` | Drop: always on |
| `ui8_street_mode_speed_limit` | **Replaced** by `speed_limit`, set by which PIN unlocked the bike |
| `ui8_motor_power_option` | Drop: constant (BBSHD 1000 W) where a power scale is needed |
| `ui8_ble_broadcast_enabled` | Drop: telemetry always on when a phone is connected |
| `ui8_battery_voltage_option` | Drop: fixed 52 V |
| `ui8_motor_firmware` | Drop: always stock |

Net result: 3 of 15 fields carry over, and the speed limit comes back in a new role. Swet102 adds `locked`, two trip counters and their max/avg stats.

---

## 7. Motor communication

- Bafang stock display protocol over UART at 1200 baud (reference implementation: Swang Stodva `uart.c`).
- **Read**: speed (wheel RPM → km/h using 2165 mm), current, battery %, moving state, errors.
- **Write**: PAS level (0–9 → stock level codes, plus the walk-assist code), lights, speed limit (the stored value).
- Not used: bbs-fw extensions (motor temp, voltage), cadence.

---

## 8. Errors and power

### Errors

Motor error codes and **lost UART communication** are shown **full screen**, kept
minimal: a big **exclamation mark** and the **error code** next to it, as the
motor reports it. No text descriptions. Lost motor link uses the same screen with
`--` in place of a code.

```
┌──────────────────────────────────────────┐
│        ██                                │
│        ██          2 1                   │
│        ██                                │
│                                          │
│        ██                                │
└──────────────────────────────────────────┘
```

M dismisses the screen. It comes back after **10 s** if the condition is still present, or straight away if a new error arrives. A motor error clears after 3 normal status replies; braking (status `03`) is not an error. After power-on the motor gets 2 s to answer before the `--` screen appears.

### Power

- PWR hold: on / off. PWR double-click: lock + off (§6).
- **Auto power-off** after **5 min** with no speed, no motor current and no button presses.
- State is saved on every change that matters (pas, speed limit, lock) and on power-off.

---

## 9. Bluetooth (BLE)

| Feature | Scope |
|---|---|
| OTA firmware update | In scope. **Keep casainho's bootloader + SoftDevice S130**, with the same flash layout and DFU `.zip` format as Swang Stodva. You can switch firmwares over BLE. |
| Telemetry | In scope. Stream speed, power, battery, PAS, speed limit, trips with max/avg, odo to the phone. |
| Player control | In scope (firmware side). Volume, prev/next track, play/pause. |
| Gate opener | In scope (firmware side). The gate A / gate B commands are sent to the phone, which does the actual opening. |
| Companion app | **Out of scope.** This doc defines the BLE GATT service; the app comes later. `swang-stodva-app` may be a starting point. |

### Goal: zero-touch connection

The rider always has the phone along. **Turning the display on should be all it
takes**: the phone connects by itself, with no taps, and stays connected for the
whole ride. Pairing or no pairing is just a means to that end. The choice below is
the one that is simplest and most reliable on the phone.

### Connection model: no pairing (same as Swang Stodva)

- The display is **always advertising** as `swet102` (connectable, no timeout while powered on; see below).
- The phone app scans by name, **connects without pairing or bonding**, and subscribes to notifications. The display keeps no list of phones and has no pairing UI.
- Commands are **fire-and-forget**: a button press sends a notification on the command characteristic. If no phone is connected or subscribed, the press is dropped. The display doesn't queue or retry.
- **No authentication on commands, including the gate.** The phone trusts any device advertising as `swet102`. This is an accepted risk: someone nearby could spoof the display and trigger gate A/B. Possible upgrades later: pin the display's MAC address in the app, or add an HMAC with a counter using a secret baked in at build time.
- Only one phone can be connected at a time. After a disconnect, advertising restarts immediately.
- Player/Gate tiles show "unavailable" (§3.1) while no phone is subscribed to the command characteristic.
- Considered and rejected: **pure advertising broadcast** (commands packed into advertising data, no connection). Android throttles scanning, so presses would lag by seconds or get lost. That's not acceptable for volume or the gate.

### Firmware requirements for auto-connect

- Use the chip's **static random address** (from FICR). It never changes across reboots or firmware updates, so the phone can remember it.
- **Advertise for as long as the display is on**: fast interval (~100 ms) for the first 30 s after boot or a disconnect, then slow (~1 s) to save power. There's no 180 s timeout like Swang Stodva has.
- Restart advertising immediately after a disconnect.
- Use relaxed connection parameters (e.g. 50–100 ms interval, 4–6 s supervision timeout), so a phone in a pocket doesn't drop the link.

### Companion app requirements (for whoever builds it)

The phone app does most of the auto-connect work. These are hard requirements:

- **First run only:** scan for `swet102`, then save its address.
- From then on: **`connectGatt(address, autoConnect = true)`** on Android. The OS
  Bluetooth stack connects in the background whenever the display shows up and
  reconnects after drops, without the app scanning.
- An **Android foreground service** (persistent notification) keeps the connection
  and command handling alive while the phone is locked in a pocket. On iOS: the
  `bluetooth-central` background mode plus a pending `connect()`.
- On every connect, subscribe to the telemetry and command characteristics again (no bonding means no saved subscriptions).
- Start at phone boot, and survive app swipe-away.
- The command handlers (media keys for the player, gate actions) must work with the screen locked.

**GATT: one custom 128-bit service** containing:

- telemetry characteristic (notify, ~1 Hz: speed, power, battery %, PAS, speed limit, odo, error)
- trips characteristic (notify, every 5 s: distance, max, avg and Ah for each of the 3 trips, plus odometer and all-time max)
- command characteristic (display → phone notify: player vol−/vol+, prev/next track, play/pause, gate A, gate B)
- standard Device Information service (firmware version)

The exact byte layout belongs in a separate implementation spec. Unlocking and
changing the speed limit from the phone are **not** supported: the PIN on the display is the only way.

---

## 10. Desktop emulator (Rust)

- **Pixel-exact screen**: the real application core (the same Rust crate as the firmware) running on the host, with the 128×64 framebuffer rendered in a desktop window.
- **Simulated motor**: a fake Bafang stock controller on the virtual UART, with keys or sliders for speed, current, battery and error injection.
- **Scripted tests**: a headless mode that feeds button gestures (including double-click and hold timing), then asserts on the framebuffer, persistent state and UART traffic. Runs in CI.
- BLE is **not** simulated at first.

The core sits behind a HAL trait (display, buttons, UART, flash, BLE, power) so that exactly the same Rust code runs on the nRF51822 and inside the emulator. See TECH_DESIGN.md.

---

## 11. Engineering

| Item | Decision |
|---|---|
| Firmware language | **Rust** core (`no_std`) for all logic + thin **C** platform layer for the nRF5 SDK 12.3 + S130 (as in Swang Stodva) |
| Emulator language | Rust (uses the `swet-heart` crate directly) |
| Repo | New, separate repo `swet102`; Swang Stodva is a read-only reference |
| Build | Nix (reproducible); PINs injected as build args |
| Flashing | SWD (WCH-LinkE) for development; BLE OTA via casainho's bootloader |
| License | **GPL-3.0** (allows reusing Swang Stodva / casainho code) |

---

## 11a. Future ideas (after M6)

Not planned for any milestone yet. Kept here so they aren't lost.

### Get Money (Lightning withdraw)

A menu item **"Get Money"**. You enter **3 digits** with the same digit picker as
the PIN (LEFT/RIGHT change the digit, M next, PWR back), and the display shows a
QR code with an **LNURL-withdraw** link for that many sats (000–999). Scanning it
with a Lightning wallet (Phoenix, Zeus, Breez, …) pulls the sats into the wallet.

- The bike holds no money and has no internet. The sats come from a server
  you run (e.g. LNbits); the display only produces a link the server trusts.
- Every link works **once** and is **signed** by the display, so a photo of the
  screen can't be reused and nobody can make up their own amount.
- Optional limits on the server side: per-link maximum, daily total.

### The bike pays per km

Same mechanism, but the amount comes from riding: the display offers a link
worth (say) N sats per km ridden since the last claim, e.g. from the battery
trip or a dedicated "reward" counter. Claiming resets that counter.

Both ideas share the same pieces (signed one-time links, a QR code generated on
the display, a small server); see TECH_DESIGN.md §15a.

## 12. Open questions

1. **Speed-limit unit**: wheel RPM vs. km/h × 10. The owner will answer this later (stand test in §6).

Moved to the technical design: the BLE byte layout (telemetry struct, command opcodes).

---

## 13. Decision log

| Date | Decision |
|---|---|
| 2026-10-03 | Wheel circumference 2165 mm |
| 2026-10-03 | PINs injected at build time via Nix build arg / env, never in the repo |
| 2026-10-03 | Keep casainho's bootloader + S130 for OTA |
| 2026-10-03 | Info pane ring: speed → power → trip → odo ("Distance" merged into Trip) |
| 2026-10-03 | Gate page: LEFT = gate A, RIGHT = gate B, short presses |
| 2026-10-03 | M timing: 350 ms double-click window, 1 s hold |
| 2026-10-03 | Wrong PIN: plain retry, no back-off |
| 2026-10-03 | Auto power-off after 5 min idle |
| 2026-10-03 | BLE: one custom GATT service (telemetry notify + command notify) + Device Info |
| 2026-10-03 | BLE: no pairing/bonding (like Swang Stodva); phone connects by name, commands are fire-and-forget; no pairing menu item |
| 2026-10-03 | Gate commands unauthenticated (accepted risk); MAC pin / HMAC possible later |
| 2026-10-03 | Zero-touch goal: phone auto-connects via remembered static address + autoConnect + foreground service; display advertises forever |
| 2026-10-03 | Menu: one item per screen (icon + label), LEFT/RIGHT flip, M enter, PWR esc, no timeout, usable while riding |
| 2026-10-03 | Menu items: Reset trip (confirm), BLE status, Motor diagnostics, Firmware version, Reboot to DFU |
| 2026-10-03 | Lock moved out of the menu to PWR double-click (padlock flash, then power off) |
| 2026-10-03 | Riding: PWR short = jump to PAS page, PWR hold = off. Menu: M = enter/set, PWR = esc (back / exit / cancel) |
| 2026-10-03 | PIN entry: PWR short = backspace to previous digit |
| 2026-10-03 | Animations: boot (~1.5 s), page switch (vertical slide in tile), info-pane switch (horizontal push); never block input |
| 2026-10-03 | Boot animation: Swang Stodva sparkles icon, then "SWET102 v<version>" scrolling right to left |
| 2026-10-03 | Speed font: start with 04B_30 34 px from Swang Stodva, judge in emulator |
| 2026-10-03 | Boot: sparkles centered (64 px tall) first, then the version text scrolls across the full width |
| 2026-10-03 | Reboot to DFU gets a confirmation (M = yes, PWR = cancel) |
| 2026-10-03 | Stock battery % trusted as-is (stable in Swang Stodva practice) |
| 2026-10-03 | Error screen: big "!" + motor error code; "--" for lost motor link |
| 2026-10-03 | BLE byte layout deferred to technical design |
| 2026-10-03 | Walk assist: hold LEFT on PAS page at level 0 |
| 2026-10-03 | Player page: click = volume, hold = prev/next track, RIGHT double-click = play/pause |
| 2026-10-03 | Three distance counters: manual trip, battery trip (auto-reset on +10 % charge), ride (since boot) |
| 2026-10-03 | Speed limit sent as wheel RPM (pending hardware verification vs. Swang Stodva's km/h × 10) |
| 2026-10-03 | Page column = white rounded tile with a black glyph: PAS number, ↑ walk assist, bulb, music note, car |
| 2026-10-03 | Mode shown by the battery icon: plain = city, lightning bolt (XOR) = sport |
| 2026-10-03 | EEPROM: keep version/pas/odo from Swang Stodva, drop the other 12 fields, add mode/locked/trips |
| 2026-10-04 | Core logic in Rust (was C), thin C platform layer for the SDK; distances stored in meters (see TECH_DESIGN.md) |
| 2026-10-05 | PAS number font: W95FA |
| 2026-10-05 | Error screen: returns after 10 s if still present; 2 s grace after power-on; error code and "!" in W95FA |
| 2026-10-05 | Future (after M6): "Get Money" menu item (3-digit amount → signed one-time LNURL-withdraw QR) and "the bike pays per km"; see §11a |
| 2026-10-04 | Odometer starts at 0 when switching from Swang Stodva; no migration or seed value |
| 2026-10-04 | Store the speed limit instead of a mode; sport = limit > 25 (city PIN → 25, sport PIN → 99) |
| 2026-10-04 | Every trip (manual, battery, ride) tracks max speed and average speed over moving time |
| 2026-10-04 | Odometer keeps an all-time max speed; every trip also tracks charge used (Ah) from integrated motor current |
| 2026-10-04 | Motor polling stays on fixed 100 ms slots (event-driven polling considered, deferred) |
| 2026-10-05 | M5: Lights page shows a bulb (outline = off, filled = on) with ON/OFF; screen dims while the lights are on. Player/Gate glyphs are dithered without a phone subscribed to commands |
| 2026-10-06 | M6: boot = sparkles 0.6 s, then the version scrolls at 150 px/s (≈ 2.3 s in all); a missing motor shows the "--" screen after the boot instead of a frozen frame. Page slide 150 ms (up for M, down for PWR back to PAS), PAS roll 100 ms, pane push 200 ms; a new button press snaps any running animation |
| 2026-10-06 | First daily-use release: v0.1.0 |
| 2026-10-07 | Owner, after trying M6: boot text as tall as the screen (scroll 500 px/s, ≈ 2.1 s in all); page switch slides horizontally, info pane slides vertically (up). The PAS digit still rolls vertically |
| 2026-10-07 | Owner, second look: boot text in two rows (SWET102 / version), slide in → hold 1.2 s → slide out (≈ 2.6 s in all; the scroll was too fast to read); the PAS number slides horizontally like the pages |
| 2026-10-07 | Version row in smaller text than SWET102, so `v<version>` always fits (no dropped "v") |
