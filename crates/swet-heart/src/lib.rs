//! Swet102 application core. Everything the display does lives here; the
//! hardware is reached only through [`Hal`]. See docs/TECH_DESIGN.md.
#![no_std]
#![forbid(unsafe_code)]

pub mod config;
pub mod gfx;
pub mod hal;
pub mod input;
pub mod motor;
pub mod store;
pub mod ui;

pub use gfx::Frame;
pub use hal::{BleChannel, BleState, Buttons, Diag, Hal, STORE_LEN};

use input::{Btn, Event, Gesture, Input};
use motor::{Motor, codec};
use store::{Record, Saver};
use ui::diag::DiagData;
use ui::pin::PinEntry;
use ui::popup::{Fault, Popup, Popups};
use ui::ride::{Page, RideScreen, View};
use ui::{Model, Overlay, Scene, Screen, Stack};

/// Rider-controlled state. `pas`, `speed_limit` and `locked` are persisted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct State {
    pub pas: u8,
    pub walk: bool,
    pub lights: bool,
    /// km/h; set by the PIN that unlocked the bike (PRODUCT §6).
    pub speed_limit: u8,
    /// Ask for a PIN at the next power-on.
    pub locked: bool,
}

impl State {
    pub const fn is_sport(&self) -> bool {
        self.speed_limit > config::CITY_LIMIT_KMH
    }
}

/// Power-off sequence (TECH_DESIGN §8.3). `On` first so the App stays
/// zero-initialised.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Power {
    On,
    /// Padlock on screen; powers off after `LOCK_FLASH_MS`.
    Locking {
        since: u32,
    },
    /// Dark screen; cuts the power latch once the save is in flash (or after
    /// `SAVE_BEFORE_OFF_MS`).
    Off {
        since: u32,
    },
}

pub struct App<H: Hal> {
    hal: H,
    frame: Frame,
    input: Input,
    motor: Motor,
    stack: Stack,
    ride: RideScreen,
    pin: PinEntry,
    state: State,
    /// Last loaded/saved record; trip fields ride along unchanged until M4.
    rec: Record,
    saver: Saver,
    /// (pas, speed_limit, locked) as of the last save, to spot changes.
    saved: (u8, u8, bool),
    power: Power,
    last_activity: u32,
    orient: u8,
    last_buttons: Buttons,
    popups: Popups,
    /// The frame last sent to the display; flush only when it differs
    /// (the SPI flush is the most expensive thing on this chip, TECH_DESIGN §4.3).
    sent: Frame,
    sent_valid: bool,
}

impl<H: Hal> App<H> {
    /// Every field is zero here (the App lives in .bss, `make check`
    /// enforces it); real defaults are set in `init()`.
    pub const fn new(hal: H) -> Self {
        Self {
            hal,
            frame: Frame::new(),
            input: Input::new(),
            motor: Motor::new(),
            stack: Stack::new(Screen::Ride),
            ride: RideScreen::new(),
            pin: PinEntry::new(),
            state: State {
                pas: 0,
                walk: false,
                lights: false,
                speed_limit: 0,
                locked: false,
            },
            rec: Record {
                pas: 0,
                speed_limit: 0,
                locked: false,
                soc_min: 0,
                odo_m: 0,
                trip_m: 0,
                trip_moving_s: 0,
                trip_batt_m: 0,
                trip_batt_moving_s: 0,
                trip_max_x10: 0,
                trip_batt_max_x10: 0,
                trip_mah: 0,
                trip_batt_mah: 0,
                odo_max_x10: 0,
            },
            saver: Saver::new(),
            saved: (0, 0, false),
            power: Power::On,
            last_activity: 0,
            orient: 0,
            last_buttons: Buttons(0),
            popups: Popups::new(),
            sent: Frame::new(),
            sent_valid: false,
        }
    }

    pub fn init(&mut self, now: u32) {
        let mut buf = [0u8; STORE_LEN];
        let loaded = if self.hal.store_load(&mut buf) {
            Record::decode(&buf)
        } else {
            None
        };
        self.rec = loaded.unwrap_or(Record::defaults());
        self.state.pas = self.rec.pas;
        self.state.speed_limit = self.rec.speed_limit;
        self.state.locked = self.rec.locked;
        self.saved = self.settings();
        if self.state.locked {
            self.stack.reset(Screen::Pin);
        }
        self.last_activity = now;
        self.orient = config::DISPLAY_ORIENT;
        self.motor.walk_keepalive_ms = config::WALK_KEEPALIVE_MS;
        self.hal.display_orient(self.orient);
    }

