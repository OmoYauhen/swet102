//! Boot animation (PRODUCT §3.4): the sparkles, full height, then
//! `SWET102 v<version>` as tall as the screen, scrolling in from the right
//! edge and out at the left. Any button skips it. Purely a function of the
//! time since power-on.

use crate::config;
use crate::gfx::assets::{SPARKLES, TEXT};
use crate::gfx::{Frame, H, Mode, W};

/// Sparkles on screen.
pub const SPARKLES_MS: u32 = 600;
/// Scroll speed of the version line (10 px per 20 ms frame).
pub const SCROLL_PX_PER_S: u32 = 500;

const NAME: &[u8] = b"SWET102 v";

/// Font pixels between glyphs.
const GAP: i32 = 1;

/// The text's lit rows fill the screen height: the scale, and the y that
/// puts the first lit row at the top edge.
fn layout() -> (i32, i32) {
    let (mut top, mut bottom) = TEXT
        .ink_rows(NAME)
        .unwrap_or((0, i32::from(TEXT.height) - 1));
    if let Some((t, b)) = TEXT.ink_rows(config::VERSION.as_bytes()) {
        top = top.min(t);
        bottom = bottom.max(b);
    }
    let scale = (H / (bottom - top + 1)).max(1);
    let ink = (bottom - top + 1) * scale;
    (scale, (H - ink) / 2 - top * scale)
}

fn text_width(scale: i32) -> i32 {
    (TEXT.width(NAME, GAP) + GAP + TEXT.width(config::VERSION.as_bytes(), GAP)) * scale
}

/// Total length: sparkles, then until the text has left the screen.
pub fn duration_ms() -> u32 {
    let (scale, _) = layout();
    SPARKLES_MS + (W + text_width(scale)) as u32 * 1000 / SCROLL_PX_PER_S
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
    let (scale, y) = layout();
    let moved = ((elapsed - SPARKLES_MS) * SCROLL_PX_PER_S / 1000) as i32;
    let x = f.text_scaled(&TEXT, NAME, W - moved, y, GAP, scale, Mode::Set);
    let v = config::VERSION.as_bytes();
    f.text_scaled(&TEXT, v, x, y, GAP, scale, Mode::Set);
}
