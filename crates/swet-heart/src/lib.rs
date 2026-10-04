//! Swet102 application core. Everything the display does lives here; the
//! hardware is reached only through [`Hal`]. See docs/TECH_DESIGN.md.
#![no_std]
#![forbid(unsafe_code)]

pub mod config;
pub mod gfx;
pub mod hal;
pub mod input;
pub mod motor;
pub mod ui;

pub use gfx::Frame;
pub use hal::{BleChannel, BleState, Buttons, Diag, Hal, STORE_LEN};

use input::{Btn, Event, Gesture, Input};
use motor::{Motor, codec};
use ui::diag::DiagData;
use ui::popup::{Fault, Popup, Popups};
use ui::ride::{Page, RideScreen, View};
use ui::{Model, Screen, Stack};

/// Rider-controlled state. Persisted from M3 on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct State {
    pub pas: u8,
    pub walk: bool,
    pub lights: bool,
    pub speed_limit: u8,
}

impl State {
    pub const fn is_sport(&self) -> bool {
        self.speed_limit > config::CITY_LIMIT_KMH
    }
}

pub struct App<H: Hal> {
    hal: H,
    frame: Frame,
    input: Input,
    motor: Motor,
    stack: Stack,
    ride: RideScreen,
    state: State,
    orient: u8,
    last_buttons: Buttons,
    popups: Popups,
    /// The frame last sent to the display; flush only when it differs
    /// (the SPI flush costs ~8 ms on this chip, TECH_DESIGN §4.3).
    sent: Frame,
    sent_valid: bool,
}

impl<H: Hal> App<H> {
    pub const fn new(hal: H) -> Self {
        Self {
            hal,
            frame: Frame::new(),
            input: Input::new(),
            motor: Motor::new(),
            stack: Stack::new(Screen::Ride),
            ride: RideScreen::new(),
            state: State {
                pas: 0,
                walk: false,
                lights: false,
                // set in init(): keeps the whole App zero-initialised (.bss, not .data)
                speed_limit: 0,
            },
            orient: 0, // set in init(), see speed_limit
            last_buttons: Buttons(0),
            popups: Popups::new(),
            sent: Frame::new(),
            sent_valid: false,
        }
    }

    pub fn init(&mut self, _now_ms: u32) {
        // Defaults until persistence (M3) loads the saved state here.
        self.state.speed_limit = config::CITY_LIMIT_KMH;
        self.orient = config::DISPLAY_ORIENT;
        self.motor.walk_keepalive_ms = config::WALK_KEEPALIVE_MS;
        self.hal.display_orient(self.orient);
    }

    /// Called every 20 ms from the main loop only. Never blocks (TECH_DESIGN §4.4).
    pub fn tick(&mut self, now: u32) {
        // 1. input
        let raw = self.hal.buttons();
        self.last_buttons = raw;
        let cfg = ui::gesture_cfg(self.stack.top(), &self.ride, self.popups.shown());
        let events = self.input.poll(raw, now, cfg);
        for ev in events.iter() {
            self.dispatch(ev, now);
        }

        // 2. motor: push the wanted state, then run the bus
        let pas_code = if self.state.walk {
            codec::PAS_WALK
        } else {
            codec::pas_code(self.state.pas)
        };
        self.motor.set_pas_code(pas_code);
        self.motor.set_lights(self.state.lights);
        self.motor.set_speed_limit_kmh(self.state.speed_limit);
        self.motor.step(&mut self.hal, now);
        let fault = self.fault(now);
        self.popups.update(fault, now);

        // 3. render, and flush only if something changed
        let model = self.model();
        let diag = self.diag_data();
        let (top, popup) = (self.stack.top(), self.popups.shown());
        ui::render(&mut self.frame, top, &self.ride, &model, &diag, popup);
        if !self.sent_valid || self.frame != self.sent {
            self.hal.display_flush(&self.frame);
            self.sent.clone_from(&self.frame);
            self.sent_valid = true;
        }
    }

    /// Link loss beats an error code (no link, no status). At power-on the
    /// controller gets the same grace period as a dropped link.
    fn fault(&self, now: u32) -> Option<Fault> {
        if !self.motor.link_up() {
            (now >= config::MOTOR_LINK_TIMEOUT_MS).then_some(Fault::LinkLost)
        } else {
            self.motor.error().map(Fault::Code)
        }
    }

