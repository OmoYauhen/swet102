//! Full-screen popups (TECH_DESIGN §5.1):
//!
//! - **Error** (PRODUCT §8): a big "!" and the motor's error code as two hex
//!   digits, or "--" when the motor link is lost. M dismisses it; it returns
//!   after `FAULT_REPEAT_MS` if the fault is still there, or at once when the
//!   fault changes.
//! - **Battery trip** (PRODUCT §3.2): "Trip on last charge" and its distance,
//!   after the pack was charged. Any button dismisses it. An error wins.
//!
//! Enums carry explicit `repr(u8)` tags with the empty case first, and nothing
//! here is an `Option` field, so a fresh App stays all zeroes (.bss).

use crate::config;
use crate::gfx::assets::{SMALL, SPEED, TEXT, W95};
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
    /// Distance of the battery trip that just ended, 100 m units.
    BatteryTrip(u16),
}

pub struct Popups {
    shown: Popup,
    dismissed: Dismissed,
    battery_trip: u16,
    battery_trip_on: bool,
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
            battery_trip: 0,
            battery_trip_on: false,
        }
    }

    pub fn shown(&self) -> Popup {
        self.shown
    }

    /// Show the battery-trip message until a button is pressed.
    pub fn battery_trip(&mut self, km_x10: u16) {
        self.battery_trip = km_x10;
        self.battery_trip_on = true;
    }

    /// Recompute from the current fault (if any); a fault beats the message.
    pub fn update(&mut self, fault: Option<Fault>, now: u32) {
        let message = if self.battery_trip_on {
            Popup::BatteryTrip(self.battery_trip)
        } else {
            Popup::None
        };
        self.shown = match fault {
            None => {
                self.dismissed = Dismissed::No;
                message
            }
            Some(f) => match self.dismissed {
                Dismissed::At(d, t) if d == f && now.wrapping_sub(t) < config::FAULT_REPEAT_MS => {
                    message
                }
                _ => {
                    self.dismissed = Dismissed::No;
                    Popup::Fault(f)
                }
            },
        };
    }

    /// M on the error screen, any button on the battery-trip message.
    pub fn dismiss(&mut self, now: u32) {
        match self.shown {
            Popup::Fault(f) => self.dismissed = Dismissed::At(f, now),
            Popup::BatteryTrip(_) => self.battery_trip_on = false,
            Popup::None => {}
        }
        self.shown = Popup::None;
    }
}

pub fn render(f: &mut Frame, p: Popup) {
    f.clear();
    let fault = match p {
        Popup::None => return,
        Popup::BatteryTrip(km_x10) => return render_battery_trip(f, km_x10),
        Popup::Fault(fault) => fault,
    };
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

/// "Trip on last charge" with the distance large, in km with one decimal.
fn render_battery_trip(f: &mut Frame, km_x10: u16) {
    let title: &[u8] = b"Trip on last charge";
    f.text(
        &TEXT,
        title,
        (W - TEXT.width(title, 1)) / 2,
        2,
        1,
        Mode::Set,
    );
    let mut d = [0u8; 12];
    let value = num::u32_dec1(u32::from(km_x10), &mut d);
    let (vw, uw) = (SPEED.width(value, 2), SMALL.width(b"km", 1));
    let x = (W - (vw + 4 + uw)) / 2;
    let y = 22;
    f.text(&SPEED, value, x, y, 2, Mode::Set);
    let base = y + i32::from(SPEED.height);
    f.text(
        &SMALL,
        b"km",
        x + vw + 4,
        base - i32::from(SMALL.height),
        1,
        Mode::Set,
    );
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
