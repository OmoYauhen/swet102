//! Swet102 application core. Everything the display does lives here; the
//! hardware is reached only through [`Hal`]. See docs/TECH_DESIGN.md.
#![no_std]
#![forbid(unsafe_code)]

pub mod config;
pub mod gfx;
pub mod hal;
mod testpattern;

pub use gfx::Frame;
pub use hal::{BleChannel, BleState, Buttons, Diag, Hal, STORE_LEN};

use testpattern::TestPattern;

pub struct App<H: Hal> {
    hal: H,
    frame: Frame,
    screen: TestPattern,
}

impl<H: Hal> App<H> {
    pub const fn new(hal: H) -> Self {
        Self {
            hal,
            frame: Frame::new(),
            screen: TestPattern::new(),
        }
    }

    pub fn init(&mut self, _now_ms: u32) {
        self.hal.display_orient(config::DISPLAY_ORIENT);
    }

    /// Called every 20 ms from the main loop only. Never blocks (TECH_DESIGN §4.4).
    pub fn tick(&mut self, now_ms: u32) {
        self.screen.tick(&mut self.hal, now_ms);
        self.frame.clear();
        self.screen.render(&mut self.frame, &self.hal, now_ms);
        self.hal.display_flush(&self.frame);
    }

    /// The phone wrote the control characteristic (TECH_DESIGN §9.5).
    pub fn ble_control(&mut self, _data: &[u8]) {}

    pub fn hal(&self) -> &H {
        &self.hal
    }

    pub fn hal_mut(&mut self) -> &mut H {
        &mut self.hal
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }
}
