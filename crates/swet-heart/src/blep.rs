//! BLE payloads and their schedule (TECH_DESIGN §9.4). The C side owns the
//! stack and the GATT table; this module decides what goes into the
//! characteristics and when.
//!
//! - **Telemetry**: 14 bytes, once a second while a phone is connected.
//! - **Trips**: four 18-byte records (trip, battery trip, ride, odometer),
//!   one per tick every 5 s while connected, and the affected one straight
//!   after a reset.
//! - **Command**: `[seq, code]`, once per button press, only while the phone
//!   is subscribed to it. Fire-and-forget: nothing is queued or retried.
//! - **Control** (phone → display): `DFU!`, or a 9-byte time sync (`T`).
//!
//! `Hal::ble_notify` publishes a value: it becomes what a read returns and is
//! notified to a subscribed phone. All payloads are little-endian and fit the
//! default 20-byte ATT payload.

use crate::hal::{BleChannel, BleState, Hal};
use crate::rides::Trip;

pub const TELEMETRY_VERSION: u8 = 2;
pub const TELEMETRY_LEN: usize = 14;
pub const TRIPS_VERSION: u8 = 1;
pub const TRIP_LEN: usize = 18;

/// Telemetry and trips cadence.
pub const TELEMETRY_MS: u32 = 1000;
pub const TRIPS_MS: u32 = 5000;

/// `error` byte when the motor link is down.
pub const ERROR_LINK_LOST: u8 = 0xFF;
/// `soc` byte before the first battery reading.
pub const SOC_UNKNOWN: u8 = 0xFF;

/// The phone wrote this to the control characteristic: reboot into DFU.
pub const CONTROL_DFU: &[u8] = b"DFU!";

/// Control message: the phone's local time and today's sun (TECH_DESIGN §9.5).
/// `[b'T', secs_of_day u32, sunset_min u16, sunrise_min u16]`, little-endian.
pub const CONTROL_TIME: u8 = b'T';
pub const TIME_SYNC_LEN: usize = 9;

/// A valid time sync: all values local, from midnight.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TimeSync {
    /// 0..86399
    pub secs_of_day: u32,
    /// Minutes, < 1440.
    pub sunset_min: u16,
    pub sunrise_min: u16,
}

/// `None` for a wrong tag or length, or any value out of range.
pub fn parse_time_sync(b: &[u8]) -> Option<TimeSync> {
    if b.len() != TIME_SYNC_LEN || b[0] != CONTROL_TIME {
        return None;
    }
    let t = TimeSync {
        secs_of_day: u32::from_le_bytes([b[1], b[2], b[3], b[4]]),
        sunset_min: u16::from_le_bytes([b[5], b[6]]),
        sunrise_min: u16::from_le_bytes([b[7], b[8]]),
    };
    (t.secs_of_day < 24 * 3600 && t.sunset_min < 24 * 60 && t.sunrise_min < 24 * 60).then_some(t)
}

/// The bytes of a time sync, as the phone sends them (simulator, tools).
pub fn encode_time_sync(t: &TimeSync) -> [u8; TIME_SYNC_LEN] {
    let mut b = [0u8; TIME_SYNC_LEN];
    b[0] = CONTROL_TIME;
    b[1..5].copy_from_slice(&t.secs_of_day.to_le_bytes());
    b[5..7].copy_from_slice(&t.sunset_min.to_le_bytes());
    b[7..9].copy_from_slice(&t.sunrise_min.to_le_bytes());
    b
}

/// Commands sent to the phone (display → phone, command characteristic).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Command {
    VolumeUp = 0x01,
    VolumeDown = 0x02,
    NextTrack = 0x03,
    PrevTrack = 0x04,
    PlayPause = 0x05,
    GateA = 0x10,
    GateB = 0x11,
}

/// Trip record ids.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum TripId {
    Trip = 0,
    Battery = 1,
    Ride = 2,
    Odometer = 3,
}

