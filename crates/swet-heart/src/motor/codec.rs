//! Bafang stock display protocol: frames and replies as pure functions
//! (TECH_DESIGN §7). Facts from Swang Stodva `state.c`.

use crate::config;

const READ: u8 = 0x11;
const WRITE: u8 = 0x16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Op {
    Status = 0x08,
    Current = 0x0A,
    Battery = 0x11,
    Speed = 0x20,
}

impl Op {
    /// Reply length in bytes.
    pub const fn reply_len(self) -> usize {
        match self {
            Op::Status => 1,
            Op::Current | Op::Battery => 2,
            Op::Speed => 3,
        }
    }

    pub const fn request(self) -> [u8; 2] {
        [READ, self as u8]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reply {
    /// 0x01 normal, 0x03 braking, anything else = error code.
    Status(u8),
    /// Motor current in 0.5 A units.
    CurrentX2(u8),
    /// State of charge, %.
    Battery(u8),
    /// Wheel speed, rpm.
    SpeedRpm(u16),
}

/// Decode a complete reply. `None` = checksum mismatch.
pub fn decode(op: Op, b: &[u8]) -> Option<Reply> {
    match (op, b) {
        (Op::Status, [s]) => Some(Reply::Status(*s)),
        (Op::Current, [v, c]) if v == c => Some(Reply::CurrentX2(*v)),
        (Op::Battery, [v, c]) if v == c => Some(Reply::Battery(*v)),
        (Op::Speed, [hi, lo, c]) if hi.wrapping_add(*lo).wrapping_add(0x20) == *c => {
            Some(Reply::SpeedRpm(u16::from_be_bytes([*hi, *lo])))
        }
        _ => None,
    }
}

/// PAS wire codes for levels 0–9 (stock 9-level display).
const PAS_CODES: [u8; 10] = [0x00, 0x01, 0x0B, 0x0C, 0x0D, 0x02, 0x15, 0x16, 0x17, 0x03];
/// Walk assist ("push") code.
pub const PAS_WALK: u8 = 0x06;

pub fn pas_code(level: u8) -> u8 {
    PAS_CODES[usize::from(level.min(9))]
}

pub fn write_pas(code: u8) -> [u8; 4] {
    [
        WRITE,
        0x0B,
        code,
        WRITE.wrapping_add(0x0B).wrapping_add(code),
    ]
}

/// Lights: no checksum.
pub fn write_lights(on: bool) -> [u8; 3] {
    [WRITE, 0x1A, if on { 0xF1 } else { 0xF0 }]
}

pub fn write_speed_limit(wire: u16) -> [u8; 5] {
    let [hi, lo] = wire.to_be_bytes();
    let sum = WRITE.wrapping_add(0x1F).wrapping_add(hi).wrapping_add(lo);
    [WRITE, 0x1F, hi, lo, sum]
}

/// The value sent in WRITE_SPEED_LIM. Unit unverified on stock firmware
/// (TECH_DESIGN §16): RPM by default, Swang Stodva's km/h × 10 otherwise.
pub const fn speed_limit_wire(kmh: u32) -> u16 {
    if config::SPEED_LIMIT_AS_RPM {
        (kmh * 1_000_000 / 60 / config::WHEEL_MM) as u16
    } else {
        (kmh * 10) as u16
    }
}

/// Wheel rpm → km/h × 10.
pub const fn rpm_to_kph_x10(rpm: u16) -> u16 {
    (rpm as u32 * config::WHEEL_MM * 6 / 10_000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests() {
        assert_eq!(Op::Speed.request(), [0x11, 0x20]);
        assert_eq!(Op::Status.reply_len(), 1);
    }

    #[test]
    fn decode_checks_checksums() {
        assert_eq!(
            decode(Op::Speed, &[0x00, 0xC0, 0xE0]),
            Some(Reply::SpeedRpm(192))
        );
        assert_eq!(decode(Op::Speed, &[0x00, 0xC0, 0xE1]), None);
        assert_eq!(decode(Op::Current, &[24, 24]), Some(Reply::CurrentX2(24)));
        assert_eq!(decode(Op::Current, &[24, 25]), None);
        assert_eq!(decode(Op::Battery, &[78, 78]), Some(Reply::Battery(78)));
        assert_eq!(decode(Op::Status, &[0x21]), Some(Reply::Status(0x21)));
        assert_eq!(decode(Op::Status, &[]), None);
    }

    #[test]
    fn writes() {
        assert_eq!(write_pas(pas_code(3)), [0x16, 0x0B, 0x0C, 0x2D]);
        assert_eq!(write_pas(PAS_WALK), [0x16, 0x0B, 0x06, 0x27]);
        assert_eq!(write_lights(true), [0x16, 0x1A, 0xF1]);
        assert_eq!(write_speed_limit(192), [0x16, 0x1F, 0x00, 0xC0, 0xF5]);
    }

    #[test]
    fn speed_limit_and_speed_use_the_2165_mm_wheel() {
        assert_eq!(speed_limit_wire(25), 192);
        assert_eq!(speed_limit_wire(99), 762);
        assert_eq!(rpm_to_kph_x10(192), 249); // 24.9 km/h
        assert_eq!(rpm_to_kph_x10(0), 0);
    }
}
