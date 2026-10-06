//! Boot animation (PRODUCT §3.4): the sparkles, full height, then two rows,
//! `SWET102` over a smaller `v<version>`, both magnified. They slide in from
//! the right, stay put long enough to read, and slide out to the left. Any
//! button skips it. Purely a function of the time since power-on.

use super::anim::{ONE, travel};
use crate::config;
use crate::gfx::assets::{SPARKLES, TEXT};
use crate::gfx::{Frame, H, Mode, W};

/// Sparkles on screen.
pub const SPARKLES_MS: u32 = 600;
/// The two rows slide in, hold still, slide out.
pub const SLIDE_IN_MS: u32 = 400;
pub const HOLD_MS: u32 = 1200;
pub const SLIDE_OUT_MS: u32 = 400;

const NAME: &[u8] = b"SWET102";
/// Font pixels between glyphs.
const GAP: i32 = 1;
/// Free pixels left and right of the widest row.
const MARGIN: i32 = 2;
/// Screen pixels between the two rows.
const ROW_GAP: i32 = 6;

/// Total length of the animation.
pub fn duration_ms() -> u32 {
    SPARKLES_MS + SLIDE_IN_MS + HOLD_MS + SLIDE_OUT_MS
}

const VERSION_PREFIX: &[u8] = b"v";

/// Lit rows of `s` in `TEXT` (all rows if it has none).
fn ink(s: &[u8]) -> (i32, i32) {
    TEXT.ink_rows(s).unwrap_or((0, i32::from(TEXT.height) - 1))
}

/// Width of the version row `v<version>` in font pixels.
fn version_width() -> i32 {
    TEXT.width(VERSION_PREFIX, GAP) + GAP + TEXT.width(config::VERSION.as_bytes(), GAP)
}

/// Scales of the two rows: the name as large as fits across the screen (and
/// two rows down it); the version a step smaller, smaller still if it needs to
/// be to fit, at least 1×.
fn scales() -> (i32, i32) {
    let (top, bottom) = ink(NAME);
    let room = W - 2 * MARGIN;
    let name = (room / TEXT.width(NAME, GAP))
        .min((H - ROW_GAP) / (2 * (bottom - top + 1)))
        .max(1);
    let version = (name - 1).min(room / version_width()).max(1);
    (name, version)
}

/// Horizontal offset of the rows: in from the right (ease-out), still, then
/// out to the left (ease-in).
fn offset(t: u32) -> i32 {
    let p = |t: u32, dur: u32| (t.min(dur) * ONE as u32 / dur) as i32; // 0..ONE
    if t < SLIDE_IN_MS {
        let rest = ONE - p(t, SLIDE_IN_MS);
        travel(W, rest * rest / ONE)
    } else if t < SLIDE_IN_MS + HOLD_MS {
        0
    } else {
        let q = p(t - SLIDE_IN_MS - HOLD_MS, SLIDE_OUT_MS);
        -travel(W, q * q / ONE)
    }
}

pub fn render(f: &mut Frame, elapsed: u32) {
    if elapsed < SPARKLES_MS {
        f.image(
            &SPARKLES,
            (W - i32::from(SPARKLES.w)) / 2,
            (H - i32::from(SPARKLES.h)) / 2,
            Mode::Set,
        );
        return;
    }
    let (s1, s2) = scales();
    let ((t1, b1), (t2, b2)) = (ink(NAME), ink(config::VERSION.as_bytes()));
    let (h1, h2) = ((b1 - t1 + 1) * s1, (b2 - t2 + 1) * s2);
    let top = (H - h1 - ROW_GAP - h2) / 2;
    let (y1, y2) = (top - t1 * s1, top + h1 + ROW_GAP - t2 * s2);
    let dx = offset(elapsed - SPARKLES_MS);

    let name_w = TEXT.width(NAME, GAP);
    f.text_scaled(
        &TEXT,
        NAME,
        (W - name_w * s1) / 2 + dx,
        y1,
        GAP,
        s1,
        Mode::Set,
    );

    let x = (W - version_width() * s2) / 2 + dx;
    let x = f.text_scaled(&TEXT, VERSION_PREFIX, x, y2, GAP, s2, Mode::Set);
    f.text_scaled(&TEXT, config::VERSION.as_bytes(), x, y2, GAP, s2, Mode::Set);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_smaller_than_the_name_and_both_fit() {
        let (name, version) = scales();
        assert!(name >= 2, "name {name}×");
        assert!(version < name, "version {version}×");
        assert!(TEXT.width(NAME, GAP) * name <= W);
        assert!(version_width() * version <= W, "the v always fits");
    }

    #[test]
    fn slides_in_holds_still_and_slides_out() {
        assert_eq!(offset(0), W);
        assert!(
            offset(SLIDE_IN_MS / 2) < W / 2,
            "ease-out: mostly in by half time"
        );
        assert_eq!(offset(SLIDE_IN_MS), 0);
        assert_eq!(offset(SLIDE_IN_MS + HOLD_MS - 1), 0);
        assert_eq!(offset(SLIDE_IN_MS + HOLD_MS + SLIDE_OUT_MS), -W);
    }
}
