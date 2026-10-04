//! M0 hardware-trial screen: orientation markers, live button state, platform
//! diagnostics, a motor STATUS ping and a moving bar to prove the tick runs.
//!
//! Controls: M click cycles the SH1107 orientation, PWR hold powers off.

use crate::config;
use crate::gfx::{Frame, Mode, num};
use crate::hal::{Buttons, Diag, Hal};

const PING_MS: u32 = 500;

pub struct TestPattern {
    orient: u8,
    prev: Buttons,
    /// All buttons have been released once since power-on (the PWR press
    /// that switched the display on is ignored until then).
    armed: bool,
    pwr_down_at: Option<u32>,
    frames: u32,
    motor_tx: u32,
    motor_rx: u32,
    next_ping: u32,
}

impl TestPattern {
    pub const fn new() -> Self {
        Self {
            orient: config::DISPLAY_ORIENT,
            prev: Buttons(0),
            armed: false,
            pwr_down_at: None,
            frames: 0,
            motor_tx: 0,
            motor_rx: 0,
            next_ping: 0,
        }
    }

    pub fn tick(&mut self, hal: &mut impl Hal, now: u32) {
        let b = hal.buttons();
        if !self.armed {
            self.armed = b.0 == 0;
        } else {
            let pressed = Buttons(b.0 & !self.prev.0);
            if pressed.has(Buttons::M) {
                self.orient = (self.orient + 1) & 3;
                hal.display_orient(self.orient);
            }
            if pressed.has(Buttons::PWR) {
                self.pwr_down_at = Some(now);
            }
            if !b.has(Buttons::PWR) {
                self.pwr_down_at = None;
            }
            if let Some(t) = self.pwr_down_at
                && now.wrapping_sub(t) >= config::HOLD_MS
            {
                hal.power_off();
            }
        }
        self.prev = b;

        while hal.uart_read().is_some() {
            self.motor_rx += 1;
        }
        if now >= self.next_ping {
            hal.uart_write(&[0x11, 0x08]); // Bafang READ STATUS
            self.motor_tx += 1;
            self.next_ping = now + PING_MS;
        }
        self.frames += 1;
    }

    pub fn render(&self, f: &mut Frame, hal: &impl Hal, now: u32) {
        let mut d = [0u8; 10];
        let mut h = [0u8; 8];
        f.rect(0, 0, 128, 64, Mode::Set);

        // Orientation markers: an up arrow + TOP at the top-left, a single
        // pixel at (1,1) and a 3×3 block at the bottom-right corner.
        f.pixel(1, 1, Mode::Set);
        f.vline(5, 3, 8, Mode::Set);
        f.hline(4, 4, 3, Mode::Set);
        f.hline(3, 5, 5, Mode::Set);
        f.text3x5(10, 3, b"TOP", 1, Mode::Set);
        f.fill_rect(124, 60, 3, 3, Mode::Set);

        let x = f.text3x5(30, 3, b"SWET102 V", 1, Mode::Set);
        f.text3x5(x, 3, config::VERSION.as_bytes(), 1, Mode::Set);
        let x = f.text3x5(100, 3, b"OR", 1, Mode::Set);
        f.text3x5(
            x + 2,
            3,
            num::u32_dec(u32::from(self.orient), &mut d),
            1,
            Mode::Set,
        );

        let line = |f: &mut Frame, y: i32, parts: &[&[u8]]| {
            let mut x = 3;
            for p in parts {
                x = f.text3x5(x, y, p, 1, Mode::Set);
            }
        };
        let mut d2 = [0u8; 10];
        line(
            f,
            12,
            &[
                b"TICK US ",
                num::u32_dec(hal.diag(Diag::TickAvgUs), &mut d),
                b"/",
                num::u32_dec(hal.diag(Diag::TickMaxUs), &mut d2),
            ],
        );
        line(
            f,
            19,
            &[
                b"MISS ",
                num::u32_dec(hal.diag(Diag::MissedTicks), &mut d),
                b"  STACK FREE ",
                num::u32_dec(hal.diag(Diag::StackFreeBytes), &mut d2),
            ],
        );
        line(
            f,
            26,
            &[
                b"RAM ",
                num::u32_dec(hal.diag(Diag::RamKb), &mut d),
                b"K  SD BASE ",
                num::u32_hex(hal.diag(Diag::SdRamBase), 8, &mut h),
            ],
        );
        line(
            f,
            33,
            &[
                b"MOTOR TX ",
                num::u32_dec(self.motor_tx, &mut d),
                b" RX ",
                num::u32_dec(self.motor_rx, &mut d2),
            ],
        );
        line(
            f,
            40,
            &[
                b"UART ERR ",
                num::u32_dec(hal.diag(Diag::UartErrors), &mut d),
            ],
        );

        // Live buttons: lit box = pressed.
        let b = self.prev;
        for (i, (mask, label)) in [
            (Buttons::LEFT, b"L"),
            (Buttons::RIGHT, b"R"),
            (Buttons::M, b"M"),
            (Buttons::PWR, b"P"),
        ]
        .iter()
        .enumerate()
        {
            let bx = 74 + i as i32 * 13;
            f.rect(bx, 38, 11, 11, Mode::Set);
            if b.has(*mask) {
                f.fill_rect(bx + 1, 39, 9, 9, Mode::Set);
            }
            f.text3x5(bx + 4, 41, *label, 1, Mode::Xor);
        }

        // Moving bar: proves ticks run and shows animation smoothness.
        let period = 2000;
        let p = (now % period) as i32;
        let travel = 128 - 2 - 16;
        let pos = if p < 1000 {
            p * travel / 1000
        } else {
            (2000 - p) * travel / 1000
        };
        f.fill_rect(1 + pos, 54, 16, 4, Mode::Set);
        line(f, 58, &[b"FRAMES ", num::u32_dec(self.frames, &mut d)]);
    }
}

impl Default for TestPattern {
    fn default() -> Self {
        Self::new()
    }
}
