//! Trips, odometer and the battery-trip rule (PRODUCT §3.2, TECH_DESIGN §7.4).
//!
//! Everything integrates over elapsed time (`now − last_now`) on every tick,
//! never per tick or per motor slot, so late ticks and the poll schedule can't
//! skew it (§4.4).

use crate::config;
use crate::store::Record;

/// km/h × 10 that two consecutive readings may differ by for a new max to count.
const MAX_AGREE_X10: u16 = 50;
/// Battery % rise that means "the pack was charged".
const CHARGE_RISE: u8 = 10;
/// Identical SoC readings in a row before the battery-trip rule trusts one.
const SOC_STABLE: u8 = 3;

/// One distance counter with its statistics.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Trip {
    pub m: u32,
    /// Time with the wheel turning.
    pub moving_ms: u32,
    pub max_x10: u16,
    /// Charge drawn from the battery.
    pub mah: u32,
}

impl Trip {
    pub const ZERO: Trip = Trip {
        m: 0,
        moving_ms: 0,
        max_x10: 0,
        mah: 0,
    };

    /// Average over moving time, km/h × 10 (= m / s × 36).
    pub fn avg_x10(&self) -> u16 {
        if self.moving_ms < 1000 {
            return 0;
        }
        (u64::from(self.m) * 36_000 / u64::from(self.moving_ms)).min(u64::from(u16::MAX)) as u16
    }
}

/// What a tick tells the trips.
pub struct Sample {
    pub now: u32,
    /// Latest wheel rpm (0 when unknown).
    pub rpm: u16,
    /// Latest motor current, 0.5 A units (0 when unknown).
    pub current_x2: u8,
    /// A new SPEED reply arrived this tick.
    pub new_speed: Option<u16>,
    /// A new BATTERY reply arrived this tick.
    pub new_soc: Option<u8>,
}

/// What a tick asks the App to do.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Outcome {
    /// The battery was charged: show this distance (100 m units) and save now.
    pub battery_trip_done: Option<u16>,
    /// Distances changed enough to be worth a (debounced) save.
    pub save: bool,
}

pub struct Rides {
    pub trip: Trip,
    pub batt: Trip,
    pub ride: Trip,
    pub odo_m: u32,
    pub odo_max_x10: u16,
    pub soc_min: u8,
    pub soc_min_valid: bool,
    started: bool,
    last_now: u32,
    /// rpm × wheel_mm × ms; 60 000 000 of these are one meter.
    dist_acc: u32,
    /// mA × ms; 3 600 000 of these are one mAh.
    charge_acc: u32,
    prev_speed_x10: u16,
    have_prev_speed: bool,
    soc_last: u8,
    soc_same: u8,
    /// Odometer at the last distance save, and since when the wheel stands still.
    odo_saved_m: u32,
    stopped_since: u32,
    stopped: bool,
}

impl Default for Rides {
    fn default() -> Self {
        Self::new()
    }
}

impl Rides {
    pub const fn new() -> Self {
        Self {
            trip: Trip::ZERO,
            batt: Trip::ZERO,
            ride: Trip::ZERO,
            odo_m: 0,
            odo_max_x10: 0,
            soc_min: 0,
            soc_min_valid: false,
            started: false,
            last_now: 0,
            dist_acc: 0,
            charge_acc: 0,
            prev_speed_x10: 0,
            have_prev_speed: false,
            soc_last: 0,
            soc_same: 0,
            odo_saved_m: 0,
            stopped_since: 0,
            stopped: false,
        }
    }

    /// Restore from flash. The ride counter always starts at zero.
    pub fn load(&mut self, r: &Record) {
        self.trip = Trip {
            m: r.trip_m,
            moving_ms: r.trip_moving_s.saturating_mul(1000),
            max_x10: r.trip_max_x10,
            mah: r.trip_mah,
        };
        self.batt = Trip {
            m: r.trip_batt_m,
            moving_ms: r.trip_batt_moving_s.saturating_mul(1000),
            max_x10: r.trip_batt_max_x10,
            mah: r.trip_batt_mah,
        };
        self.odo_m = r.odo_m;
        self.odo_max_x10 = r.odo_max_x10;
        self.soc_min = r.soc_min;
        self.soc_min_valid = r.soc_min_valid;
        self.odo_saved_m = r.odo_m;
    }

