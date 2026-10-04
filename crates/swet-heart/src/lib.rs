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
        }
    }

    pub fn init(&mut self, _now_ms: u32) {
        // Defaults until persistence (M3) loads the saved state here.
        self.state.speed_limit = config::CITY_LIMIT_KMH;
        self.orient = config::DISPLAY_ORIENT;
        self.hal.display_orient(self.orient);
    }

    /// Called every 20 ms from the main loop only. Never blocks (TECH_DESIGN §4.4).
    pub fn tick(&mut self, now: u32) {
        // 1. input
        let raw = self.hal.buttons();
        self.last_buttons = raw;
        let cfg = ui::gesture_cfg(self.stack.top(), &self.ride);
        let events = self.input.poll(raw, now, cfg);
        for ev in events.iter() {
            self.dispatch(ev);
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

        // 3. render
        let model = self.model();
        let diag = self.diag_data();
        ui::render(&mut self.frame, self.stack.top(), &self.ride, &model, &diag);
        self.hal.display_flush(&self.frame);
    }

    /// Event routing (TECH_DESIGN §5.5): global first, then the top screen,
    /// and on the ride screen LEFT/RIGHT go to the current page.
    fn dispatch(&mut self, ev: Event) {
        use Gesture::*;
        if ev.btn == Btn::Pwr && ev.g == Hold {
            self.hal.power_off();
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
            motor: self.motor.diag,
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

    pub fn page(&self) -> Page {
        self.ride.page
    }

    pub fn view(&self) -> View {
        self.ride.view
    }

    pub fn motor(&self) -> &Motor {
        &self.motor
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
