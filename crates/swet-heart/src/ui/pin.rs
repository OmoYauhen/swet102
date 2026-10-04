//! PIN entry after a locked power-off (PRODUCT §6), and the padlock shown
//! while locking.
//!
//! Digit picker: LEFT/RIGHT change the current digit, M confirms it and moves
//! on, PWR goes back one digit. After the 4th digit the App checks the code.

use crate::config;
use crate::gfx::assets::W95;
use crate::gfx::{Frame, Mode, W, num};

const BOX_W: i32 = 22;
const BOX_H: i32 = 38;
const GAP: i32 = 6;
const BOX_Y: i32 = 18;

pub struct PinEntry {
    digits: [u8; 4],
    pos: u8,
    wrong: bool,
    wrong_until: u32,
}

impl Default for PinEntry {
    fn default() -> Self {
        Self::new()
    }
}

impl PinEntry {
    pub const fn new() -> Self {
        Self {
            digits: [0; 4],
            pos: 0,
            wrong: false,
            wrong_until: 0,
        }
    }

    pub fn up(&mut self) {
        let d = &mut self.digits[usize::from(self.pos)];
        *d = (*d + 1) % 10;
    }

    pub fn down(&mut self) {
        let d = &mut self.digits[usize::from(self.pos)];
        *d = (*d + 9) % 10;
    }

    /// Accept the current digit. Returns the code once all four are in, and
    /// starts a fresh entry.
    pub fn confirm(&mut self) -> Option<[u8; 4]> {
        if self.pos < 3 {
            self.pos += 1;
            return None;
        }
        let code = self.digits;
        *self = Self::new();
        Some(code)
    }

    /// Back to the previous digit; nothing on the first one.
    pub fn backspace(&mut self) {
        self.pos = self.pos.saturating_sub(1);
    }

    /// The code matched neither PIN: show "WRONG PIN" for a moment.
    pub fn rejected(&mut self, now: u32) {
        self.wrong = true;
        self.wrong_until = now.wrapping_add(config::PIN_WRONG_MS);
    }

    pub fn position(&self) -> u8 {
        self.pos
    }

    pub fn render(&self, f: &mut Frame, now: u32) {
        let show_wrong = self.wrong && now.wrapping_sub(self.wrong_until) > u32::MAX / 2;
        let title: &[u8] = if show_wrong {
            b"WRONG PIN"
        } else {
            b"ENTER PIN"
        };
        let tw = title.len() as i32 * 4 - 1;
        f.text3x5((W - tw) / 2, 5, title, 1, Mode::Set);

        let x0 = (W - (4 * BOX_W + 3 * GAP)) / 2;
        for i in 0..4u8 {
            let x = x0 + i32::from(i) * (BOX_W + GAP);
            if i == self.pos {
                // current digit: white tile, black digit (like the page tile)
                f.round_rect(x, BOX_Y, BOX_W, BOX_H, 3, Mode::Set);
                let mut d = [0u8; 10];
                let s = num::u32_dec(u32::from(self.digits[usize::from(i)]), &mut d);
                let gw = W95.width(s, 0);
                let gy = BOX_Y + (BOX_H - i32::from(W95.height)) / 2;
                f.text(&W95, s, x + (BOX_W - gw) / 2, gy, 0, Mode::Clear);
            } else {
                f.rect(x, BOX_Y, BOX_W, BOX_H, Mode::Set);
                if i < self.pos {
                    // entered: a dot, never the digit
                    f.fill_rect(x + BOX_W / 2 - 3, BOX_Y + BOX_H / 2 - 3, 6, 6, Mode::Set);
                }
            }
        }
    }
}

/// Full-screen padlock while locking.
pub fn render_padlock(f: &mut Frame) {
    let cx = W / 2;
    // shackle: a U of 6 px bars
    f.fill_rect(cx - 14, 6, 28, 26, Mode::Set);
    f.fill_rect(cx - 8, 12, 16, 20, Mode::Clear);
    // body
    f.round_rect(cx - 20, 28, 40, 30, 3, Mode::Set);
    // keyhole
    f.fill_rect(cx - 4, 36, 8, 7, Mode::Clear);
    f.fill_rect(cx - 2, 42, 4, 8, Mode::Clear);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_wraps_moves_and_returns_the_code() {
        let mut p = PinEntry::new();
        p.down(); // 0 → 9
        assert_eq!(p.confirm(), None);
        p.up();
        p.up(); // 2
        assert_eq!(p.confirm(), None);
        p.backspace(); // back to the second digit, still 2
        assert_eq!(p.position(), 1);
        p.up(); // 3
        assert_eq!(p.confirm(), None);
        p.up(); // third digit: 1
        assert_eq!(p.confirm(), None);
        assert_eq!(p.confirm(), Some([9, 3, 1, 0]));
        assert_eq!(p.position(), 0, "fresh entry after the 4th digit");
    }

    #[test]
    fn backspace_on_the_first_digit_does_nothing() {
        let mut p = PinEntry::new();
        p.backspace();
        assert_eq!(p.position(), 0);
    }
}
