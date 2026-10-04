//! Screen stack and rendering (TECH_DESIGN §5). Screens are enum variants;
//! the App routes events to the top one (§5.5).

pub mod diag;
pub mod pin;
pub mod popup;
pub mod ride;

use crate::gfx::Frame;
use crate::input::GestureCfg;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Screen {
    Ride,
    /// Platform + motor diagnostics. Until the menu exists (M4), M hold opens it.
    Diag,
    /// Locked at power-on: enter a PIN to ride with assist.
    Pin,
}

const DEPTH: usize = 4;

pub struct Stack {
    items: [Screen; DEPTH],
    /// Index of the top screen; `items[0]` is the base. Starts at 0 so a
    /// fresh App is all zeroes (.bss).
    top: usize,
}

impl Stack {
    pub const fn new(base: Screen) -> Self {
        Self {
            items: [base; DEPTH],
            top: 0,
        }
    }

    pub fn top(&self) -> Screen {
        self.items[self.top]
    }

    pub fn push(&mut self, s: Screen) {
        if self.top + 1 < DEPTH {
            self.top += 1;
            self.items[self.top] = s;
        }
    }

    pub fn pop(&mut self) {
        self.top = self.top.saturating_sub(1);
    }

    /// Replace the whole stack with one base screen (boot, unlock).
    pub fn reset(&mut self, base: Screen) {
        self.items[0] = base;
        self.top = 0;
    }
}

/// Read-only data the screens draw. Built once per frame by the App.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Model {
    pub link_up: bool,
    pub speed_x10: Option<u16>,
    pub power_w: Option<u16>,
    pub soc: Option<u8>,
    pub pas: u8,
    pub walk: bool,
    pub sport: bool,
}

/// Power-off overlay (TECH_DESIGN §8.3): the display goes dark the moment
/// power-off starts, while the last save finishes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Overlay {
    None,
    Padlock,
    Dark,
}

/// Everything one frame needs.
pub struct Scene<'a> {
    pub top: Screen,
    pub ride: &'a ride::RideScreen,
    pub pin: &'a pin::PinEntry,
    pub model: &'a Model,
    pub diag: &'a diag::DiagData,
    pub popup: popup::Popup,
    pub overlay: Overlay,
    pub now: u32,
}

pub fn gesture_cfg(top: Screen, ride: &ride::RideScreen, popup: popup::Popup) -> GestureCfg {
    if popup != popup::Popup::None {
        return GestureCfg::SIMPLE;
    }
    match top {
        Screen::Ride => ride.gesture_cfg(),
        Screen::Diag | Screen::Pin => GestureCfg::SIMPLE,
    }
}

pub fn render(f: &mut Frame, s: &Scene) {
    f.clear();
    match s.overlay {
        Overlay::Dark => return,
        Overlay::Padlock => return pin::render_padlock(f),
        Overlay::None => {}
    }
    if s.popup != popup::Popup::None {
        return popup::render(f, s.popup); // full screen, covers everything
    }
    match s.top {
        Screen::Ride => s.ride.render(f, s.model),
        Screen::Diag => diag::render(f, s.diag),
        Screen::Pin => s.pin.render(f, s.now),
    }
}
