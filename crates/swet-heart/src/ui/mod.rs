//! Screen stack and rendering (TECH_DESIGN §5). Screens are enum variants;
//! the App routes events to the top one (§5.5).

pub mod anim;
pub mod boot;
pub mod diag;
pub mod menu;
pub mod pin;
pub mod popup;
pub mod ride;

use crate::gfx::Frame;
use crate::input::GestureCfg;

// Explicit tag: `Ride` must be 0 so a fresh App is all zeroes (.bss).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Screen {
    Ride,
    /// Locked at power-on: enter a PIN to ride with assist.
    Pin,
    /// M hold: the menu (PRODUCT §5).
    Menu,
    Confirm(menu::Confirm),
    /// Detail screens opened from the menu.
    Diag,
    Ble,
    Firmware,
    /// Power-on animation; then `Ride` or `Pin` (PRODUCT §3.4).
    Boot,
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
    pub lights: bool,
    /// A phone is subscribed to commands: Player and Gate work.
    pub commands: bool,
    pub trip: crate::rides::Trip,
    pub batt: crate::rides::Trip,
    pub ride: crate::rides::Trip,
    pub odo_m: u32,
    pub odo_max_x10: u16,
}

/// Power-off overlay (TECH_DESIGN §8.3): the display goes dark the moment
/// power-off starts, while the last save finishes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Overlay {
    None,
    Padlock,
    Dark,
    /// Rebooting into the bootloader's update mode: what to do next. The
    /// bootloader doesn't drive the OLED, so this stays on screen while it
    /// keeps power.
    Update,
}

/// Everything one frame needs.
pub struct Scene<'a> {
    pub top: Screen,
    pub ride: &'a ride::RideScreen,
    pub pin: &'a pin::PinEntry,
    pub menu: &'a menu::Menu,
    pub ble: menu::BleInfo,
    pub model: &'a Model,
    pub diag: &'a diag::DiagData,
    pub popup: popup::Popup,
    pub overlay: Overlay,
    pub now: u32,
    /// Time since power-on, for the boot animation.
    pub boot_elapsed: u32,
}

pub fn gesture_cfg(top: Screen, ride: &ride::RideScreen, popup: popup::Popup) -> GestureCfg {
    if popup != popup::Popup::None {
        return GestureCfg::SIMPLE;
    }
    match top {
        Screen::Ride => ride.gesture_cfg(),
        _ => GestureCfg::SIMPLE,
    }
}

pub fn render(f: &mut Frame, s: &Scene) {
    f.clear();
    match s.overlay {
        Overlay::Dark => return,
        Overlay::Update => return menu::render_update(f),
        Overlay::Padlock => return pin::render_padlock(f),
        Overlay::None => {}
    }
    if s.popup != popup::Popup::None {
        return popup::render(f, s.popup); // full screen, covers everything
    }
    match s.top {
        Screen::Ride => s.ride.render(f, s.model, s.now),
        Screen::Pin => s.pin.render(f, s.now),
        Screen::Menu => s.menu.render(f, s.now),
        Screen::Confirm(c) => menu::render_confirm(f, c),
        Screen::Diag => diag::render(f, s.diag),
        Screen::Ble => menu::render_ble(f, &s.ble),
        Screen::Firmware => menu::render_firmware(f),
        Screen::Boot => boot::render(f, s.boot_elapsed),
    }
}
