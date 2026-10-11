//! Swet102 application core. Everything the display does lives here; the
//! hardware is reached only through [`Hal`]. See docs/TECH_DESIGN.md.
#![no_std]
#![forbid(unsafe_code)]

pub mod blep;
pub mod config;
pub mod gfx;
pub mod hal;
pub mod input;
pub mod motor;
pub mod rides;
pub mod store;
pub mod ui;

pub use gfx::Frame;
pub use hal::{BleChannel, BleState, Buttons, Diag, Hal, STORE_LEN};

use blep::{Blep, Command, Telemetry, TripId, Trips};
use input::{Btn, Event, Gesture, Input};
use motor::{Motor, codec};
use rides::{Rides, Sample};
use store::{Record, Saver};
use ui::diag::DiagData;
use ui::menu::{BleInfo, Confirm, Item, Menu};
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
    /// Dark screen; reboots into the bootloader's DFU mode once saved.
    Dfu {
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
    menu: Menu,
    rides: Rides,
    /// Motor reading counters already handed to `rides`.
    seen_speed: u32,
    seen_soc: u32,
    state: State,
    /// Last loaded/saved record; trip fields ride along unchanged until M4.
    rec: Record,
    saver: Saver,
    /// (pas, speed_limit, locked) as of the last save, to spot changes.
    saved: (u8, u8, bool),
    power: Power,
    last_activity: u32,
    last_buttons: Buttons,
    popups: Popups,
    /// The frame last sent to the display; flush only when it differs
    /// (the SPI flush is the most expensive thing on this chip, TECH_DESIGN §4.3).
    sent: Frame,
    sent_valid: bool,
    blep: Blep,
    /// Contrast last sent to the display; 0 = not yet (the App starts zeroed).
    contrast: u8,
    /// Time of the last tick, for calls from outside `tick()` (BLE control).
    now: u32,
    /// Last tick with the wheel turning.
    last_moving: u32,
    /// Power-on time and the screen the boot animation hands over to.
    boot_t0: u32,
    boot_then: Screen,
    /// The update-mode screen has reached the display: only then reboot,
    /// so it is what stays on the OLED while the bootloader runs.
    update_shown: bool,
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
            menu: Menu::new(),
            rides: Rides::new(),
            seen_speed: 0,
            seen_soc: 0,
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
                soc_min_valid: false,
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
            last_buttons: Buttons(0),
            popups: Popups::new(),
            sent: Frame::new(),
            sent_valid: false,
            blep: Blep::new(),
            contrast: 0,
            now: 0,
            last_moving: 0,
            boot_t0: 0,
            boot_then: Screen::Ride,
            update_shown: false,
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
        self.rides.load(&self.rec);
        self.saved = self.settings();
        self.boot_then = if self.state.locked {
            Screen::Pin
        } else {
            Screen::Ride
        };
        self.boot_t0 = now;
        self.stack.reset(Screen::Boot);
        self.last_activity = now;
        self.last_moving = now;
        self.now = now;
        self.motor.walk_keepalive_ms = config::WALK_KEEPALIVE_MS;
    }

    /// Called every 20 ms from the main loop only. Never blocks (TECH_DESIGN §4.4).
    pub fn tick(&mut self, now: u32) {
        self.now = now;
        // 1. input (ignored once power-off has started)
        let raw = self.hal.buttons();
        self.last_buttons = raw;
        let cfg = ui::gesture_cfg(self.stack.top(), &self.ride, self.popup_shown());
        let events = self.input.poll(raw, now, cfg);
        if self.power == Power::On {
            for ev in events.iter() {
                self.dispatch(ev, now);
            }
        }
        if self.stack.top() == Screen::Boot
            && now.wrapping_sub(self.boot_t0) >= ui::boot::duration_ms()
        {
            self.end_boot();
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
        self.ride_step(now);
        if self.motor.values.rpm.is_some_and(|r| r > 0) {
            self.last_moving = now;
        }
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

        // 4. phone
        let (tel, trips) = (self.telemetry(now), self.trips());
        self.blep.step(&mut self.hal, now, &tel, &trips);

        // 5. screen brightness follows the lights (PRODUCT §3.1)
        let contrast = if self.state.lights {
            config::CONTRAST_NIGHT
        } else {
            config::CONTRAST_DAY
        };
        if self.contrast != contrast {
            self.contrast = contrast;
            self.hal.display_contrast(contrast);
        }

        // 6. render, and flush only if something changed
        let model = self.model();
        let diag = self.diag_data();
        let overlay = match self.power {
            Power::On => Overlay::None,
            Power::Locking { .. } => Overlay::Padlock,
            Power::Off { .. } => Overlay::Dark,
            Power::Dfu { .. } => Overlay::Update,
        };
        let scene = Scene {
            top: self.stack.top(),
            ride: &self.ride,
            pin: &self.pin,
            menu: &self.menu,
            ble: self.ble_info(),
            model: &model,
            diag: &diag,
            popup: self.popup_shown(),
            overlay,
            now,
            boot_elapsed: now.wrapping_sub(self.boot_t0),
        };
        ui::render(&mut self.frame, &scene);
        if !self.sent_valid || self.frame != self.sent {
            self.hal.display_flush(&self.frame);
            self.sent.clone_from(&self.frame);
            self.sent_valid = true;
        }
        // the frame just flushed (or still on the display) is the update screen
        self.update_shown = overlay == Overlay::Update;
    }

    fn telemetry(&self, now: u32) -> Telemetry {
        let m = self.model();
        Telemetry {
            speed_x10: m.speed_x10.unwrap_or(0),
            power_w: m.power_w.unwrap_or(0),
            soc: m.soc.unwrap_or(blep::SOC_UNKNOWN),
            pas: self.state.pas,
            speed_limit: self.state.speed_limit,
            lights: self.state.lights,
            walk: self.state.walk,
            link_up: m.link_up,
            error: match self.fault(now) {
                None => 0,
                Some(Fault::Code(c)) => c,
                Some(Fault::LinkLost) => blep::ERROR_LINK_LOST,
            },
            odo_m: self.rides.odo_m,
        }
    }

    fn trips(&self) -> Trips {
        let r = &self.rides;
        Trips {
            trip: r.trip,
            batt: r.batt,
            ride: r.ride,
            odo: rides::Trip {
                m: r.odo_m,
                max_x10: r.odo_max_x10,
                ..rides::Trip::ZERO
            },
        }
    }

    /// The boot animation is over (or skipped): show the riding screen, or
    /// the PIN screen when locked.
    fn end_boot(&mut self) {
        self.stack.reset(self.boot_then);
    }

    /// Skip the boot animation (simulator: most tests start on the ride screen).
    pub fn skip_boot(&mut self) {
        if self.stack.top() == Screen::Boot {
            self.end_boot();
        }
    }

    /// Popups wait until the boot animation is over.
    fn popup_shown(&self) -> Popup {
        if self.stack.top() == Screen::Boot {
            Popup::None
        } else {
            self.popups.shown()
        }
    }

    fn settings(&self) -> (u8, u8, bool) {
        (self.state.pas, self.state.speed_limit, self.state.locked)
    }

    fn record(&self) -> Record {
        let mut r = Record {
            pas: self.state.pas,
            speed_limit: self.state.speed_limit,
            locked: self.state.locked,
            ..self.rec
        };
        self.rides.store(&mut r);
        r
    }

    /// Feed the trips with this tick's motor data (TECH_DESIGN §7.4).
    fn ride_step(&mut self, now: u32) {
        let m = &self.motor;
        let new_speed = (m.speed_samples != self.seen_speed)
            .then(|| m.values.rpm.map(codec::rpm_to_kph_x10))
            .flatten();
        let new_soc = (m.soc_samples != self.seen_soc)
            .then_some(m.values.soc)
            .flatten();
        self.seen_speed = m.speed_samples;
        self.seen_soc = m.soc_samples;
        let out = self.rides.step(&Sample {
            now,
            rpm: m.values.rpm.unwrap_or(0),
            current_x2: m.values.current_x2.unwrap_or(0),
            new_speed,
            new_soc,
        });
        if let Some(km_x10) = out.battery_trip_done {
            self.popups.battery_trip(km_x10);
            self.blep.trip_changed(TripId::Battery);
            self.saver.now(now);
            self.rides.saved();
        } else if out.save {
            self.saver.changed(now);
            self.rides.saved();
        }
    }

    fn ble_info(&self) -> BleInfo {
        let s = self.hal.ble_state().0;
        BleInfo {
            connected: s & BleState::CONNECTED != 0,
            commands: s & BleState::COMMAND_SUB != 0,
            address: self.hal.ble_address(),
            sent: self.blep.commands,
            dropped: self.hal.diag(Diag::BleDropped),
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
            Power::Dfu { since } => {
                if self.update_shown
                    && (self.saver.idle(&self.hal)
                        || now.wrapping_sub(since) >= config::SAVE_BEFORE_OFF_MS)
                {
                    self.hal.reboot_to_dfu();
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
        // Input never waits behind an animation (PRODUCT §3.4): a new press
        // snaps it to its end. Only the press: the release of the double-click
        // that started a pane slide must not cut it short.
        if ev.g == Down {
            self.ride.snap();
        }
        match self.popup_shown() {
            Popup::None => {}
            // the error screen takes all input; M acknowledges it
            Popup::Fault(_) => {
                if (ev.btn, ev.g) == (Btn::M, Click) {
                    self.popups.dismiss(now);
                }
                return;
            }
            // the battery-trip message goes away with any button
            Popup::BatteryTrip(_) => {
                if ev.g == Click {
                    self.popups.dismiss(now);
                }
                return;
            }
        }
        match self.stack.top() {
            Screen::Ride => match (ev.btn, ev.g) {
                (Btn::M, Click) => {
                    let m = self.model();
                    self.ride.next_page(&m, now);
                }
                (Btn::M, Double) => self.ride.next_view(now),
                (Btn::M, Hold) => {
                    self.menu.open();
                    self.stack.push(Screen::Menu);
                }
                (Btn::Pwr, Click) => {
                    let m = self.model();
                    self.ride.goto_pas(&m, now);
                }
                (Btn::Pwr, Double) => self.lock(now),
                (Btn::Left | Btn::Right, _) => self.page_event(ev, now),
                _ => {}
            },
            Screen::Menu => match (ev.btn, ev.g) {
                (Btn::Left, Click) => self.menu.prev(),
                (Btn::Right, Click) => self.menu.next(),
                (Btn::M, Click) => self.stack.push(match self.menu.item() {
                    Item::ResetTrip => Screen::Confirm(Confirm::ResetTrip),
                    Item::Ble => Screen::Ble,
                    Item::Diagnostics => Screen::Diag,
                    Item::Firmware => Screen::Firmware,
                    Item::Dfu => Screen::Update,
                }),
                (Btn::Pwr, Click) => self.stack.pop(),
                _ => {}
            },
            Screen::Confirm(c) => match (ev.btn, ev.g) {
                (Btn::M, Click) => {
                    self.stack.pop();
                    match c {
                        Confirm::ResetTrip => {
                            self.rides.reset_trip();
                            self.blep.trip_changed(TripId::Trip);
                            self.saver.now(now);
                            self.menu.show_done(now);
                        }
                    }
                }
                (Btn::Pwr, Click) => self.stack.pop(),
                _ => {}
            },
            // Update: the moment PWR goes down, save and reset while it is still
            // held; held for 5 s, it takes the bootloader into update mode
            // (TECH_DESIGN §9.5). Any other button goes back.
            Screen::Update => match (ev.btn, ev.g) {
                (Btn::Pwr, Down) => {
                    self.saver.now(now);
                    self.power = Power::Dfu { since: now };
                }
                (_, Click) => self.stack.pop(),
                _ => {}
            },
            // any button skips the boot animation; the press does nothing else
            Screen::Boot => {
                if ev.g == Click {
                    self.end_boot();
                }
            }
            Screen::Diag | Screen::Ble | Screen::Firmware => {
                if (ev.btn, ev.g) == (Btn::Pwr, Click) {
                    self.stack.pop();
                }
            }
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
        self.ride.reset_to_pas();
    }

    fn page_event(&mut self, ev: Event, now: u32) {
        let pas = self.state.pas;
        if let Some(c) = self.page_command(ev)
            // dropped when no phone listens: fire-and-forget (PRODUCT §9)
            && self.blep.command(&mut self.hal, c)
        {
            self.ride.show_action(c, now);
        }
        self.ride.slide_pas(pas, self.state.pas, now);
    }

    /// Handles the page's own buttons; returns the command for the phone, if any.
    fn page_command(&mut self, ev: Event) -> Option<Command> {
        use Gesture::*;
        let s = &mut self.state;
        // Walk assist ends with its hold whatever page is showing by then
        // (an M click during the hold switches the page under it).
        if (ev.btn, ev.g) == (Btn::Left, HoldEnd) && s.walk {
            s.walk = false;
            return None;
        }
        match self.ride.page {
            Page::Pas => {
                match (ev.btn, ev.g) {
                    (Btn::Left, Click) => s.pas = s.pas.saturating_sub(1),
                    (Btn::Right, Click) => s.pas = (s.pas + 1).min(config::PAS_MAX),
                    (Btn::Left, Hold) if s.pas == 0 => s.walk = true,
                    _ => {}
                }
                None
            }
            Page::Lights => {
                match (ev.btn, ev.g) {
                    (Btn::Left, Click) => s.lights = false,
                    (Btn::Right, Click) => s.lights = true,
                    _ => {}
                }
                None
            }
            Page::Player => match (ev.btn, ev.g) {
                (Btn::Left, Click) => Some(Command::VolumeDown),
                (Btn::Left, Hold) => Some(Command::PrevTrack),
                (Btn::Right, Click) => Some(Command::VolumeUp),
                (Btn::Right, Hold) => Some(Command::NextTrack),
                (Btn::Right, Double) => Some(Command::PlayPause),
                _ => None,
            },
            Page::Gate => match (ev.btn, ev.g) {
                (Btn::Left, Click) => Some(Command::GateA),
                (Btn::Right, Click) => Some(Command::GateB),
                _ => None,
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
            lights: self.state.lights,
            commands: self.hal.ble_state().0 & BleState::COMMAND_SUB != 0,
            trip: self.rides.trip,
            batt: self.rides.batt,
            ride: self.rides.ride,
            odo_m: self.rides.odo_m,
            odo_max_x10: self.rides.odo_max_x10,
        }
    }

    fn diag_data(&self) -> DiagData {
        let h = &self.hal;
        DiagData {
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
    /// `"DFU!"` opens the Update screen, like the menu item, once the wheel
    /// has stood still for `DFU_STOPPED_MS`. The rider still has to hold PWR:
    /// the bootloader can't keep the display powered across a reset on its
    /// own (TECH_DESIGN §9.5). Anything else is ignored.
    pub fn ble_control(&mut self, data: &[u8]) {
        if data == blep::CONTROL_DFU
            && self.power == Power::On
            && self.now.wrapping_sub(self.last_moving) >= config::DFU_STOPPED_MS
            && self.stack.top() != Screen::Update
        {
            self.stack.push(Screen::Update);
        }
    }

    // --- read-only accessors for the simulator and tests ---

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn power(&self) -> Power {
        self.power
    }

    pub fn rides(&self) -> &Rides {
        &self.rides
    }

    pub fn menu_item(&self) -> Item {
        self.menu.item()
    }

    pub fn saves(&self) -> u32 {
        self.saver.writes
    }

    pub fn screen(&self) -> Screen {
        self.stack.top()
    }

    pub fn popup(&self) -> Popup {
        self.popup_shown()
    }

    /// A page, PAS or info-pane slide is running.
    pub fn animating(&self) -> bool {
        self.ride.animating(self.now)
    }

    pub fn page(&self) -> Page {
        self.ride.page
    }

    pub fn view(&self) -> View {
        self.ride.view
    }

    pub fn blep(&self) -> &Blep {
        &self.blep
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