    /// Write the persisted part into a record.
    pub fn store(&self, r: &mut Record) {
        r.trip_m = self.trip.m;
        r.trip_moving_s = self.trip.moving_ms / 1000;
        r.trip_max_x10 = self.trip.max_x10;
        r.trip_mah = self.trip.mah;
        r.trip_batt_m = self.batt.m;
        r.trip_batt_moving_s = self.batt.moving_ms / 1000;
        r.trip_batt_max_x10 = self.batt.max_x10;
        r.trip_batt_mah = self.batt.mah;
        r.odo_m = self.odo_m;
        r.odo_max_x10 = self.odo_max_x10;
        r.soc_min = self.soc_min;
        r.soc_min_valid = self.soc_min_valid;
    }

    pub fn reset_trip(&mut self) {
        self.trip = Trip::ZERO;
    }

    /// A save is about to happen: distances are saved up to here.
    pub fn saved(&mut self) {
        self.odo_saved_m = self.odo_m;
    }

    pub fn step(&mut self, s: &Sample) -> Outcome {
        let mut out = Outcome::default();
        let dt = if self.started {
            s.now.wrapping_sub(self.last_now)
        } else {
            0
        };
        self.started = true;
        self.last_now = s.now;

        // distance: whole meters go to every counter
        self.dist_acc += u32::from(s.rpm) * config::WHEEL_MM * dt;
        while self.dist_acc >= 60_000_000 {
            self.dist_acc -= 60_000_000;
            for t in [&mut self.trip, &mut self.batt, &mut self.ride] {
                t.m += 1;
            }
            self.odo_m += 1;
        }
        // moving time
        if s.rpm > 0 {
            for t in [&mut self.trip, &mut self.batt, &mut self.ride] {
                t.moving_ms = t.moving_ms.saturating_add(dt);
            }
        }
        // charge: whole mAh go to every trip
        self.charge_acc += u32::from(s.current_x2) * 500 * dt;
        while self.charge_acc >= 3_600_000 {
            self.charge_acc -= 3_600_000;
            for t in [&mut self.trip, &mut self.batt, &mut self.ride] {
                t.mah += 1;
            }
        }
        // max speed: only when two readings in a row agree
        if let Some(v) = s.new_speed {
            if self.have_prev_speed && v.abs_diff(self.prev_speed_x10) <= MAX_AGREE_X10 {
                let m = v.min(self.prev_speed_x10);
                for t in [&mut self.trip, &mut self.batt, &mut self.ride] {
                    t.max_x10 = t.max_x10.max(m);
                }
                self.odo_max_x10 = self.odo_max_x10.max(m);
            }
            self.prev_speed_x10 = v;
            self.have_prev_speed = true;
        }
        if let Some(soc) = s.new_soc {
            out.battery_trip_done = self.on_soc(soc);
        }

        // distance save policy (TECH_DESIGN §8.3)
        let unsaved = self.odo_m - self.odo_saved_m;
        if s.rpm > 0 {
            self.stopped = false;
        } else if !self.stopped {
            self.stopped = true;
            self.stopped_since = s.now;
        }
        let stopped_long =
            self.stopped && s.now.wrapping_sub(self.stopped_since) >= config::SAVE_STOPPED_MS;
        if unsaved >= config::SAVE_EVERY_M
            || (stopped_long && unsaved >= config::SAVE_STOPPED_MIN_M)
        {
            out.save = true;
        }
        out
    }

