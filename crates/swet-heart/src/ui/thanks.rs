//! Menu → Special thanks (PRODUCT §5): the people who helped, one name at a
//! time, as written (lowercase, `@`, `_`), in W95FA magnified to the largest
//! scale the longest name fits. LEFT/RIGHT step through them (wrapping), PWR
//! goes back. Names come and go with the boot animation's slides
//! (`boot::slide_in` / `slide_out`): the old name slides out, then the new
//! one slides in. Purely a function of the time since the step, so a late
//! tick lands where it should (TECH_DESIGN §4.4), and a new press snaps it.

use super::anim::Tween;
use super::boot::{SLIDE_IN_MS, SLIDE_OUT_MS, slide_in, slide_out};
use super::menu::{HEART, draw_art};
use crate::gfx::assets::W95_NICK;
use crate::gfx::{Frame, H, Mode, W};

/// In order; RIGHT goes down the list.
pub const NAMES: [&[u8]; 3] = [b"@entropia_pl", b"@yahor_k", b"@veganbunt"];

/// Font pixels between glyphs: W95FA's own spacing (2 px at 2×). One
/// screen pixel at 2× would leave more margin but run `tr` together.
const GAP: i32 = 1;
/// Free pixels left and right of the longest name, at least: with the
/// font's spacing, `@entropia_pl` at 2× is 126 px, so 1 (boot's 2 would
/// drop it to 1×).
const MARGIN: i32 = 1;
/// Rows kept for the heart above the name and the dots below it.
const EDGE: i32 = 14;

/// All zeroes = first name, nothing moving, so it can live in the
/// zero-initialised App (.bss).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Thanks {
    index: u8,
    /// The name sliding out while the step runs.
    prev: u8,
    /// A step from `prev` (out, then `index` in) rather than the opening
    /// (`index` in only).
    stepped: bool,
    /// LEFT: the slides mirrored (out to the right, in from the left).
    back: bool,
    tween: Tween,
}

impl Thanks {
    pub const fn new() -> Self {
        Self {
            index: 0,
            prev: 0,
            stepped: false,
            back: false,
            tween: Tween::IDLE,
        }
    }

    /// Opening always starts at the first name, sliding in like the boot text.
    pub fn open(&mut self, now: u32) {
        *self = Self::new();
        self.tween.start(now, SLIDE_IN_MS as u16);
    }

    /// LEFT (`forward` false) / RIGHT: the previous / next name, wrapping.
    pub fn step(&mut self, forward: bool, now: u32) {
        let n = NAMES.len() as u8;
        self.prev = self.index;
        self.index = (self.index + if forward { 1 } else { n - 1 }) % n;
        self.stepped = true;
        self.back = !forward;
        self.tween.start(now, (SLIDE_OUT_MS + SLIDE_IN_MS) as u16);
    }

    pub fn index(&self) -> usize {
        usize::from(self.index) % NAMES.len()
    }

    /// Jump to the end (input arrived, TECH_DESIGN §5.4).
    pub fn snap(&mut self) {
        self.tween.snap();
    }

    pub fn animating(&self, now: u32) -> bool {
        self.tween.running(now)
    }

    /// Which name is on screen, and its horizontal offset.
    fn showing(&self, now: u32) -> (usize, i32) {
        let Some(t) = self.tween.elapsed(now) else {
            return (self.index(), 0);
        };
        let mirror = if self.back { -1 } else { 1 };
        if !self.stepped {
            (self.index(), slide_in(t))
        } else if t < SLIDE_OUT_MS {
            (usize::from(self.prev) % NAMES.len(), mirror * slide_out(t))
        } else {
            (self.index(), mirror * slide_in(t - SLIDE_OUT_MS))
        }
    }

    pub fn render(&self, f: &mut Frame, now: u32) {
        draw_art(f, &HEART, (W - HEART[0].len() as i32) / 2, 3, 1);

        let (i, dx) = self.showing(now);
        let name = NAMES[i];
        let s = scale();
        let (top, bottom) = ink_rows();
        let y = (H - (bottom - top + 1) * s) / 2 - top * s;
        let x = (W - W95_NICK.width(name, GAP) * s) / 2 + dx;
        f.text_scaled(&W95_NICK, name, x, y, GAP, s, Mode::Set);

        // position dots, as in the menu; they follow the name on screen
        let n = NAMES.len() as i32;
        let x0 = (W - (n * 8 - 4)) / 2;
        for k in 0..n {
            let x = x0 + k * 8;
            if k == i as i32 {
                f.fill_rect(x, 57, 4, 4, Mode::Set);
            } else {
                f.rect(x, 57, 4, 4, Mode::Set);
            }
        }
    }
}

/// Lit rows of all the names together, so the baseline doesn't jump.
fn ink_rows() -> (i32, i32) {
    NAMES
        .iter()
        .filter_map(|s| W95_NICK.ink_rows(s))
        .fold((i32::MAX, 0), |(a, b), (t, u)| (a.min(t), b.max(u)))
}

/// The largest scale at which the longest name fits across the screen, and
/// the names between the heart and the dots; at least 1× (as boot's `scales`).
fn scale() -> i32 {
    let (top, bottom) = ink_rows();
    let widest = NAMES
        .iter()
        .map(|n| W95_NICK.width(n, GAP))
        .max()
        .unwrap_or(1);
    ((W - 2 * MARGIN) / widest)
        .min((H - 2 * EDGE) / (bottom - top + 1))
        .max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_drawable_and_the_longest_fits_at_twice_the_size() {
        for n in NAMES {
            assert!(
                n.iter().all(|&c| W95_NICK.chars.contains(&c)),
                "{}",
                core::str::from_utf8(n).unwrap_or("?")
            );
        }
        assert_eq!(scale(), 2);
        // the longest name: 52 px of ink + 11 gaps = 63 font px → 126 of 128
        assert_eq!(W95_NICK.width(b"@entropia_pl", GAP) * 2, 126);
        assert_eq!(ink_rows(), (0, 11), "24 px tall at 2×");
    }

    #[test]
    fn steps_wrap_both_ways_and_reopen_at_the_first_name() {
        let mut t = Thanks::new();
        t.open(0);
        t.step(false, 10);
        assert_eq!(t.index(), 2);
        t.step(true, 20);
        t.step(true, 30);
        assert_eq!(t.index(), 1);
        t.open(40);
        assert_eq!(t.index(), 0);
    }

    #[test]
    fn the_old_name_slides_out_then_the_new_one_in() {
        let mut t = Thanks::new();
        t.open(0);
        assert_eq!(t.showing(0), (0, W), "opening: in from the right");
        assert_eq!(t.showing(SLIDE_IN_MS), (0, 0));
        t.step(true, 1000);
        assert_eq!(t.showing(1000), (0, 0));
        let (i, dx) = t.showing(1000 + SLIDE_OUT_MS / 2);
        assert!(i == 0 && dx < 0, "out to the left: {dx}");
        assert_eq!(t.showing(1000 + SLIDE_OUT_MS), (1, W), "in from the right");
        assert_eq!(t.showing(1000 + SLIDE_OUT_MS + SLIDE_IN_MS), (1, 0));
        // LEFT: mirrored
        t.step(false, 3000);
        let (i, dx) = t.showing(3000 + SLIDE_OUT_MS / 2);
        assert!(i == 1 && dx > 0, "out to the right: {dx}");
        assert_eq!(t.showing(3000 + SLIDE_OUT_MS), (0, -W), "in from the left");
        t.snap();
        assert_eq!(t.showing(3001 + SLIDE_OUT_MS), (0, 0));
    }
}