    /// Called every 20 ms from the main loop only. Never blocks (TECH_DESIGN §4.4).
    pub fn tick(&mut self, now: u32) {
        // 1. input (ignored once power-off has started)
        let raw = self.hal.buttons();
        self.last_buttons = raw;
        let cfg = ui::gesture_cfg(self.stack.top(), &self.ride, self.popups.shown());
        let events = self.input.poll(raw, now, cfg);
        if self.power == Power::On {
            for ev in events.iter() {
                self.dispatch(ev, now);
            }
        }

        // 2. motor: push the wanted state, then run the bus. No assist while
        // locked (PRODUCT §6).
        let pas_code = if self.state.locked {
            codec::pas_code(0)
        } else if self.state.walk {
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

        // 3. persistence and power
        self.auto_off(raw, now);
        let settings = self.settings();
        if settings != self.saved {
            self.saved = settings;
            self.saver.changed(now);
        }
        let rec = self.record();
        self.saver.step(&mut self.hal, &rec, now);
        self.power_step(now);

        // 4. render, and flush only if something changed
        let model = self.model();
        let diag = self.diag_data();
        let overlay = match self.power {
            Power::On => Overlay::None,
            Power::Locking { .. } => Overlay::Padlock,
            Power::Off { .. } => Overlay::Dark,
        };
        let scene = Scene {
            top: self.stack.top(),
            ride: &self.ride,
            pin: &self.pin,
            model: &model,
            diag: &diag,
            popup: self.popups.shown(),
            overlay,
            now,
        };
        ui::render(&mut self.frame, &scene);
        if !self.sent_valid || self.frame != self.sent {
            self.hal.display_flush(&self.frame);
            self.sent.clone_from(&self.frame);
            self.sent_valid = true;
        }
    }

    fn settings(&self) -> (u8, u8, bool) {
        (self.state.pas, self.state.speed_limit, self.state.locked)
    }

    fn record(&self) -> Record {
        Record {
            pas: self.state.pas,
            speed_limit: self.state.speed_limit,
            locked: self.state.locked,
            ..self.rec
        }
    }

    /// Start the power-off sequence: save now, go dark, cut power once saved.
    fn power_off(&mut self, now: u32) {
        self.saver.now(now);
        self.power = Power::Off { since: now };
    }

    /// PWR double-click on the ride screen (PRODUCT §6).
    fn lock(&mut self, now: u32) {
        self.state.locked = true;
        self.state.walk = false;
        self.saver.now(now);
        self.power = Power::Locking { since: now };
    }

    fn power_step(&mut self, now: u32) {
        match self.power {
            Power::On => {}
            Power::Locking { since } => {
                if now.wrapping_sub(since) >= config::LOCK_FLASH_MS {
                    self.power_off(now);
                }
            }
            Power::Off { since } => {
                if self.saver.idle(&self.hal)
                    || now.wrapping_sub(since) >= config::SAVE_BEFORE_OFF_MS
                {
                    self.hal.power_off();
                }
            }
        }
    }

    /// Off after `AUTO_OFF_MS` with no wheel movement, no motor current and
    /// no buttons (PRODUCT §8).
    fn auto_off(&mut self, raw: Buttons, now: u32) {
        let v = &self.motor.values;
        let busy =
            raw.0 != 0 || v.rpm.is_some_and(|r| r > 0) || v.current_x2.is_some_and(|c| c > 0);
        if busy {
            self.last_activity = now;
        } else if self.power == Power::On
            && now.wrapping_sub(self.last_activity) >= config::AUTO_OFF_MS
        {
            self.power_off(now);
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
            self.power_off(now);
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
                (Btn::Pwr, Double) => self.lock(now),
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
            Screen::Pin => match (ev.btn, ev.g) {
                (Btn::Left, Click) => self.pin.down(),
                (Btn::Right, Click) => self.pin.up(),
                (Btn::Pwr, Click) => self.pin.backspace(),
                (Btn::M, Click) => {
                    if let Some(code) = self.pin.confirm() {
                        self.try_unlock(code, now);
                    }
                }
                _ => {}
            },
        }
    }

    /// The city PIN unlocks with the city limit, the sport PIN with the sport
    /// limit; locking and unlocking is how the mode changes (PRODUCT §6).
    fn try_unlock(&mut self, code: [u8; 4], now: u32) {
        let limit = if code == config::PIN_CITY {
            config::CITY_LIMIT_KMH
        } else if code == config::PIN_SPORT {
            config::SPORT_LIMIT_KMH
        } else {
            self.pin.rejected(now);
            return;
        };
        self.state.speed_limit = limit;
        self.state.locked = false;
        self.saver.now(now);
        self.stack.reset(Screen::Ride);
        self.ride.goto_pas();
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
            saves: self.saver.writes,
            store_errors: h.diag(Diag::StoreErrors),
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

    pub fn power(&self) -> Power {
        self.power
    }

    pub fn saves(&self) -> u32 {
        self.saver.writes
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
