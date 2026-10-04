//! Minimal drawing on the window's 0RGB pixel buffer: rectangles, dots and
//! text in the firmware's own 13 px font (`swet_heart::gfx::assets::TEXT`),
//! so the panel needs no font crate.

use swet_heart::gfx::Font;
use swet_heart::gfx::assets::TEXT;

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
}

impl Canvas<'_> {
    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        let (x0, x1) = (x.max(0) as usize, ((x + w).max(0) as usize).min(self.w));
        let (y0, y1) = (y.max(0) as usize, ((y + h).max(0) as usize).min(self.h));
        for yy in y0..y1 {
            self.buf[yy * self.w + x0..yy * self.w + x1.max(x0)].fill(c);
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

    /// Text at (`x`, `y`) top-left; returns the x after it.
    pub fn text(&mut self, s: &str, x: i32, y: i32, c: u32) -> i32 {
        let font: &Font = &TEXT;
        let mut cx = x;
        for ch in s.bytes() {
            let Some(i) = font.chars.iter().position(|&k| k == ch) else {
                cx += 4;
                continue;
            };
            let (sx, ex) = (
                usize::from(font.offsets[i]),
                usize::from(font.offsets[i + 1]),
            );
            for row in 0..usize::from(font.height) {
                let line = &font.bits[row * usize::from(font.stride)..];
                for (col, s) in (sx..ex).enumerate() {
                    if line[s >> 3] & (1 << (s & 7)) != 0 {
                        self.fill(cx + col as i32, y + row as i32, 1, 1, c);
                    }
                }
            }
            cx += (ex - sx) as i32 + 1;
        }
        cx
    }

    pub fn text_width(s: &str) -> i32 {
        TEXT.width(s.as_bytes(), 1)
    }

    /// A key chip like "[W]": filled with `s.on` while held.
    pub fn chip(&mut self, label: &str, x: i32, y: i32, held: bool, s: Chip) -> i32 {
        let w = Self::text_width(label) + 10;
        if held {
            self.fill(x, y, w, 17, s.on);
            self.text(label, x + 5, y + 2, s.bg);
        } else {
            self.outline(x, y, w, 17, s.off);
            self.text(label, x + 5, y + 2, s.off);
        }
        x + w + 4
    }
}