    /// Battery-trip rule (PRODUCT §3.2): follow the lowest SoC down; a rise of
    /// 10 % or more means the pack was charged.
    fn on_soc(&mut self, soc: u8) -> Option<u16> {
        if soc == self.soc_last {
            self.soc_same = self.soc_same.saturating_add(1);
        } else {
            self.soc_last = soc;
            self.soc_same = 1;
        }
        if self.soc_same < SOC_STABLE {
            return None; // ignore single readings (boot, voltage sag)
        }
        if !self.soc_min_valid {
            // first boot with this firmware: just take the current level
            self.soc_min = soc;
            self.soc_min_valid = true;
            return None;
        }
        if soc < self.soc_min {
            self.soc_min = soc;
            return None;
        }
        if soc >= self.soc_min.saturating_add(CHARGE_RISE) {
            let km_x10 = (self.batt.m / 100).min(u32::from(u16::MAX)) as u16;
            self.batt = Trip::ZERO;
            self.soc_min = soc;
            return Some(km_x10);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick(r: &mut Rides, now: u32, rpm: u16, current_x2: u8) -> Outcome {
        r.step(&Sample {
            now,
            rpm,
            current_x2,
            new_speed: None,
            new_soc: None,
        })
    }

    #[test]
    fn distance_and_moving_time_integrate_over_elapsed_time() {
        let mut r = Rides::new();
        // 277 rpm × 2165 mm ≈ 9.995 m/s; ticks of uneven length on purpose
        let mut now = 0;
        for i in 0..3000 {
            now += if i % 3 == 0 { 40 } else { 10 };
            tick(&mut r, now, 277, 0);
        }
        // 1000 × 40 ms + 2000 × 10 ms = 60 s; the first tick has no elapsed time → ~599.3 m
        assert!((599..=600).contains(&r.trip.m), "{} m", r.trip.m);
        assert_eq!(
            (r.ride.m, r.batt.m, r.odo_m),
            (r.trip.m, r.trip.m, r.trip.m)
        );
        assert!((59_950..=60_000).contains(&r.trip.moving_ms));
        assert_eq!(r.trip.avg_x10(), 359); // 35.9 km/h
    }

    #[test]
    fn standing_still_adds_no_moving_time() {
        let mut r = Rides::new();
        for i in 1..=100 {
            tick(&mut r, i * 20, 0, 0);
        }
        assert_eq!((r.trip.m, r.trip.moving_ms, r.trip.avg_x10()), (0, 0, 0));
    }

    #[test]
    fn charge_counts_milliamp_hours() {
        let mut r = Rides::new();
        // 10 A for 36 s = 100 mAh (1801 ticks = 1800 intervals of 20 ms)
        for i in 0..=1800 {
            tick(&mut r, i * 20, 0, 20);
        }
        assert_eq!(r.trip.mah, 100);
    }

    #[test]
    fn max_needs_two_agreeing_readings() {
        let mut r = Rides::new();
        let mut now = 0;
        let mut speed = |r: &mut Rides, v: u16| {
            now += 200;
            r.step(&Sample {
                now,
                rpm: 1,
                current_x2: 0,
                new_speed: Some(v),
                new_soc: None,
            });
        };
        speed(&mut r, 300);
        speed(&mut r, 310);
        assert_eq!(r.trip.max_x10, 300);
        speed(&mut r, 900); // one corrupt sample
        speed(&mut r, 320);
        assert_eq!(r.trip.max_x10, 300, "a lone spike is not a record");
        speed(&mut r, 340);
        assert_eq!((r.trip.max_x10, r.odo_max_x10), (320, 320));
    }

    fn soc(r: &mut Rides, now: &mut u32, v: u8, times: u8) -> Option<u16> {
        let mut out = None;
        for _ in 0..times {
            *now += 600;
            out = r
                .step(&Sample {
                    now: *now,
                    rpm: 0,
                    current_x2: 0,
                    new_speed: None,
                    new_soc: Some(v),
                })
                .battery_trip_done
                .or(out);
        }
        out
    }

    #[test]
    fn battery_trip_resets_on_a_10_percent_rise() {
        let mut r = Rides::new();
        let mut now = 0;
        assert_eq!(
            soc(&mut r, &mut now, 70, 3),
            None,
            "first boot: silent init"
        );
        assert_eq!((r.soc_min, r.soc_min_valid), (70, true));
        r.batt.m = 42_700;
        assert_eq!(soc(&mut r, &mut now, 40, 3), None);
        assert_eq!(r.soc_min, 40);
        assert_eq!(
            soc(&mut r, &mut now, 49, 3),
            None,
            "+9 % is a top-up, not a charge"
        );
        assert_eq!(soc(&mut r, &mut now, 95, 3), Some(427));
        assert_eq!((r.batt.m, r.soc_min), (0, 95));
    }

    #[test]
    fn single_soc_readings_are_ignored() {
        let mut r = Rides::new();
        let mut now = 0;
        soc(&mut r, &mut now, 40, 3);
        assert_eq!(
            soc(&mut r, &mut now, 90, 2),
            None,
            "two readings are not enough"
        );
        assert_eq!(soc(&mut r, &mut now, 41, 3), None);
        assert_eq!(r.soc_min, 40);
    }

    #[test]
    fn distance_saves_every_km_and_after_a_stop() {
        let mut r = Rides::new();
        let mut now = 0;
        let mut saves = 0;
        // ~10 m/s for 120 s → 1.2 km: one save at the 1 km mark
        for _ in 0..6000 {
            now += 20;
            if tick(&mut r, now, 277, 0).save {
                saves += 1;
                r.saved();
            }
        }
        assert_eq!(saves, 1);
        // stop: after 5 s with ≥ 100 m unsaved
        for _ in 0..300 {
            now += 20;
            if tick(&mut r, now, 0, 0).save {
                saves += 1;
                r.saved();
            }
        }
        assert_eq!(saves, 2);
    }
}
