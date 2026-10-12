//! Lights: the rider's mode (OFF / ON / AUTO, menu → Lights) and the time of
//! day the phone sends, which AUTO needs (PRODUCT §3.5, TECH_DESIGN §9.5).
//!
//! RAM only: after power-on the mode is OFF and the time unknown. The first
//! time sync after power-on switches to AUTO, unless the rider has already
//! picked a mode in the menu; a pick always wins over later syncs.

use crate::blep::TimeSync;
use crate::config;

const DAY_S: u32 = 24 * 60 * 60;

/// `Off` first, so the App stays zero-initialised.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum LightsMode {
    Off,
    On,
    /// On from `LIGHTS_LEAD_MIN` before sunset until sunrise; off while the
    /// time is unknown.
    Auto,
}

impl LightsMode {
    /// Menu order (PRODUCT §5).
    pub const ALL: [LightsMode; 3] = [LightsMode::Off, LightsMode::On, LightsMode::Auto];

    pub fn label(self) -> &'static [u8] {
        match self {
            LightsMode::Off => b"OFF",
            LightsMode::On => b"ON",
            LightsMode::Auto => b"AUTO",
        }
    }
}

/// Local time of day, kept running from the tick clock between syncs.
/// Zero-initialised = unknown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DayClock {
    synced: bool,
    /// Local seconds since midnight at `at_ms`.
    secs: u32,
    at_ms: u32,
    sunset_min: u16,
    sunrise_min: u16,
}

impl DayClock {
    pub const fn new() -> Self {
        Self {
            synced: false,
            secs: 0,
            at_ms: 0,
            sunset_min: 0,
            sunrise_min: 0,
        }
    }

    pub fn sync(&mut self, t: &TimeSync, now: u32) {
        *self = Self {
            synced: true,
            secs: t.secs_of_day,
            at_ms: now,
            sunset_min: t.sunset_min,
            sunrise_min: t.sunrise_min,
        };
    }

    /// Local seconds since midnight, wrapping at midnight; `None` before the
    /// first sync. `now` wraps after ~49 days, `wrapping_sub` copes.
    pub fn secs_of_day(&self, now: u32) -> Option<u32> {
        let elapsed = now.wrapping_sub(self.at_ms) / 1000;
        self.synced.then_some((self.secs + elapsed % DAY_S) % DAY_S)
    }

    /// (lights on from, lights off at) in minutes since midnight: the last
    /// sync's sunset − `LIGHTS_LEAD_MIN`, and its sunrise.
    pub fn dark_window(&self) -> Option<(u16, u16)> {
        self.synced.then_some((
            self.sunset_min.saturating_sub(config::LIGHTS_LEAD_MIN),
            self.sunrise_min,
        ))
    }

    /// Dark: at or after (sunset − lead), or before sunrise. Unknown time = not dark.
    pub fn dark(&self, now: u32) -> bool {
        match (self.secs_of_day(now), self.dark_window()) {
            (Some(t), Some((from, until))) => {
                t >= u32::from(from) * 60 || t < u32::from(until) * 60
            }
            _ => false,
        }
    }
}

impl Default for DayClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Mode + clock. Zero-initialised: OFF, no time, nothing picked yet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Lights {
    pub mode: LightsMode,
    pub clock: DayClock,
    /// The rider picked a mode in the menu since power-on.
    picked: bool,
    /// A time sync has arrived since power-on.
    synced_once: bool,
}

impl Lights {
    pub const fn new() -> Self {
        Self {
            mode: LightsMode::Off,
            clock: DayClock::new(),
            picked: false,
            synced_once: false,
        }
    }

    /// Menu → Lights → M.
    pub fn pick(&mut self, mode: LightsMode) {
        self.mode = mode;
        self.picked = true;
    }

