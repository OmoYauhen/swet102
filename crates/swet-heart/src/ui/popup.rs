//! Full-screen error popup (PRODUCT §8, TECH_DESIGN §5.1/§7.3): a big "!" and
//! the motor's error code as two hex digits, or "--" when the motor link is lost.
//! M dismisses it; it returns after `FAULT_REPEAT_MS` if the fault is still
//! there, or at once when the fault changes.
//!
//! Enums carry explicit `repr(u8)` tags with the empty case first, and nothing
//! here is an `Option` field, so a fresh App stays all zeroes (.bss).

use crate::config;
use crate::gfx::assets::W95;
use crate::gfx::{Frame, H, Mode, W, num};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Fault {
    LinkLost,
    Code(u8),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum Dismissed {
    No,
    At(Fault, u32),
}

/// What the popup layer shows this tick.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Popup {
    None,
    Fault(Fault),
}

pub struct Popups {
    shown: Popup,
    dismissed: Dismissed,
}

impl Default for Popups {
    fn default() -> Self {
        Self::new()
    }
}

impl Popups {
    pub const fn new() -> Self {
        Self {
            shown: Popup::None,
            dismissed: Dismissed::No,
        }
    }

    pub fn shown(&self) -> Popup {
        self.shown
    }

    /// Recompute from the current fault (if any).
    pub fn update(&mut self, fault: Option<Fault>, now: u32) {
        self.shown = match fault {
            None => {
                self.dismissed = Dismissed::No;
                Popup::None
            }
            Some(f) => match self.dismissed {
                Dismissed::At(d, t) if d == f && now.wrapping_sub(t) < config::FAULT_REPEAT_MS => {
                    Popup::None
                }
                _ => {
                    self.dismissed = Dismissed::No;
                    Popup::Fault(f)
                }
            },
        };
    }

    /// M on the error screen.
    pub fn dismiss(&mut self, now: u32) {
        if let Popup::Fault(f) = self.shown {
            self.dismissed = Dismissed::At(f, now);
            self.shown = Popup::None;
        }
    }
}

pub fn render(f: &mut Frame, p: Popup) {
    let Popup::Fault(fault) = p else { return };
    f.clear();
    let mut d = [0u8; 8];
    let bang = W95.width(b"!", 0);
    let gap = 20;
    // Two hex digits, or for a lost link two dashes of the same width. The
    // font's own hyphen sits near the baseline and reads like "__".
    let code = match fault {
        Fault::LinkLost => None,
        Fault::Code(c) => Some(num::u32_hex(u32::from(c), 2, &mut d)),
    };
    let cw = code.map_or(W95.width(b"00", 4), |s| W95.width(s, 4));
    let x = (W - (bang + gap + cw)) / 2;
    let y = (H - i32::from(W95.height)) / 2;
    f.text(&W95, b"!", x, y, 0, Mode::Set);
    let cx = x + bang + gap;
    match code {
        Some(s) => {
            f.text(&W95, s, cx, y, 4, Mode::Set);
        }
        None => {
            let dash = (cw - 4) / 2;
            let my = y + i32::from(W95.height) / 2 - 2;
            f.fill_rect(cx, my, dash, 4, Mode::Set);
            f.fill_rect(cx + dash + 4, my, dash, 4, Mode::Set);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: Option<Fault> = Some(Fault::Code(0x21));

    #[test]
    fn shows_dismisses_and_returns_after_the_repeat_time() {
        let mut p = Popups::new();
        p.update(E, 0);
        assert_eq!(p.shown(), Popup::Fault(Fault::Code(0x21)));
        p.dismiss(100);
        p.update(E, 5_000);
        assert_eq!(p.shown(), Popup::None);
        p.update(E, 100 + config::FAULT_REPEAT_MS);
        assert_eq!(p.shown(), Popup::Fault(Fault::Code(0x21)));
    }

    #[test]
    fn a_different_fault_shows_at_once() {
        let mut p = Popups::new();
        p.update(E, 0);
        p.dismiss(0);
        p.update(Some(Fault::Code(0x08)), 1_000);
        assert_eq!(p.shown(), Popup::Fault(Fault::Code(0x08)));
    }

    #[test]
    fn clearing_the_fault_forgets_the_dismissal() {
        let mut p = Popups::new();
        p.update(E, 0);
        p.dismiss(0);
        p.update(None, 1_000);
        p.update(E, 2_000);
        assert_eq!(p.shown(), Popup::Fault(Fault::Code(0x21)));
    }
}
