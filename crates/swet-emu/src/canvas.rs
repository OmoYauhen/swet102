//! Minimal drawing on the window's 0RGB pixel buffer: rectangles, dots and
//! text. Text uses W95FA (assets/fonts, SIL OFL 1.1, the same pixel font as the
//! PAS digits), rasterised once at start-up exactly on its pixel grid and
//! scaled ×2, so it stays crisp and readable next to the 6× display.

// Desktop-only font rasterising: floats are fine here (the workspace denies
// them for the firmware core).
#![allow(clippy::float_arithmetic)]

use ab_glyph::{Font as _, FontRef, PxScale, ScaleFont as _, point};

static W95FA: &[u8] = include_bytes!("../../../assets/fonts/W95FA.otf");
/// W95FA design grid: 80 font units per pixel, glyph x offset 50 units.
const UNIT: f32 = 80.0;
const X_ORIGIN: f32 = 50.0;
/// Screen pixels per font pixel.
const TEXT_SCALE: f32 = 2.0;

struct Glyph {
    w: usize,
    h: usize,
    /// Offset of the bitmap from (pen x, line top).
    dx: i32,
    dy: i32,
    px: Vec<bool>,
    advance: i32,
}

/// W95FA glyphs for printable ASCII.
pub struct UiFont {
    glyphs: Vec<Option<Glyph>>,
    /// Height of one text line.
    pub height: i32,
}

impl UiFont {
    pub fn load() -> Self {
        let font = FontRef::try_from_slice(W95FA).expect("W95FA.otf is a valid font");
        // PxScale is the pixel height of ascent − descent: this puts one font
        // pixel on exactly TEXT_SCALE screen pixels.
        let px = font.height_unscaled() / UNIT * TEXT_SCALE;
        let scaled = font.as_scaled(PxScale::from(px));
        let ascent = scaled.ascent().round();
        let glyphs = (0u8..128)
            .map(|c| {
                let ch = c as char;
                if !ch.is_ascii_graphic() && ch != ' ' {
                    return None;
                }
                let mut g = scaled.scaled_glyph(ch);
                g.position = point(-X_ORIGIN / UNIT * TEXT_SCALE, ascent);
                let advance = scaled.h_advance(g.id).round() as i32;
                let Some(outline) = font.outline_glyph(g) else {
                    return Some(Glyph {
                        w: 0,
                        h: 0,
                        dx: 0,
                        dy: 0,
                        px: Vec::new(),
                        advance,
                    });
                };
                let b = outline.px_bounds();
                let (w, h) = (b.width() as usize, b.height() as usize);
                let mut bits = vec![false; w * h];
                outline.draw(|x, y, v| {
                    if v >= 0.5 && (x as usize) < w && (y as usize) < h {
                        bits[y as usize * w + x as usize] = true;
                    }
                });
                Some(Glyph {
                    w,
                    h,
                    dx: b.min.x as i32,
                    dy: b.min.y as i32,
                    px: bits,
                    advance,
                })
            })
            .collect();
        Self {
            glyphs,
            height: scaled.height().round() as i32,
        }
    }

    fn glyph(&self, c: u8) -> Option<&Glyph> {
        self.glyphs.get(usize::from(c)).and_then(Option::as_ref)
    }

    pub fn width(&self, s: &str) -> i32 {
        s.bytes()
            .map(|c| self.glyph(c).map_or(0, |g| g.advance))
            .sum()
    }
}

/// Colours of a key chip: fill when held, outline/text when not, background.
#[derive(Clone, Copy)]
pub struct Chip {
    pub on: u32,
    pub off: u32,
    pub bg: u32,
}

pub struct Canvas<'a> {
    pub buf: &'a mut [u32],
    pub w: usize,
    pub h: usize,
    pub font: &'a UiFont,
}

impl Canvas<'_> {
    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        let (x0, x1) = (x.max(0) as usize, ((x + w).max(0) as usize).min(self.w));
        let (y0, y1) = (y.max(0) as usize, ((y + h).max(0) as usize).min(self.h));
        if x0 >= x1 {
            return;
        }
        for yy in y0..y1 {
            self.buf[yy * self.w + x0..yy * self.w + x1].fill(c);
        }
    }

    pub fn outline(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        self.fill(x, y, w, 1, c);
        self.fill(x, y + h - 1, w, 1, c);
        self.fill(x, y, 1, h, c);
        self.fill(x + w - 1, y, 1, h, c);
    }

    pub fn dot(&mut self, cx: i32, cy: i32, r: i32, c: u32) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    self.fill(cx + dx, cy + dy, 1, 1, c);
                }
            }
        }
    }

    /// Text with its line top at (`x`, `y`); returns the x after it.
    pub fn text(&mut self, s: &str, x: i32, y: i32, c: u32) -> i32 {
        let font = self.font;
        let mut cx = x;
        for ch in s.bytes() {
            let Some(g) = font.glyph(ch) else { continue };
            for gy in 0..g.h {
                for gx in 0..g.w {
                    if g.px[gy * g.w + gx] {
                        self.fill(cx + g.dx + gx as i32, y + g.dy + gy as i32, 1, 1, c);
                    }
                }
            }
            cx += g.advance;
        }
        cx
    }

    pub fn text_width(&self, s: &str) -> i32 {
        self.font.width(s)
    }

    /// A key chip like "W": filled with `s.on` while held.
    pub fn chip(&mut self, label: &str, x: i32, y: i32, held: bool, s: Chip) -> i32 {
        let (w, h) = (self.text_width(label) + 12, self.font.height + 4);
        if held {
            self.fill(x, y, w, h, s.on);
            self.text(label, x + 6, y + 2, s.bg);
        } else {
            self.outline(x, y, w, h, s.off);
            self.text(label, x + 6, y + 2, s.off);
        }
        x + w + 5
    }
}
