//! Persistent record and save policy (TECH_DESIGN §8).
//!
//! One 48-byte record, encoded field by field in little-endian (no transmute).
//! Settings changes are saved 3 s after the last change; unlocking, locking
//! and power-off save at once.

use crate::config;
use crate::hal::{Hal, STORE_LEN};

/// Layout version. Unknown versions load as defaults (SS data is never read).
pub const VERSION: u8 = 1;

/// Everything that survives a power cycle. Trip fields are carried unchanged
/// until the trip logic lands (M4).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Record {
    pub pas: u8,
    /// km/h sent to the controller; 25 = city, > 25 = sport.
    pub speed_limit: u8,
    pub locked: bool,
    pub soc_min: u8,
    pub odo_m: u32,
    pub trip_m: u32,
    pub trip_moving_s: u32,
    pub trip_batt_m: u32,
    pub trip_batt_moving_s: u32,
    pub trip_max_x10: u16,
    pub trip_batt_max_x10: u16,
    pub trip_mah: u32,
    pub trip_batt_mah: u32,
    pub odo_max_x10: u16,
}

impl Record {
    pub const fn defaults() -> Self {
        Self {
            pas: 0,
            speed_limit: config::CITY_LIMIT_KMH,
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
        }
    }

    pub fn encode(&self) -> [u8; STORE_LEN] {
        let mut b = [0u8; STORE_LEN];
        b[0] = VERSION;
        b[1] = self.pas;
        b[2] = self.speed_limit;
        b[3] = u8::from(self.locked);
        b[4] = self.soc_min;
        // 5..8 reserved
        put32(&mut b, 8, self.odo_m);
        put32(&mut b, 12, self.trip_m);
        put32(&mut b, 16, self.trip_moving_s);
        put32(&mut b, 20, self.trip_batt_m);
        put32(&mut b, 24, self.trip_batt_moving_s);
        put16(&mut b, 28, self.trip_max_x10);
        put16(&mut b, 30, self.trip_batt_max_x10);
        put32(&mut b, 32, self.trip_mah);
        put32(&mut b, 36, self.trip_batt_mah);
        put16(&mut b, 40, self.odo_max_x10);
        // 42..48 reserved
        b
    }

    /// `None` for an unknown layout version. Out-of-range values are clamped
    /// rather than rejected, so one bad field can't wipe the odometer.
    pub fn decode(b: &[u8; STORE_LEN]) -> Option<Self> {
        if b[0] != VERSION {
            return None;
        }
        let speed_limit = match b[2] {
            0 => config::CITY_LIMIT_KMH,
            v => v.min(config::SPORT_LIMIT_KMH),
        };
        Some(Self {
            pas: b[1].min(config::PAS_MAX),
            speed_limit,
            locked: b[3] != 0,
            soc_min: b[4].min(100),
            odo_m: get32(b, 8),
            trip_m: get32(b, 12),
            trip_moving_s: get32(b, 16),
            trip_batt_m: get32(b, 20),
            trip_batt_moving_s: get32(b, 24),
            trip_max_x10: get16(b, 28),
            trip_batt_max_x10: get16(b, 30),
            trip_mah: get32(b, 32),
            trip_batt_mah: get32(b, 36),
            odo_max_x10: get16(b, 40),
        })
    }
}

