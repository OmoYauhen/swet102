//! Fake Bafang BBSHD controller (stock display protocol) with real 1200-baud
//! timing on the virtual clock (TECH_DESIGN §11.2).

use std::collections::VecDeque;

/// One byte at 1200 baud 8N1 = 10 bits = 8333 µs.
pub const BYTE_US: u64 = 8_333;
/// Controller think time before it starts replying.
pub const LATENCY_US: u64 = 10_000;

#[derive(Debug, Clone)]
pub struct FakeMotor {
    /// false = the controller doesn't answer at all (link-loss tests).
    pub online: bool,
    /// STATUS reply: 0x01 normal, 0x03 braking, other = error code.
    pub status: u8,
    pub rpm: u16,
    /// Motor current in 0.5 A units.
    pub current_x2: u8,
    pub soc: u8,
    /// Last values written by the display.
    pub pas_code: Option<u8>,
    pub lights: Option<bool>,
    pub speed_limit_wire: Option<u16>,
    rx: Vec<u8>,
    /// Bytes on their way back to the display, with arrival time (µs).
    out: VecDeque<(u64, u8)>,
    line_free_at_us: u64,
}

impl Default for FakeMotor {
    fn default() -> Self {
        Self {
            online: true,
            status: 0x01,
            rpm: 0,
            current_x2: 0,
            soc: 78,
            pas_code: None,
            lights: None,
            speed_limit_wire: None,
            rx: Vec::new(),
            out: VecDeque::new(),
            line_free_at_us: 0,
        }
    }
}

impl FakeMotor {
    /// Wheel rpm for a road speed on the 2165 mm wheel.
    pub fn set_speed_kmh(&mut self, kmh: u32) {
        self.rpm = (kmh * 1_000_000 / (60 * swet_heart::config::WHEEL_MM)) as u16;
    }

    /// Bytes from the display, arriving after they've been clocked out.
    pub fn receive(&mut self, now_us: u64, bytes: &[u8]) {
        let done_us = now_us + BYTE_US * bytes.len() as u64;
        for &b in bytes {
            self.rx.push(b);
            if let Some(reply) = self.parse() {
                self.reply(done_us, &reply);
            }
        }
    }

    /// Next reply byte that has fully arrived by `now_us`.
    pub fn pop_arrived(&mut self, now_us: u64) -> Option<u8> {
        match self.out.front() {
            Some(&(t, b)) if t <= now_us => {
                self.out.pop_front();
                Some(b)
            }
            _ => None,
        }
    }

    fn reply(&mut self, after_us: u64, bytes: &[u8]) {
        if !self.online || bytes.is_empty() {
            return;
        }
        let mut t = (after_us + LATENCY_US).max(self.line_free_at_us);
        for &b in bytes {
            t += BYTE_US;
            self.out.push_back((t, b));
        }
        self.line_free_at_us = t;
    }

    /// Recognise one complete request at the start of `rx`; returns the reply
    /// (empty for writes). `None` = need more bytes.
    fn parse(&mut self) -> Option<Vec<u8>> {
        let reply = match *self.rx.as_slice() {
            [0x11, 0x08] => vec![self.status],
            [0x11, 0x0A] => vec![self.current_x2, self.current_x2],
            [0x11, 0x11] => vec![self.soc, self.soc],
            [0x11, 0x20] => {
                let [hi, lo] = self.rpm.to_be_bytes();
                vec![hi, lo, hi.wrapping_add(lo).wrapping_add(0x20)]
            }
            [0x11, _] => Vec::new(),
            [0x16, 0x0B, code, _sum] => {
                self.pas_code = Some(code);
                Vec::new()
            }
            [0x16, 0x1A, v] => {
                self.lights = Some(v == 0xF1);
                Vec::new()
            }
            [0x16, 0x1F, hi, lo, _sum] => {
                self.speed_limit_wire = Some(u16::from_be_bytes([hi, lo]));
                Vec::new()
            }
            [0x16] | [0x16, _] | [0x16, 0x0B, ..] | [0x16, 0x1F, ..] | [0x11] => return None,
            _ => Vec::new(), // garbage: drop it
        };
        self.rx.clear();
        Some(reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reply_arrives_after_wire_time_and_latency() {
        let mut m = FakeMotor::default();
        m.receive(20_000, &[0x11, 0x08]);
        // 2 bytes out + latency + 1 byte back = 20 + 16.7 + 10 + 8.3 = 55 ms
        assert_eq!(m.pop_arrived(54_000), None);
        assert_eq!(m.pop_arrived(55_000), Some(0x01));
        assert_eq!(m.pop_arrived(100_000), None);
    }

    #[test]
    fn offline_controller_stays_silent() {
        let mut m = FakeMotor {
            online: false,
            ..FakeMotor::default()
        };
        m.receive(0, &[0x11, 0x08]);
        assert_eq!(m.pop_arrived(1_000_000), None);
    }

    #[test]
    fn records_writes() {
        let mut m = FakeMotor::default();
        m.receive(0, &[0x16, 0x0B, 0x0C, 0x2D]);
        m.receive(0, &[0x16, 0x1F, 0x00, 0xC0, 0xF5]);
        m.receive(0, &[0x16, 0x1A, 0xF1]);
        assert_eq!(m.pas_code, Some(0x0C));
        assert_eq!(m.speed_limit_wire, Some(192));
        assert_eq!(m.lights, Some(true));
        assert_eq!(m.pop_arrived(u64::MAX), None, "writes get no reply");
    }

    #[test]
    fn speed_reply_has_checksum() {
        let mut m = FakeMotor::default();
        m.set_speed_kmh(25);
        m.receive(0, &[0x11, 0x20]);
        let got: Vec<u8> = std::iter::from_fn(|| m.pop_arrived(u64::MAX)).collect();
        assert_eq!(got, [0x00, 0xC0, 0xE0]);
    }
}
