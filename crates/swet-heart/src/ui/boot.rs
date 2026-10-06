//! Boot animation (PRODUCT §3.4): the sparkles, full height, then two rows,
//! `SWET102` over the version, magnified. They slide in from the right, stay
//! put long enough to read, and slide out to the left. Any button skips it.
//! Purely a function of the time since power-on.

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

/// The version row: `v0.1.0`, or just `0.1.0-dev` when the `v` would make it
/// wider than `max_w` font pixels.
fn version_row(max_w: i32) -> (&'static [u8], &'static [u8]) {
    let v = config::VERSION.as_bytes();
    if TEXT.width(v, GAP) + GAP + TEXT.width(b"v", GAP) <= max_w {
        (b"v", v)
    } else {
        (b"", v)
    }
}

fn width(parts: (&[u8], &[u8])) -> i32 {
    match parts {
        (b"", v) => TEXT.width(v, GAP),
        (p, v) => TEXT.width(p, GAP) + GAP + TEXT.width(v, GAP),
    }
}

/// Both rows share one scale: the largest that fits the name across the
/// screen and the two rows down it.
fn scale() -> i32 {
    let (top, bottom) = TEXT
        .ink_rows(NAME)
        .unwrap_or((0, i32::from(TEXT.height) - 1));
    let ink = bottom - top + 1;
    let by_width = (W - 2 * MARGIN) / TEXT.width(NAME, GAP);
    let by_height = (H - ROW_GAP) / (2 * ink);
    by_width.min(by_height).max(1)
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
    let s = scale();
    let (top, bottom) = TEXT
        .ink_rows(NAME)
        .unwrap_or((0, i32::from(TEXT.height) - 1));
    let row_h = (bottom - top + 1) * s;
    let y1 = (H - 2 * row_h - ROW_GAP) / 2 - top * s;
    let y2 = y1 + row_h + ROW_GAP;
    let dx = offset(elapsed - SPARKLES_MS);

    let name_w = TEXT.width(NAME, GAP);
    f.text_scaled(
        &TEXT,
        NAME,
        (W - name_w * s) / 2 + dx,
        y1,
        GAP,
        s,
        Mode::Set,
    );

    let parts = version_row((W - 2 * MARGIN) / s);
    let x = (W - width(parts) * s) / 2 + dx;
    let x = if parts.0.is_empty() {
        x
    } else {
        f.text_scaled(&TEXT, parts.0, x, y2, GAP, s, Mode::Set)
    };
    f.text_scaled(&TEXT, parts.1, x, y2, GAP, s, Mode::Set);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_fit_the_screen() {
        let s = scale();
        let name_w = TEXT.width(NAME, GAP);
        assert!(s >= 2, "scale {s}");
        assert!(name_w * s <= W);
        assert!(width(version_row((W - 2 * MARGIN) / s)) * s <= W);
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
