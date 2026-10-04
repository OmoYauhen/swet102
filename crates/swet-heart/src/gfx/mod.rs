//! Landscape 1-bpp framebuffer and drawing primitives (TECH_DESIGN §6).
//!
//! `Frame.0[y][x >> 3]`, bit `x & 7`, 1 = lit. This byte order is exactly what
//! the SH1107 expects when the 64 controller columns are sent as 64 rows of
//! 16 bytes, so the platform flushes it without any rotation.

#[rustfmt::skip]
pub mod assets;
pub mod font3x5;
pub mod num;

pub const W: i32 = 128;
pub const H: i32 = 64;

#[derive(Clone, PartialEq, Eq)]
pub struct Frame(pub [[u8; 16]; 64]);

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

/// A proportional bitmap font: one strip of glyphs, upright, row-major,
/// LSB = leftmost, 1 = lit. Glyph `i` spans columns `offsets[i]..offsets[i + 1]`.
pub struct Font {
    pub height: u8,
    pub stride: u16,
    pub chars: &'static [u8],
    pub offsets: &'static [u16],
    pub bits: &'static [u8],
}

/// A 1-bpp image in the same layout as [`Font`].
pub struct Image {
    pub w: u16,
    pub h: u16,
    pub stride: u16,
    pub bits: &'static [u8],
}

impl Font {
    fn glyph(&self, c: u8) -> Option<(i32, i32)> {
        let i = self.chars.iter().position(|&k| k == c)?;
        let x0 = i32::from(self.offsets[i]);
        Some((x0, i32::from(self.offsets[i + 1]) - x0))
    }

    /// Width of `s` in pixels with `gap` px between glyphs.
    pub fn width(&self, s: &[u8], gap: i32) -> i32 {
        let w: i32 = s
            .iter()
            .filter_map(|&c| self.glyph(c))
            .map(|(_, w)| w + gap)
            .sum();
        (w - gap).max(0)
    }
}

