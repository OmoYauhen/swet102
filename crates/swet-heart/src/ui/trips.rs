//! Menu → Trips (PRODUCT §5): RIDE / TRIP / BAT / ODO. LEFT/RIGHT step
//! through them (wrapping, sliding sideways), M asks to reset the trip, PWR
//! backs out. Also Menu → Power, the wattmeter.

use super::Model;
use super::anim::{Tween, travel};
use super::ride::big_value;
use crate::gfx::assets::{SMALL, SPEED};
use crate::gfx::{Frame, Mode, W, num};
use crate::rides::Trip;

/// The views' column: the screen between the side arrows.
const VIEW_X: i32 = 13;
const VIEW_W: i32 = W - 2 * VIEW_X;

/// `Ride` first, so the App stays zero-initialised.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum View {
    Ride,
    Trip,
    BattTrip,
    Odo,
}

impl View {
    /// Ring order (PRODUCT §5).
    pub const RING: [View; 4] = [View::Ride, View::Trip, View::BattTrip, View::Odo];

    fn step(self, forward: bool) -> View {
        let n = Self::RING.len();
        let i = Self::RING.iter().position(|&v| v == self).unwrap_or(0);
        Self::RING[(i + if forward { 1 } else { n - 1 }) % n]
    }
}

pub const SLIDE_MS: u16 = 150;

pub struct TripsScreen {
    pub view: View,
    /// `from` leaves and `view` arrives: from the right, or from the left
    /// when `back` (LEFT).
    slide: Tween,
    from: View,
    back: bool,
}

impl Default for TripsScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl TripsScreen {
    pub const fn new() -> Self {
        Self {
            view: View::Ride,
            slide: Tween::IDLE,
            from: View::Ride,
            back: false,
        }
    }

    /// Opening always starts at RIDE, like the menu at its first item.
    pub fn open(&mut self) {
        *self = Self::new();
    }

    /// LEFT/RIGHT: the next view slides in from that side.
    pub fn step(&mut self, forward: bool, now: u32) {
        self.from = self.view;
        self.back = !forward;
        self.slide.start(now, SLIDE_MS);
        self.view = self.view.step(forward);
    }

    /// Straight to `v` (after a trip reset: show the zeroed TRIP).
    pub fn show(&mut self, v: View) {
        self.view = v;
        self.slide.snap();
    }

    pub fn snap(&mut self) {
        self.slide.snap();
    }

    pub fn animating(&self, now: u32) -> bool {
        self.slide.running(now)
    }

    pub fn render(&self, f: &mut Frame, m: &Model, now: u32) {
        match self.slide.eased(now) {
            Some(e) => {
                // both live, the screen edges clip them
                let d = travel(W, e);
                let sign = if self.back { -1 } else { 1 };
                render_view(f, self.from, m, -d * sign);
                render_view(f, self.view, m, (W - d) * sign);
            }
            None => render_view(f, self.view, m, 0),
        }
        // side arrows, as on the menu: the ring wraps both ways. Drawn last on
        // a cleared patch, so a sliding view passes under them.
        for (x, s) in [(3, b"<"), (W - 9, b">")] {
            f.fill_rect(x - 1, 18, 8, 12, Mode::Clear);
            f.text3x5(x, 19, s, 2, Mode::Set);
        }
    }
}

/// One view, shifted right by `dx`.
fn render_view(f: &mut Frame, view: View, m: &Model, dx: i32) {
    let x = VIEW_X + dx;
    match view {
        View::Ride => trip_view(f, x, b"RIDE", &m.ride),
        View::Trip => trip_view(f, x, b"TRIP", &m.trip),
        View::BattTrip => trip_view(f, x, b"BAT", &m.batt),
        View::Odo => odo_view(f, x, m),
    }
}

/// Menu → Power: the live wattmeter (motor current × pack voltage).
pub fn render_power(f: &mut Frame, m: &Model) {
    big_value(f, &SPEED, 2, VIEW_X, VIEW_W, m.power_w.map(u32::from), b"W");
    f.text3x5(VIEW_X, 1, b"POWER", 1, Mode::Set);
}

/// Distance large with "km", centred in the column at `x`. Below 1000 km
/// with one decimal, above that in whole km.
fn distance(f: &mut Frame, x: i32, m: u32) {
    let mut d = [0u8; 12];
    let s = if m < 1_000_000 {
        num::u32_dec1(m / 100, &mut d)
    } else {
        let mut w = [0u8; 10];
        let s = num::u32_dec(m / 1000, &mut w);
        d[..s.len()].copy_from_slice(s);
        &d[..s.len()]
    };
    let (vw, uw) = (SPEED.width(s, 1), SMALL.width(b"km", 1));
    let x = x + (VIEW_W - (vw + 3 + uw)) / 2;
    let y = 7;
    f.text(&SPEED, s, x, y, 1, Mode::Set);
    let base = y + i32::from(SPEED.height);
    f.text(
        &SMALL,
        b"km",
        x + vw + 3,
        base - i32::from(SMALL.height),
        1,
        Mode::Set,
    );
}

fn line(f: &mut Frame, x: i32, y: i32, parts: &[&[u8]]) {
    let mut x = x;
    for p in parts {
        x = f.text3x5(x, y, p, 1, Mode::Set);
    }
}

/// PRODUCT §3.2: distance, then max and average, then charge used.
fn trip_view(f: &mut Frame, x: i32, label: &[u8], t: &Trip) {
    f.text3x5(x, 1, label, 1, Mode::Set);
    distance(f, x, t.m);
    let (mut a, mut b) = ([0u8; 12], [0u8; 12]);
    line(
        f,
        x,
        45,
        &[
            b"MAX ",
            num::u32_dec1(u32::from(t.max_x10), &mut a),
            b"  AVG ",
            num::u32_dec1(u32::from(t.avg_x10()), &mut b),
        ],
    );
    let mut c = [0u8; 13];
    line(f, x, 53, &[num::u32_dec2(t.mah / 10, &mut c), b" AH"]);
}

/// Total distance and the all-time max speed.
fn odo_view(f: &mut Frame, x: i32, m: &Model) {
    f.text3x5(x, 1, b"ODO", 1, Mode::Set);
    distance(f, x, m.odo_m);
    let mut a = [0u8; 12];
    line(
        f,
        x,
        45,
        &[b"MAX ", num::u32_dec1(u32::from(m.odo_max_x10), &mut a)],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_wraps_both_ways_and_reopens_at_ride() {
        let mut t = TripsScreen::new();
        t.step(false, 0);
        assert_eq!(t.view, View::Odo, "last in the ring");
        t.step(true, 0);
        t.step(true, 0);
        assert_eq!(t.view, View::Trip);
        t.open();
        assert_eq!(t.view, View::Ride);
    }
}
