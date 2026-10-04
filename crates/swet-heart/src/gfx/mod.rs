//! Landscape 1-bpp framebuffer and drawing primitives (TECH_DESIGN §6).
//!
//! `Frame.0[y][x >> 3]`, bit `x & 7`, 1 = lit. This byte order is exactly what
//! the SH1107 expects when the 64 controller columns are sent as 64 rows of
//! 16 bytes, so the platform flushes it without any rotation.

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

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, mode: Mode) {
        let (x0, x1) = (x.max(0), (x + w).min(W));
        let (y0, y1) = (y.max(0), (y + h).min(H));
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.pixel(xx, yy, mode);
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