/// Everything the telemetry packet carries.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Telemetry {
    pub speed_x10: u16,
    pub power_w: u16,
    /// `SOC_UNKNOWN` until the first reading.
    pub soc: u8,
    pub pas: u8,
    pub speed_limit: u8,
    pub lights: bool,
    pub walk: bool,
    pub link_up: bool,
    /// 0 = none, else the STATUS code; `ERROR_LINK_LOST` without a link.
    pub error: u8,
    pub odo_m: u32,
}

pub fn encode_telemetry(t: &Telemetry) -> [u8; TELEMETRY_LEN] {
    let mut b = [0u8; TELEMETRY_LEN];
    b[0] = TELEMETRY_VERSION;
    b[1..3].copy_from_slice(&t.speed_x10.to_le_bytes());
    b[3..5].copy_from_slice(&t.power_w.to_le_bytes());
    b[5] = t.soc;
    b[6] = t.pas;
    b[7] = t.speed_limit;
    b[8] = u8::from(t.lights) | u8::from(t.walk) << 1 | u8::from(t.link_up) << 2;
    b[9] = t.error;
    b[10..14].copy_from_slice(&(t.odo_m / 100).to_le_bytes());
    b
}

/// One trips record. The odometer uses `dist_m` and `max_x10` only.
pub fn encode_trip(id: TripId, t: &Trip) -> [u8; TRIP_LEN] {
    let mut b = [0u8; TRIP_LEN];
    b[0] = TRIPS_VERSION;
    b[1] = id as u8;
    b[2..6].copy_from_slice(&t.m.to_le_bytes());
    b[6..8].copy_from_slice(&t.max_x10.to_le_bytes());
    b[8..10].copy_from_slice(&t.avg_x10().to_le_bytes());
    b[10..14].copy_from_slice(&t.mah.to_le_bytes());
    b[14..18].copy_from_slice(&(t.moving_ms / 1000).to_le_bytes());
    b
}

/// The four trip records as of this tick.
pub struct Trips {
    pub trip: Trip,
    pub batt: Trip,
    pub ride: Trip,
    /// Odometer as a trip: only `m` and `max_x10` are set.
    pub odo: Trip,
}

impl Trips {
    fn get(&self, id: TripId) -> &Trip {
        match id {
            TripId::Trip => &self.trip,
            TripId::Battery => &self.batt,
            TripId::Ride => &self.ride,
            TripId::Odometer => &self.odo,
        }
    }
}

const TRIP_IDS: [TripId; 4] = [
    TripId::Trip,
    TripId::Battery,
    TripId::Ride,
    TripId::Odometer,
];

/// Zero-initialised (lives in the App, in .bss): due times of 0 mean "send at
/// the first tick with a phone".
pub struct Blep {
    seq: u8,
    next_telemetry: u32,
    next_trips: u32,
    /// Trip records still to send, bit = `TripId`.
    trips_due: u8,
    connected: bool,
    /// Commands notified since boot (for the emulator panel and tests).
    pub commands: u32,
    pub last_command: u8,
}

impl Default for Blep {
    fn default() -> Self {
        Self::new()
    }
}

impl Blep {
    pub const fn new() -> Self {
        Self {
            seq: 0,
            next_telemetry: 0,
            next_trips: 0,
            trips_due: 0,
            connected: false,
            commands: 0,
            last_command: 0,
        }
    }

    /// A trip was reset: send its record on the next tick.
    pub fn trip_changed(&mut self, id: TripId) {
        self.trips_due |= 1 << id as u8;
    }

    /// Notify a command if the phone listens for them. Returns whether it went out.
    pub fn command(&mut self, hal: &mut impl Hal, c: Command) -> bool {
        if hal.ble_state().0 & BleState::COMMAND_SUB == 0 {
            return false;
        }
        self.seq = self.seq.wrapping_add(1);
        hal.ble_notify(BleChannel::Command, &[self.seq, c as u8]);
        self.commands = self.commands.wrapping_add(1);
        self.last_command = c as u8;
        true
    }

