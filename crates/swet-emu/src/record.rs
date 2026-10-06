//! GIF capture of the display (TECH_DESIGN §11.3, F11): two colours, every
//! distinct frame with its real duration, so animations play at true speed.

use std::borrow::Cow;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use swet_heart::Frame;
use swet_heart::gfx::{H, W};

/// Pixels per OLED pixel in the GIF.
pub const SCALE: usize = 4;
/// How long the final frame stays up before the loop restarts.
const LAST_FRAME_CS: u16 = 150;

pub struct Recorder {
    enc: gif::Encoder<BufWriter<File>>,
    /// The frame on screen and when it appeared; written once the next differs.
    pending: Option<(Frame, u32)>,
    pub frames: u32,
}

impl Recorder {
    pub fn create(path: &Path) -> Result<Self, String> {
        let file = File::create(path).map_err(|e| e.to_string())?;
        // palette: 0 = dark, 1 = lit (white with a hint of blue, like the window)
        let palette = [0x08, 0x0A, 0x0C, 0xE8, 0xF4, 0xFF];
        let (w, h) = ((W as usize * SCALE) as u16, (H as usize * SCALE) as u16);
        let mut enc =
            gif::Encoder::new(BufWriter::new(file), w, h, &palette).map_err(|e| e.to_string())?;
        enc.set_repeat(gif::Repeat::Infinite)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            enc,
            pending: None,
            frames: 0,
        })
    }

    /// Offer the current screen at `now_ms`; only changes are recorded.
    pub fn push(&mut self, frame: &Frame, now_ms: u32) -> Result<(), String> {
        match &self.pending {
            Some((f, _)) if f == frame => Ok(()),
            Some((f, t)) => {
                let (f, t) = (f.clone(), *t);
                self.write(&f, cs(now_ms - t))?;
                self.pending = Some((frame.clone(), now_ms));
                Ok(())
            }
            None => {
                self.pending = Some((frame.clone(), now_ms));
                Ok(())
            }
        }
    }

    /// Write the last frame and close the file.
    pub fn finish(mut self) -> Result<u32, String> {
        if let Some((f, _)) = self.pending.take() {
            self.write(&f, LAST_FRAME_CS)?;
        }
        Ok(self.frames)
    }

    fn write(&mut self, frame: &Frame, delay_cs: u16) -> Result<(), String> {
        let (w, h) = (W as usize * SCALE, H as usize * SCALE);
        let mut idx = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                if frame.get((x / SCALE) as i32, (y / SCALE) as i32) {
                    idx[y * w + x] = 1;
                }
            }
        }
        let gf = gif::Frame {
            width: w as u16,
            height: h as u16,
            delay: delay_cs,
            buffer: Cow::Owned(idx),
            ..gif::Frame::default()
        };
        self.enc.write_frame(&gf).map_err(|e| e.to_string())?;
        self.frames += 1;
        Ok(())
    }
}

/// ms → GIF centiseconds (at least 2: viewers slow down shorter delays).
fn cs(ms: u32) -> u16 {
    (ms / 10).clamp(2, u32::from(u16::MAX)) as u16
}
