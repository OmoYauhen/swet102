//! Time-based tweens (TECH_DESIGN §5.4). Position comes from `now − t0`, so a
//! late frame lands where it should and the animation still ends on time.
//! Ease-out `1 − (1 − p)²` in Q8 fixed point; no floats.

/// Q8 "one".
pub const ONE: i32 = 256;

/// All zeroes when idle, so it can live in the zero-initialised App (.bss).
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Tween {
    t0: u32,
    dur: u16,
    on: bool,
}

impl Tween {
    pub const IDLE: Tween = Tween {
        t0: 0,
        dur: 0,
        on: false,
    };

    pub fn start(&mut self, now: u32, dur_ms: u16) {
        *self = Tween {
            t0: now,
            dur: dur_ms.max(1),
            on: true,
        };
    }

    /// Jump to the end (input arrived, TECH_DESIGN §5.4).
    pub fn snap(&mut self) {
        self.on = false;
    }

    /// Eased progress 0..=`ONE` while running, `None` once finished or idle.
    pub fn eased(&self, now: u32) -> Option<i32> {
        if !self.on {
            return None;
        }
        let t = now.wrapping_sub(self.t0);
        if t >= u32::from(self.dur) {
            return None;
        }
        let p = (t * ONE as u32 / u32::from(self.dur)) as i32; // 0..ONE linear
        let rest = ONE - p;
        Some(ONE - rest * rest / ONE)
    }

    pub fn running(&self, now: u32) -> bool {
        self.eased(now).is_some()
    }
}

/// `span × eased / ONE`: how far a sliding thing has moved.
pub fn travel(span: i32, eased: i32) -> i32 {
    span * eased / ONE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eases_out_and_ends_on_time() {
        let mut t = Tween::IDLE;
        assert_eq!(t.eased(0), None);
        t.start(1000, 200);
        assert_eq!(t.eased(1000), Some(0));
        let half = t.eased(1100).expect("running");
        assert!(
            half > ONE / 2,
            "ease-out is past halfway at half time: {half}"
        );
        assert!(t.eased(1199).expect("running") <= ONE);
        assert_eq!(t.eased(1200), None);
    }

    #[test]
    fn snap_ends_it() {
        let mut t = Tween::IDLE;
        t.start(0, 150);
        t.snap();
        assert!(!t.running(10));
    }

    #[test]
    fn late_frames_jump_to_the_right_place() {
        let mut t = Tween::IDLE;
        t.start(0, 100);
        // a frame 60 ms late sees the same value as an on-time frame at that time
        assert_eq!(t.eased(80), Some(ONE - (ONE - 204) * (ONE - 204) / ONE));
    }
}
