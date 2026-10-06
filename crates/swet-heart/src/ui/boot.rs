//! Boot animation (PRODUCT §3.4): the sparkles, full height, then
//! `SWET102 v<version>` scrolling in from the right edge and out at the left.
//! Any button skips it. Purely a function of the time since power-on.

use crate::config;
use crate::gfx::assets::{SPARKLES, TEXT};
use crate::gfx::{Frame, H, Mode, W};

/// Sparkles on screen.
pub const SPARKLES_MS: u32 = 600;
/// Scroll speed of the version line (3 px per 20 ms frame).
pub const SCROLL_PX_PER_S: u32 = 150;

const NAME: &[u8] = b"SWET102 v";

fn text_width() -> i32 {
    TEXT.width(NAME, 1) + 1 + TEXT.width(config::VERSION.as_bytes(), 1)
}

/// Total length: sparkles, then until the text has left the screen.
pub fn duration_ms() -> u32 {
    SPARKLES_MS + (W + text_width()) as u32 * 1000 / SCROLL_PX_PER_S
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
    let moved = ((elapsed - SPARKLES_MS) * SCROLL_PX_PER_S / 1000) as i32;
    let x = W - moved;
    let y = (H - i32::from(TEXT.height)) / 2;
    let x = f.text(&TEXT, NAME, x, y, 1, Mode::Set);
    f.text(&TEXT, config::VERSION.as_bytes(), x + 1, y, 1, Mode::Set);
}
