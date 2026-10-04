//! Debounce + gesture engine (TECH_DESIGN §5.3).
//!
//! Each button reports `Down`, `Up`, `Click`, `Double`, `Hold` and `HoldEnd`.
//! Whether a button waits for a double-click or watches for a hold comes from
//! the screen on top ([`GestureCfg`]) and is latched when the button goes down,
//! so a screen change mid-press can't reinterpret the release.

use crate::config;
use crate::hal::Buttons;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Btn {
    Left = 0,
    Right = 1,
    M = 2,
    Pwr = 3,
}

impl Btn {
    pub const ALL: [Btn; 4] = [Btn::Left, Btn::Right, Btn::M, Btn::Pwr];

    const fn mask(self) -> u8 {
        match self {
            Btn::Left => Buttons::LEFT,
            Btn::Right => Buttons::RIGHT,
            Btn::M => Buttons::M,
            Btn::Pwr => Buttons::PWR,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gesture {
    Down,
    Up,
    Click,
    Double,
    Hold,
    HoldEnd,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Event {
    pub btn: Btn,
    pub g: Gesture,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct BtnCfg {
    /// Wait `DBL_MS` after a release for a second press.
    pub double: bool,
    /// Report `Hold` after `HOLD_MS`; that press then gives no `Click`.
    pub hold: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct GestureCfg(pub [BtnCfg; 4]);

impl GestureCfg {
    /// Instant clicks only, plus PWR hold (power off works everywhere).
    pub const SIMPLE: Self = Self([
        BtnCfg {
            double: false,
            hold: false,
        },
        BtnCfg {
            double: false,
            hold: false,
        },
        BtnCfg {
            double: false,
            hold: false,
        },
        BtnCfg {
            double: false,
            hold: true,
        },
    ]);

    pub const fn with(mut self, b: Btn, cfg: BtnCfg) -> Self {
        self.0[b as usize] = cfg;
        self
    }
}

/// Up to this many events per poll (4 buttons × at most 3 each).
pub const MAX_EVENTS: usize = 12;

#[derive(Default)]
pub struct Events {
    buf: [Option<Event>; MAX_EVENTS],
    len: usize,
}

impl Events {
    fn push(&mut self, btn: Btn, g: Gesture) {
        if self.len < MAX_EVENTS {
            self.buf[self.len] = Some(Event { btn, g });
            self.len += 1;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        self.buf[..self.len].iter().flatten().copied()
    }
}

#[derive(Clone, Copy, Default)]
struct BtnState {
    stable: bool,
    /// Consecutive raw samples that disagree with `stable`.
    flip: u8,
    down_at: u32,
    cfg: BtnCfg,
    hold_fired: bool,
    /// This press was the second half of a double-click.
    consumed: bool,
    /// Released at this time, waiting to see whether a second press follows.
    pending_click: Option<u32>,
}

pub struct Input {
    btn: [BtnState; 4],
    /// All buttons have been released once since power-on. The PWR press that
    /// switched the display on is still held when the firmware starts.
    armed: bool,
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

impl Input {
    pub const fn new() -> Self {
        const B: BtnState = BtnState {
            stable: false,
            flip: 0,
            down_at: 0,
            cfg: BtnCfg {
                double: false,
                hold: false,
            },
            hold_fired: false,
            consumed: false,
            pending_click: None,
        };
        Self {
            btn: [B; 4],
            armed: false,
        }
    }

    /// True while any button is (debounced) down.
    pub fn any_down(&self) -> bool {
        self.btn.iter().any(|b| b.stable)
    }

    pub fn poll(&mut self, raw: Buttons, now: u32, cfg: GestureCfg) -> Events {
        let mut ev = Events::default();
        if !self.armed {
            self.armed = raw.0 == 0;
            return ev;
        }
        for b in Btn::ALL {
            let s = &mut self.btn[b as usize];
            let pressed = raw.has(b.mask());

            // A pending single click expires once the double-click window has passed.
            if let Some(t) = s.pending_click
                && !s.stable
                && now.wrapping_sub(t) >= config::DBL_MS
            {
                s.pending_click = None;
                ev.push(b, Gesture::Click);
            }

            if pressed != s.stable {
                s.flip += 1;
                if s.flip < config::DEBOUNCE_SAMPLES {
                    continue;
                }
            }
            s.flip = 0;

            match (s.stable, pressed) {
                (false, true) => {
                    s.stable = true;
                    s.down_at = now;
                    s.hold_fired = false;
                    s.consumed = false;
                    ev.push(b, Gesture::Down);
                    if s.pending_click.take().is_some() {
                        // second press inside the window: keep the first press's cfg
                        s.consumed = true;
                        ev.push(b, Gesture::Double);
                    } else {
                        s.cfg = cfg.0[b as usize];
                    }
                }
                (true, true) => {
                    if s.cfg.hold
                        && !s.hold_fired
                        && !s.consumed
                        && now.wrapping_sub(s.down_at) >= config::HOLD_MS
                    {
                        s.hold_fired = true;
                        ev.push(b, Gesture::Hold);
                    }
                }
                (true, false) => {
                    s.stable = false;
                    ev.push(b, Gesture::Up);
                    if s.hold_fired {
                        ev.push(b, Gesture::HoldEnd);
                    } else if !s.consumed {
                        if s.cfg.double {
                            s.pending_click = Some(now);
                        } else {
                            ev.push(b, Gesture::Click);
                        }
                    }
                }
                (false, false) => {}
            }
        }
        ev
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: u32 = config::TICK_MS;

    struct Rig {
        input: Input,
        now: u32,
        raw: u8,
        cfg: GestureCfg,
        log: [Option<Event>; 64],
        n: usize,
    }

    impl Rig {
        fn new(cfg: GestureCfg) -> Self {
            let mut r = Self {
                input: Input::new(),
                now: 0,
                raw: 0,
                cfg,
                log: [None; 64],
                n: 0,
            };
            r.run(TICK); // arm
            r
        }
        fn run(&mut self, ms: u32) {
            let end = self.now + ms;
            while self.now < end {
                self.now += TICK;
                for e in self
                    .input
                    .poll(Buttons(self.raw), self.now, self.cfg)
                    .iter()
                {
                    if e.g != Gesture::Down && e.g != Gesture::Up {
                        self.log[self.n] = Some(e);
                        self.n += 1;
                    }
                }
            }
        }
        fn press(&mut self, b: Btn, ms: u32) {
            self.raw |= b.mask();
            self.run(ms);
            self.raw &= !b.mask();
        }
        fn got(&self) -> impl Iterator<Item = Event> + '_ {
            self.log[..self.n].iter().flatten().copied()
        }
        fn gestures(&self, b: Btn) -> usize {
            self.got().filter(|e| e.btn == b).count()
        }
        fn has(&self, b: Btn, g: Gesture) -> bool {
            self.got().any(|e| e == Event { btn: b, g })
        }
    }

    const DBL: BtnCfg = BtnCfg {
        double: true,
        hold: true,
    };

    #[test]
    fn simple_click_fires_on_release() {
        let mut r = Rig::new(GestureCfg::SIMPLE);
        r.press(Btn::Right, 60);
        r.run(TICK * 2);
        assert!(r.has(Btn::Right, Gesture::Click));
        assert_eq!(r.gestures(Btn::Right), 1);
    }

    #[test]
    fn single_sample_glitch_is_debounced() {
        let mut r = Rig::new(GestureCfg::SIMPLE);
        r.press(Btn::Right, TICK);
        r.run(200);
        assert_eq!(r.gestures(Btn::Right), 0);
    }

    #[test]
    fn double_click_window_delays_single_click() {
        let mut r = Rig::new(GestureCfg::SIMPLE.with(Btn::M, DBL));
        r.press(Btn::M, 60);
        r.run(200);
        assert_eq!(r.gestures(Btn::M), 0, "still inside the 350 ms window");
        r.run(300);
        assert!(r.has(Btn::M, Gesture::Click));
    }

    #[test]
    fn double_click_gives_double_and_no_click() {
        let mut r = Rig::new(GestureCfg::SIMPLE.with(Btn::M, DBL));
        r.press(Btn::M, 60);
        r.run(100);
        r.press(Btn::M, 60);
        r.run(600);
        assert!(r.has(Btn::M, Gesture::Double));
        assert_eq!(r.gestures(Btn::M), 1);
    }

    #[test]
    fn hold_fires_once_then_hold_end_and_no_click() {
        let mut r = Rig::new(GestureCfg::SIMPLE.with(
            Btn::Left,
            BtnCfg {
                double: false,
                hold: true,
            },
        ));
        r.press(Btn::Left, 1500);
        r.run(100);
        let left: [Gesture; 2] = [Gesture::Hold, Gesture::HoldEnd];
        let got: [Option<Gesture>; 2] = {
            let mut it = r.got().filter(|e| e.btn == Btn::Left).map(|e| e.g);
            [it.next(), it.next()]
        };
        assert_eq!(got, [Some(left[0]), Some(left[1])]);
        assert_eq!(r.gestures(Btn::Left), 2);
    }

    #[test]
    fn button_held_at_power_on_is_ignored_until_released() {
        let mut input = Input::new();
        let held = Buttons(Buttons::PWR);
        let mut events = 0;
        for i in 1..200 {
            events += input
                .poll(held, i * TICK, GestureCfg::SIMPLE)
                .iter()
                .count();
        }
        assert_eq!(events, 0);
    }

    #[test]
    fn config_is_latched_at_press() {
        let mut r = Rig::new(GestureCfg::SIMPLE.with(Btn::M, DBL));
        r.raw |= Btn::M.mask();
        r.run(60);
        r.cfg = GestureCfg::SIMPLE; // the screen changed while M was down
        r.raw = 0;
        r.run(100);
        assert_eq!(
            r.gestures(Btn::M),
            0,
            "release still uses the double-click rules"
        );
        r.run(400);
        assert!(r.has(Btn::M, Gesture::Click));
    }
}