    pub fn step(&mut self, hal: &mut impl Hal, now: u32, tel: &Telemetry, trips: &Trips) {
        let connected = hal.ble_state().0 & BleState::CONNECTED != 0;
        if connected && !self.connected {
            // a fresh connection gets everything at once
            self.next_telemetry = now;
            self.next_trips = now;
        }
        self.connected = connected;
        if !connected {
            self.trips_due = 0;
            return;
        }
        if now >= self.next_telemetry {
            self.next_telemetry = now + TELEMETRY_MS;
            hal.ble_notify(BleChannel::Telemetry, &encode_telemetry(tel));
        }
        if now >= self.next_trips {
            self.next_trips = now + TRIPS_MS;
            self.trips_due = 0b1111;
        }
        // one record per tick, so a burst can't run the SoftDevice out of buffers
        if let Some(&id) = TRIP_IDS
            .iter()
            .find(|&&id| self.trips_due & 1 << id as u8 != 0)
        {
            self.trips_due &= !(1 << id as u8);
            hal.ble_notify(BleChannel::Trips, &encode_trip(id, trips.get(id)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_layout() {
        let t = Telemetry {
            speed_x10: 0x0123,
            power_w: 0x0456,
            soc: 78,
            pas: 3,
            speed_limit: 25,
            lights: true,
            walk: false,
            link_up: true,
            error: 0,
            odo_m: 123_456,
        };
        assert_eq!(
            encode_telemetry(&t),
            [
                2, 0x23, 0x01, 0x56, 0x04, 78, 3, 25, 0b101, 0, 0xD2, 0x04, 0, 0
            ]
        );
    }

    #[test]
    fn time_sync_layout_and_rejects() {
        // 18:30:15, sunset 19:05, sunrise 06:40
        let b = [b'T', 0x37, 0x04, 0x01, 0x00, 0x77, 0x04, 0x90, 0x01];
        let t = TimeSync {
            secs_of_day: 66_615,
            sunset_min: 1143,
            sunrise_min: 400,
        };
        assert_eq!(parse_time_sync(&b), Some(t));
        assert_eq!(encode_time_sync(&t), b);

        assert_eq!(parse_time_sync(&b[..8]), None, "short");
        assert_eq!(parse_time_sync(&[&b[..], &[0]].concat()), None, "long");
        assert_eq!(parse_time_sync(b"DFU!"), None);
        let bad = |i: usize, v: &[u8]| {
            let mut x = b;
            x[i..i + v.len()].copy_from_slice(v);
            parse_time_sync(&x)
        };
        assert_eq!(bad(0, b"t"), None, "tag");
        assert_eq!(bad(1, &86_400u32.to_le_bytes()), None, "secs");
        assert!(bad(1, &86_399u32.to_le_bytes()).is_some());
        assert_eq!(bad(5, &1440u16.to_le_bytes()), None, "sunset");
        assert_eq!(bad(7, &1440u16.to_le_bytes()), None, "sunrise");
        assert!(bad(7, &1439u16.to_le_bytes()).is_some());
    }

    #[test]
    fn trip_layout() {
        let t = Trip {
            m: 36_000,
            moving_ms: 3_600_000,
            max_x10: 412,
            mah: 7850,
        };
        let b = encode_trip(TripId::Battery, &t);
        assert_eq!(b[..2], [1, 1]);
        assert_eq!(u32::from_le_bytes([b[2], b[3], b[4], b[5]]), 36_000);
        assert_eq!(u16::from_le_bytes([b[6], b[7]]), 412);
        assert_eq!(u16::from_le_bytes([b[8], b[9]]), 360); // 36 km in 1 h
        assert_eq!(u32::from_le_bytes([b[10], b[11], b[12], b[13]]), 7850);
        assert_eq!(u32::from_le_bytes([b[14], b[15], b[16], b[17]]), 3600);
    }
}
