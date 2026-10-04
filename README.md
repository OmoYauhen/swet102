# Swet102

Opinionated firmware for the Bafang **SW102** handlebar display, built for one
e-bike: **PET** (Personal Electrical Transport). SW102 + PET = Swet102.

- [`docs/PRODUCT.md`](docs/PRODUCT.md) — what it does
- [`docs/TECH_DESIGN.md`](docs/TECH_DESIGN.md) — how it's built

**Status:** M0 — skeleton and Rust-on-nRF51 trial. The display shows a test
pattern, not the riding UI yet.

## Layout

| Path | What |
|---|---|
| `crates/swet-heart` | `no_std` Rust core: all logic, hardware behind `trait Hal` |
| `crates/swet-fw` | staticlib for the nRF51: `Hal` over FFI, `swet_init` / `swet_tick` |
| `crates/swet-sim` | simulator: virtual clock, fake motor, golden-frame tests |
| `crates/swet-emu` | desktop emulator window |
| `platform/nrf51` | C: main loop, SH1107, UART, SoftDevice glue (nRF5 SDK 12.3 + S130) |

## Build

Everything runs inside the Nix dev shell:

```sh
nix develop
cargo test                 # core + simulator tests
cargo run -p swet-emu      # emulator: ←/→ LEFT/RIGHT, ↓/Space M, P/Esc PWR, L motor link, F12 PNG
make                       # build/swet102.hex
make check                 # size gates
```

`nix flake check` runs what CI runs.

## Flashing

The display keeps casainho's bootloader and S130. Firmware updates need a
**`VERSION_NUM` higher than the installed one** (the bootloader refuses
downgrades); `version.mk` makes it date-based.

```sh
make dfu        # build/swet102-<VERSION_NUM>.zip → nRF Toolbox → DFU
make flash-app  # over SWD (WCH-LinkE in DAP mode), keeps bootloader + S130
```

After an OTA update the bootloader keeps advertising `SW102_DFU` until you
**power off and start the display with a long PWR press**.

`tools/nrfutil.sh` and `tools/openocd.sh` run nrfutil 6.1.7 and OpenOCD in Docker
(nrfutil needs Python < 3.11; raw USB needs root on NixOS).

## M0 test pattern

| Shows | Meaning |
|---|---|
| ↑ TOP, pixel at top-left, block at bottom-right | which way is up; **M click** cycles the 4 SH1107 orientations (`OR n`) |
| `TICK US avg/max`, `MISS` | time spent in one `swet_tick()`, late ticks |
| `STACK FREE` | bytes of the 4 KB stack never touched |
| `RAM nK`, `SD BASE` | RAM size from FICR, lowest RAM start the SoftDevice accepts |
| `MOTOR TX/RX` | a STATUS request every 500 ms and the reply bytes received |
| L R M P boxes | live button state |
| moving bar | the tick loop and frame rate |

Hold **PWR** for 1 s to power off.

## License

GPL-3.0. Some low-level code is adapted from
[Swang Stodva](https://github.com/OmoYauhen/swang-stodva) and casainho's SW102 firmware.