fn put32(b: &mut [u8; STORE_LEN], at: usize, v: u32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn put16(b: &mut [u8; STORE_LEN], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn get32(b: &[u8; STORE_LEN], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn get16(b: &[u8; STORE_LEN], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// When to write. Zero-initialised = nothing pending.
pub struct Saver {
    pending: bool,
    /// An immediate save is waiting; later debounced changes must not delay it.
    urgent: bool,
    due: u32,
    pub writes: u32,
}

impl Default for Saver {
    fn default() -> Self {
        Self::new()
    }
}

impl Saver {
    pub const fn new() -> Self {
        Self {
            pending: false,
            urgent: false,
            due: 0,
            writes: 0,
        }
    }

    /// A setting changed: save once it has been quiet for `SAVE_DEBOUNCE_MS`.
    pub fn changed(&mut self, now: u32) {
        self.pending = true;
        if !self.urgent {
            self.due = now.wrapping_add(config::SAVE_DEBOUNCE_MS);
        }
    }

    /// Save at the next opportunity (lock, unlock, power-off).
    pub fn now(&mut self, now: u32) {
        self.pending = true;
        self.urgent = true;
        self.due = now;
    }

    pub fn step(&mut self, hal: &mut impl Hal, rec: &Record, now: u32) {
        if self.pending && now.wrapping_sub(self.due) < u32::MAX / 2 && !hal.store_busy() {
            hal.store_save(&rec.encode());
            self.pending = false;
            self.urgent = false;
            self.writes += 1;
        }
    }

    /// Nothing waiting and the last write has finished.
    pub fn idle(&self, hal: &impl Hal) -> bool {
        !self.pending && !hal.store_busy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal Hal for the saver: records writes, never busy.
    struct Flash(u32);
    impl Hal for Flash {
        fn display_flush(&mut self, _: &crate::Frame) {}
        fn display_contrast(&mut self, _: u8) {}
        fn buttons(&mut self) -> crate::Buttons {
            crate::Buttons(0)
        }
        fn uart_write(&mut self, _: &[u8]) {}
        fn uart_read(&mut self) -> Option<u8> {
            None
        }
        fn store_load(&mut self, _: &mut [u8; STORE_LEN]) -> bool {
            false
        }
        fn store_save(&mut self, _: &[u8; STORE_LEN]) {
            self.0 += 1;
        }
        fn store_busy(&self) -> bool {
            false
        }
        fn ble_state(&self) -> crate::BleState {
            crate::BleState(0)
        }
        fn ble_notify(&mut self, _: crate::BleChannel, _: &[u8]) {}
        fn ble_address(&self) -> [u8; 6] {
            [0; 6]
        }
        fn power_off(&mut self) {}
        fn reboot_to_dfu(&mut self) {}
        fn diag(&self, _: crate::Diag) -> u32 {
            0
        }
    }

    #[test]
    fn debounce_extends_but_never_delays_an_urgent_save() {
        let (mut s, mut f, r) = (Saver::new(), Flash(0), Record::defaults());
        s.changed(0);
        s.changed(2_000); // extends to 5 000
        s.step(&mut f, &r, 4_000);
        assert_eq!(f.0, 0);
        s.step(&mut f, &r, 5_000);
        assert_eq!(f.0, 1);

        s.now(10_000); // unlock
        s.changed(10_000); // …and the same tick's change detection
        s.step(&mut f, &r, 10_000);
        assert_eq!(f.0, 2, "saved at once");
    }

    #[test]
    fn roundtrip_keeps_every_field() {
        let r = Record {
            pas: 7,
            speed_limit: 99,
            locked: true,
            soc_min: 41,
            odo_m: 1_234_567,
            trip_m: 42_700,
            trip_moving_s: 7_120,
            trip_batt_m: 18_000,
            trip_batt_moving_s: 3_600,
            trip_max_x10: 384,
            trip_batt_max_x10: 377,
            trip_mah: 7_850,
            trip_batt_mah: 3_200,
            odo_max_x10: 512,
        };
        assert_eq!(Record::decode(&r.encode()), Some(r));
    }

    #[test]
    fn layout_offsets_match_the_design() {
        let mut r = Record::defaults();
        r.odo_m = 0x0403_0201;
        r.odo_max_x10 = 0x0605;
        let b = r.encode();
        assert_eq!(b[0], VERSION);
        assert_eq!(b[2], 25);
        assert_eq!(&b[8..12], &[1, 2, 3, 4]);
        assert_eq!(&b[40..42], &[5, 6]);
    }

    #[test]
    fn unknown_version_and_garbage_are_handled() {
        let mut b = Record::defaults().encode();
        b[0] = 0xFF; // erased flash / other firmware
        assert_eq!(Record::decode(&b), None);
        let mut b = Record::defaults().encode();
        b[1] = 200; // pas
        b[2] = 0; // speed limit
        let r = Record::decode(&b).expect("valid version");
        assert_eq!((r.pas, r.speed_limit), (9, 25));
    }
}
