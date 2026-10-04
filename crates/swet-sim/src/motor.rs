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
    /// STATUS reply byte: 0x01 normal, 0x03 braking, other = error code.
    pub status: u8,
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
            rx: Vec::new(),
            out: VecDeque::new(),
            line_free_at_us: 0,
        }
    }
}

impl FakeMotor {
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
        if !self.online {
            return;
        }
        let mut t = (after_us + LATENCY_US).max(self.line_free_at_us);
        for &b in bytes {
            t += BYTE_US;
            self.out.push_back((t, b));
        }
        self.line_free_at_us = t;
    }

    /// Recognise one complete request at the start of `rx`; returns the reply.
    fn parse(&mut self) -> Option<Vec<u8>> {
        let reply = match self.rx.as_slice() {
            [0x11, 0x08] => vec![self.status],
            [0x11, _] | [0x16, ..] => {
                // Unknown read or a write: consume silently (M1 adds the full model).
                Vec::new()
            }
            [_] => return None,
            _ => Vec::new(),
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
}
