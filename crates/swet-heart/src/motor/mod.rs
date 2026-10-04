//! Motor bus master: fixed 100 ms slots, coalesced writes, link state
//! (TECH_DESIGN §7). Non-blocking: `step()` only drains bytes that already
//! arrived and queues at most one frame per slot (§4.4).

pub mod codec;

use crate::config;
use crate::hal::Hal;
use codec::{Op, Reply};

/// One request per slot; speed every other slot.
const SCHEDULE: [Op; 6] = [
    Op::Speed,
    Op::Current,
    Op::Speed,
    Op::Battery,
    Op::Speed,
    Op::Status,
];

// Explicit tag so `Idle` is 0 and the App stays zero-initialised (.bss);
// without it `Idle` would live in an unused `Op` value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum Bus {
    Idle,
    Waiting { op: Op, got: [u8; 3], n: usize },
}

/// Latest decoded values. `None` until the first valid reply.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct MotorValues {
    pub rpm: Option<u16>,
    pub current_x2: Option<u8>,
    pub soc: Option<u8>,
    pub status: Option<u8>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct MotorDiag {
    pub requests: u32,
    pub replies: u32,
    pub timeouts: u32,
    pub bad_checksums: u32,
    pub stray_bytes: u32,
    pub writes: u32,
}

/// What the display wants the controller to have.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Desired {
    pas_code: u8,
    lights: bool,
    limit_wire: u16,
}

pub struct Motor {
    bus: Bus,
    slot: usize,
    next_slot_at: u32,
    last_ok: Option<u32>,
    up: bool,
    desired: Desired,
    pending_pas: bool,
    pending_lights: bool,
    pending_limit: bool,
    pub values: MotorValues,
    pub diag: MotorDiag,
}

impl Motor {
    pub const fn new() -> Self {
        Self {
            bus: Bus::Idle,
            slot: 0,
            next_slot_at: 0,
            last_ok: None,
            up: false,
            desired: Desired {
                pas_code: 0,
                lights: false,
                limit_wire: 0,
            },
            pending_pas: false,
            pending_lights: false,
            pending_limit: false,
            values: MotorValues {
                rpm: None,
                current_x2: None,
                soc: None,
                status: None,
            },
            diag: MotorDiag {
                requests: 0,
                replies: 0,
                timeouts: 0,
                bad_checksums: 0,
                stray_bytes: 0,
                writes: 0,
            },
        }
    }

    pub fn link_up(&self) -> bool {
        self.up
    }

    pub fn set_pas_code(&mut self, code: u8) {
        if self.desired.pas_code != code {
            self.desired.pas_code = code;
            self.pending_pas = true;
        }
    }

    pub fn set_lights(&mut self, on: bool) {
        if self.desired.lights != on {
            self.desired.lights = on;
            self.pending_lights = true;
        }
    }

    pub fn set_speed_limit_kmh(&mut self, kmh: u8) {
        let wire = codec::speed_limit_wire(u32::from(kmh));
        if self.desired.limit_wire != wire {
            self.desired.limit_wire = wire;
            self.pending_limit = true;
        }
    }

    pub fn step(&mut self, hal: &mut impl Hal, now: u32) {
        while let Some(b) = hal.uart_read() {
            self.on_byte(b, now);
        }

        if self.up
            && self
                .last_ok
                .is_some_and(|t| now.wrapping_sub(t) >= config::MOTOR_LINK_TIMEOUT_MS)
        {
            self.up = false;
            self.values = MotorValues::default();
        }

        if now < self.next_slot_at {
            return;
        }
        self.next_slot_at = now + config::MOTOR_SLOT_MS;

        if let Bus::Waiting { .. } = self.bus {
            self.diag.timeouts += 1;
            self.bus = Bus::Idle;
        }

        // Writes take the slot ahead of the scheduled read, but only once the
        // controller has answered something (SS does the same).
        if self.up {
            if self.pending_limit {
                self.pending_limit = false;
                return self.write(hal, &codec::write_speed_limit(self.desired.limit_wire));
            }
            if self.pending_pas {
                self.pending_pas = false;
                return self.write(hal, &codec::write_pas(self.desired.pas_code));
            }
            if self.pending_lights {
                self.pending_lights = false;
                return self.write(hal, &codec::write_lights(self.desired.lights));
            }
        }

        let op = SCHEDULE[self.slot];
        self.slot = (self.slot + 1) % SCHEDULE.len();
        hal.uart_write(&op.request());
        self.diag.requests += 1;
        self.bus = Bus::Waiting {
            op,
            got: [0; 3],
            n: 0,
        };
    }

    fn write(&mut self, hal: &mut impl Hal, frame: &[u8]) {
        hal.uart_write(frame);
        self.diag.writes += 1;
    }

    fn on_byte(&mut self, b: u8, now: u32) {
        let Bus::Waiting { op, mut got, mut n } = self.bus else {
            self.diag.stray_bytes += 1;
            return;
        };
        got[n] = b;
        n += 1;
        if n < op.reply_len() {
            self.bus = Bus::Waiting { op, got, n };
            return;
        }
        self.bus = Bus::Idle;
        match codec::decode(op, &got[..n]) {
            Some(r) => self.on_reply(r, now),
            None => self.diag.bad_checksums += 1,
        }
    }

    fn on_reply(&mut self, r: Reply, now: u32) {
        self.diag.replies += 1;
        self.last_ok = Some(now);
        if !self.up {
            // Link (re)established: the controller may have rebooted, so
            // push everything we want it to have (TECH_DESIGN §7.3).
            self.up = true;
            self.pending_pas = true;
            self.pending_lights = true;
            self.pending_limit = true;
        }
        match r {
            Reply::Status(s) => self.values.status = Some(s),
            Reply::CurrentX2(c) => self.values.current_x2 = Some(c),
            Reply::Battery(p) => self.values.soc = Some(p),
            Reply::SpeedRpm(rpm) => self.values.rpm = Some(rpm),
        }
    }
}

impl Default for Motor {
    fn default() -> Self {
        Self::new()
    }
}