    /// The phone sent the time. The first one after power-on turns AUTO on,
    /// unless the rider has chosen already.
    pub fn sync(&mut self, t: &TimeSync, now: u32) {
        self.clock.sync(t, now);
        if !self.synced_once && !self.picked {
            self.mode = LightsMode::Auto;
        }
        self.synced_once = true;
    }

    /// The effective light state: what the motor light, the screen
    /// brightness and telemetry follow.
    pub fn on(&self, now: u32) -> bool {
        match self.mode {
            LightsMode::Off => false,
            LightsMode::On => true,
            LightsMode::Auto => self.clock.dark(now),
        }
    }
}

impl Default for Lights {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sunset 18:30, sunrise 06:45: lights from 18:10 until 06:45.
    fn sync_at(secs_of_day: u32) -> TimeSync {
        TimeSync {
            secs_of_day,
            sunset_min: 18 * 60 + 30,
            sunrise_min: 6 * 60 + 45,
        }
    }

    const fn hms(h: u32, m: u32, s: u32) -> u32 {
        h * 3600 + m * 60 + s
    }

    fn dark_at(secs: u32) -> bool {
        let mut c = DayClock::new();
        c.sync(&sync_at(secs), 0);
        c.dark(0)
    }

    #[test]
    fn dark_from_the_lead_before_sunset_until_sunrise() {
        assert_eq!(config::LIGHTS_LEAD_MIN, 20);
        assert!(!dark_at(hms(12, 0, 0)));
        assert!(!dark_at(hms(18, 9, 59)));
        assert!(dark_at(hms(18, 10, 0)), "sunset − 20 min");
        assert!(dark_at(hms(23, 59, 59)));
        assert!(dark_at(0));
        assert!(dark_at(hms(6, 44, 59)));
        assert!(!dark_at(hms(6, 45, 0)), "sunrise");
    }

    #[test]
    fn the_clock_runs_on_the_tick_and_wraps_at_midnight() {
        let mut c = DayClock::new();
        assert_eq!(c.secs_of_day(5000), None);
        assert!(!c.dark(5000), "unknown time is not dark");
        let t0 = 1_000_000;
        c.sync(&sync_at(hms(23, 59, 50)), t0);
        assert_eq!(c.secs_of_day(t0 + 9_999), Some(hms(23, 59, 59)));
        assert_eq!(c.secs_of_day(t0 + 10_000), Some(0));
        assert_eq!(c.secs_of_day(t0 + 10_000 + DAY_S * 1000), Some(0));
        // just before sunrise the next morning, then light
        let to_sunrise = (hms(6, 45, 0) + 10) * 1000;
        assert!(c.dark(t0 + to_sunrise - 1000));
        assert!(!c.dark(t0 + to_sunrise));
        // the tick counter wrapping (after ~49 days) doesn't break it
        c.sync(&sync_at(hms(12, 0, 0)), u32::MAX - 999);
        assert_eq!(c.secs_of_day(9_000), Some(hms(12, 0, 10)));
    }

    #[test]
    fn first_sync_turns_auto_on_and_a_pick_wins_after_that() {
        let mut l = Lights::new();
        assert_eq!(l.mode, LightsMode::Off);
        l.sync(&sync_at(hms(21, 0, 0)), 0);
        assert_eq!(l.mode, LightsMode::Auto);
        assert!(l.on(0));
        l.pick(LightsMode::Off);
        l.sync(&sync_at(hms(21, 0, 0)), 100);
        assert_eq!(l.mode, LightsMode::Off);
        assert!(!l.on(100));
    }

    #[test]
    fn a_pick_before_the_first_sync_is_kept() {
        let mut l = Lights::new();
        l.pick(LightsMode::On);
        l.sync(&sync_at(hms(12, 0, 0)), 0);
        assert_eq!(l.mode, LightsMode::On);
        assert!(l.on(0));
    }

    #[test]
    fn auto_without_a_time_is_off() {
        let mut l = Lights::new();
        l.pick(LightsMode::Auto);
        assert!(!l.on(0));
    }
}