#[inline(always)]
fn apply(byte: &mut u8, mask: u8, mode: Mode) {
    match mode {
        Mode::Set => *byte |= mask,
        Mode::Clear => *byte &= !mask,
        Mode::Xor => *byte ^= mask,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Set,
    Clear,
    Xor,
}

impl Frame {
    pub const fn new() -> Self {
        Self([[0; 16]; 64])
    }

    pub fn clear(&mut self) {
        self.0 = [[0; 16]; 64];
    }

    pub fn get(&self, x: i32, y: i32) -> bool {
        if !(0..W).contains(&x) || !(0..H).contains(&y) {
            return false;
        }
        self.0[y as usize][(x >> 3) as usize] & (1 << (x & 7)) != 0
    }

    /// Plot one pixel; coordinates outside the screen are ignored.
    pub fn pixel(&mut self, x: i32, y: i32, mode: Mode) {
        if !(0..W).contains(&x) || !(0..H).contains(&y) {
            return;
        }
        let byte = &mut self.0[y as usize][(x >> 3) as usize];
        let bit = 1u8 << (x & 7);
        match mode {
            Mode::Set => *byte |= bit,
            Mode::Clear => *byte &= !bit,
            Mode::Xor => *byte ^= bit,
        }
    }

    /// Filled rectangle, clipped. Works a byte (8 px) at a time.
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, mode: Mode) {
        let (x0, x1) = (x.max(0), (x + w).min(W));
        let (y0, y1) = (y.max(0), (y + h).min(H));
        if x0 >= x1 {
            return;
        }
        for yy in y0..y1 {
            let row = &mut self.0[yy as usize];
            let mut cx = x0;
            while cx < x1 {
                let bit = cx & 7;
                let n = (8 - bit).min(x1 - cx);
                let mask = (((1u16 << n) - 1) << bit) as u8;
                apply(&mut row[(cx >> 3) as usize], mask, mode);
                cx += n;
            }
        }
    }

    pub fn hline(&mut self, x: i32, y: i32, w: i32, mode: Mode) {
        self.fill_rect(x, y, w, 1, mode);
    }

    pub fn vline(&mut self, x: i32, y: i32, h: i32, mode: Mode) {
        self.fill_rect(x, y, 1, h, mode);
    }

    /// 1 px outline.
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, mode: Mode) {
        self.hline(x, y, w, mode);
        self.hline(x, y + h - 1, w, mode);
        self.vline(x, y + 1, h - 2, mode);
        self.vline(x + w - 1, y + 1, h - 2, mode);
    }

    /// Copy a `w`×`h` block starting at column `sx` of a packed bitmap to (`dx`, `dy`).
    #[allow(clippy::too_many_arguments)]
    fn blit_bits(
        &mut self,
        bits: &[u8],
        stride: u16,
        sx: i32,
        w: i32,
        h: i32,
        dx: i32,
        dy: i32,
        mode: Mode,
    ) {
        let stride = usize::from(stride);
        // clip once: source columns i in [i0, i1), rows y in [y0, y1)
        let (i0, i1) = ((-dx).max(0), w.min(W - dx));
        let (y0, y1) = ((-dy).max(0), h.min(H - dy));
        if i0 >= i1 {
            return;
        }
        for y in y0..y1 {
            let src = &bits[y as usize * stride..(y as usize + 1) * stride];
            let dst = &mut self.0[(dy + y) as usize];
            let mut i = i0;
            while i < i1 {
                // up to 8 px that land in one destination byte
                let d = dx + i;
                let n = (8 - (d & 7)).min(i1 - i);
                let s = (sx + i) as usize;
                let lo = u16::from(src[s >> 3]);
                let hi = src.get((s >> 3) + 1).map_or(0, |&b| u16::from(b));
                let v = ((lo | hi << 8) >> (s & 7)) & ((1u16 << n) - 1);
                if v != 0 {
                    apply(&mut dst[(d >> 3) as usize], (v << (d & 7)) as u8, mode);
                }
                i += n;
            }
        }
    }

    pub fn image(&mut self, img: &Image, x: i32, y: i32, mode: Mode) {
        self.blit_bits(
            img.bits,
            img.stride,
            0,
            i32::from(img.w),
            i32::from(img.h),
            x,
            y,
            mode,
        );
    }

    /// Draw `s` with its top-left at (`x`, `y`). Unknown characters are skipped.
    /// Returns the x after the last glyph.
    pub fn text(&mut self, font: &Font, s: &[u8], x: i32, y: i32, gap: i32, mode: Mode) -> i32 {
        let mut cx = x;
        for &c in s {
            if let Some((sx, w)) = font.glyph(c) {
                self.blit_bits(
                    font.bits,
                    font.stride,
                    sx,
                    w,
                    i32::from(font.height),
                    cx,
                    y,
                    mode,
                );
                cx += w + gap;
            }
        }
        cx
    }

    /// Rounded-rectangle fill with corner radius `r` (r ≤ 4 looks right at this size).
    pub fn round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, mode: Mode) {
        for yy in 0..h {
            let dy = if yy < r {
                r - yy
            } else if yy >= h - r {
                yy - (h - r - 1)
            } else {
                0
            };
            // widest k with k² + dy² ≤ r²: the row reaches k px past the corner centre
            let mut k = r;
            while k > 0 && k * k + dy * dy > r * r {
                k -= 1;
            }
            let inset = r - k;
            self.hline(x + inset, y + yy, w - 2 * inset, mode);
        }
    }

    /// Text in the built-in 3×5 font, `scale`× magnified. Returns the x after the last glyph.
    pub fn text3x5(&mut self, x: i32, y: i32, s: &[u8], scale: i32, mode: Mode) -> i32 {
        let mut cx = x;
        for &c in s {
            let g = font3x5::glyph(c);
            for row in 0..5 {
                for col in 0..3 {
                    if g & (1 << ((4 - row) * 3 + (2 - col))) != 0 {
                        self.fill_rect(cx + col * scale, y + row * scale, scale, scale, mode);
                    }
                }
            }
            cx += 4 * scale;
        }
        cx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_maps_to_row_major_lsb_first() {
        let mut f = Frame::new();
        f.pixel(0, 0, Mode::Set);
        f.pixel(9, 1, Mode::Set);
        f.pixel(127, 63, Mode::Set);
        assert_eq!(f.0[0][0], 0b0000_0001);
        assert_eq!(f.0[1][1], 0b0000_0010);
        assert_eq!(f.0[63][15], 0b1000_0000);
    }

    #[test]
    fn out_of_bounds_is_ignored() {
        let mut f = Frame::new();
        f.pixel(-1, 0, Mode::Set);
        f.pixel(128, 0, Mode::Set);
        f.fill_rect(120, 60, 20, 20, Mode::Set);
        assert!(f.get(127, 63));
        assert!(!f.get(119, 63));
    }

    /// Pixel-at-a-time reference for the byte-wise fast paths.
    #[allow(clippy::too_many_arguments)]
    fn naive_blit(
        f: &mut Frame,
        bits: &[u8],
        stride: u16,
        sx: i32,
        w: i32,
        h: i32,
        dx: i32,
        dy: i32,
        mode: Mode,
    ) {
        let stride = usize::from(stride);
        for y in 0..h {
            for x in 0..w {
                let s = (sx + x) as usize;
                if bits[y as usize * stride + (s >> 3)] & (1 << (s & 7)) != 0 {
                    f.pixel(dx + x, dy + y, mode);
                }
            }
        }
    }

    fn background() -> Frame {
        let mut f = Frame::new();
        for y in 0..H {
            for x in 0..W {
                if (x * 7 + y * 3) % 5 < 2 {
                    f.pixel(x, y, Mode::Set);
                }
            }
        }
        f
    }

    #[test]
    fn fast_text_matches_pixel_reference() {
        use super::assets::{SPEED, W95};
        for font in [&W95, &SPEED] {
            for (dx, dy) in [
                (0, 0),
                (3, 5),
                (-7, -3),
                (110, 40),
                (5, 50),
                (-25, 10),
                (127, 63),
            ] {
                for mode in [Mode::Set, Mode::Clear, Mode::Xor] {
                    let mut fast = background();
                    let mut slow = fast.clone();
                    fast.text(font, font.chars, dx, dy, 1, mode);
                    let mut cx = dx;
                    for &c in font.chars {
                        let (sx, w) = font.glyph(c).expect("glyph");
                        naive_blit(
                            &mut slow,
                            font.bits,
                            font.stride,
                            sx,
                            w,
                            i32::from(font.height),
                            cx,
                            dy,
                            mode,
                        );
                        cx += w + 1;
                    }
                    assert!(fast == slow, "text differs at ({dx},{dy}) {mode:?}");
                }
            }
        }
    }

    #[test]
    fn fast_fill_matches_pixel_reference() {
        for (x, y, w, h) in [
            (0, 0, 128, 64),
            (3, 2, 1, 1),
            (5, 5, 11, 7),
            (-4, -4, 10, 10),
            (120, 60, 20, 20),
            (7, 9, 9, 3),
            (8, 0, 8, 64),
            (0, 0, 0, 5),
        ] {
            for mode in [Mode::Set, Mode::Clear, Mode::Xor] {
                let mut fast = background();
                let mut slow = fast.clone();
                fast.fill_rect(x, y, w, h, mode);
                for yy in y..y + h {
                    for xx in x..x + w {
                        slow.pixel(xx, yy, mode);
                    }
                }
                assert!(fast == slow, "fill differs for ({x},{y},{w},{h}) {mode:?}");
            }
        }
    }

    #[test]
    fn xor_toggles() {
        let mut f = Frame::new();
        f.fill_rect(0, 0, 4, 4, Mode::Set);
        f.fill_rect(2, 2, 4, 4, Mode::Xor);
        assert!(f.get(1, 1));
        assert!(!f.get(3, 3));
        assert!(f.get(5, 5));
    }
}