    /// Event routing (TECH_DESIGN §5.5): global first, then the top screen,
    /// and on the ride screen LEFT/RIGHT go to the current page.
    fn dispatch(&mut self, ev: Event, now: u32) {
        use Gesture::*;
        if ev.btn == Btn::Pwr && ev.g == Hold {
            self.hal.power_off();
            return;
        }
        if self.popups.shown() != Popup::None {
            // the error screen takes all input; M acknowledges it
            if (ev.btn, ev.g) == (Btn::M, Click) {
                self.popups.dismiss(now);
            }
            return;
        }
        match self.stack.top() {
            Screen::Ride => match (ev.btn, ev.g) {
                (Btn::M, Click) => self.ride.next_page(),
                (Btn::M, Double) => self.ride.next_view(),
                (Btn::M, Hold) => self.stack.push(Screen::Diag), // menu comes in M4
                (Btn::Pwr, Click) => self.ride.goto_pas(),
                (Btn::Pwr, Double) => {} // lock comes in M3
                (Btn::Left | Btn::Right, _) => self.page_event(ev),
                _ => {}
            },
            Screen::Diag => match (ev.btn, ev.g) {
                (Btn::M, Click) => {
                    self.orient = (self.orient + 1) & 3;
                    self.hal.display_orient(self.orient);
                    self.sent_valid = false; // the panel re-reads its RAM in the new order
                }
                (Btn::Pwr, Click) => self.stack.pop(),
                _ => {}
            },
        }
    }

    fn page_event(&mut self, ev: Event) {
        use Gesture::*;
        let s = &mut self.state;
        match self.ride.page {
            Page::Pas => match (ev.btn, ev.g) {
                (Btn::Left, Click) => s.pas = s.pas.saturating_sub(1),
                (Btn::Right, Click) => s.pas = (s.pas + 1).min(config::PAS_MAX),
                (Btn::Left, Hold) if s.pas == 0 => s.walk = true,
                (Btn::Left, HoldEnd) => s.walk = false,
                _ => {}
            },
        }
    }

    fn model(&self) -> Model {
        let v = &self.motor.values;
        Model {
            link_up: self.motor.link_up(),
            speed_x10: v.rpm.map(codec::rpm_to_kph_x10),
            power_w: v
                .current_x2
                .map(|c| (u32::from(c) * config::PACK_V / 2) as u16),
            soc: v.soc,
            pas: self.state.pas,
            walk: self.state.walk,
            sport: self.state.is_sport(),
        }
    }

    fn diag_data(&self) -> DiagData {
        let h = &self.hal;
        DiagData {
            orient: self.orient,
            buttons: self.last_buttons,
            tick_avg_us: h.diag(Diag::TickAvgUs),
            tick_max_us: h.diag(Diag::TickMaxUs),
            missed: h.diag(Diag::MissedTicks),
            stack_free: h.diag(Diag::StackFreeBytes),
            ram_kb: h.diag(Diag::RamKb),
            sd_ram_base: h.diag(Diag::SdRamBase),
            uart_errors: h.diag(Diag::UartErrors),
            lcd_avg_us: h.diag(Diag::FlushAvgUs),
            lcd_max_us: h.diag(Diag::FlushMaxUs),
            motor: self.motor.diag,
            values: self.motor.values,
        }
    }

    /// The phone wrote the control characteristic (TECH_DESIGN §9.5).
    pub fn ble_control(&mut self, _data: &[u8]) {}

    // --- read-only accessors for the simulator and tests ---

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn screen(&self) -> Screen {
        self.stack.top()
    }

    pub fn popup(&self) -> Popup {
        self.popups.shown()
    }

    pub fn page(&self) -> Page {
        self.ride.page
    }

    pub fn view(&self) -> View {
        self.ride.view
    }

    pub fn motor(&self) -> &Motor {
        &self.motor
    }

    /// For the simulator: flip config-like motor knobs (e.g. walk keep-alive).
    pub fn motor_mut(&mut self) -> &mut Motor {
        &mut self.motor
    }

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
