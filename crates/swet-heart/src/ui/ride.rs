//! The riding screen: page tile | info pane | battery (PRODUCT §3).

use super::Model;
use super::anim::{Tween, travel};
use crate::gfx::assets::{SMALL, SPEED, W95};
use crate::gfx::{Frame, H, Mode, num};
use crate::input::{Btn, BtnCfg, GestureCfg};
use crate::rides::Trip;

/// Column 1: white rounded tile.
const TILE: (i32, i32, i32, i32) = (0, 0, 26, 64);
/// Column 2.
const PANE_X: i32 = 28;
const PANE_W: i32 = 86;
/// Column 3.
const BAT_X: i32 = 116;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Pas,
    Lights,
    Player,
    Gate,
}

impl Page {
    /// Ring order (PRODUCT §3.1).
    const RING: [Page; 4] = [Page::Pas, Page::Lights, Page::Player, Page::Gate];

    fn next(self) -> Page {
        let i = Self::RING.iter().position(|&p| p == self).unwrap_or(0);
        Self::RING[(i + 1) % Self::RING.len()]
    }

    /// Pages that send commands to the phone (TECH_DESIGN §5.2).
    pub fn needs_ble(self) -> bool {
        matches!(self, Page::Player | Page::Gate)
    }

    fn gestures(self, cfg: GestureCfg) -> GestureCfg {
        const HOLD: BtnCfg = BtnCfg {
            double: false,
            hold: true,
        };
        match self {
            // LEFT hold at level 0 = walk assist
            Page::Pas => cfg.with(Btn::Left, HOLD),
            Page::Lights | Page::Gate => cfg,
            // hold = previous / next track, RIGHT double = play / pause;
            // only RIGHT waits for a double-click, so volume − stays instant
            Page::Player => cfg.with(Btn::Left, HOLD).with(
                Btn::Right,
                BtnCfg {
                    double: true,
                    hold: true,
                },
            ),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
    Speed,
    Power,
    Trip,
    BattTrip,
    Ride,
    Odo,
}

impl View {
    /// Ring order (PRODUCT §3.2).
    const RING: [View; 6] = [
        View::Speed,
        View::Power,
        View::Trip,
        View::BattTrip,
        View::Ride,
        View::Odo,
    ];

    fn next(self) -> View {
        let i = Self::RING.iter().position(|&v| v == self).unwrap_or(0);
        Self::RING[(i + 1) % Self::RING.len()]
    }
}

/// What the page tile shows. Snapshotted when a slide starts, so the outgoing
/// glyph stays what it was while the incoming one is live.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum TileGlyph {
    Pas(u8),
    Walk,
    Lights(bool),
    Player,
    Gate,
}

impl TileGlyph {
    fn of(page: Page, m: &Model) -> TileGlyph {
        match page {
            Page::Pas if m.walk => TileGlyph::Walk,
            Page::Pas => TileGlyph::Pas(m.pas),
            Page::Lights => TileGlyph::Lights(m.lights),
            Page::Player => TileGlyph::Player,
            Page::Gate => TileGlyph::Gate,
        }
    }
}

/// Animations are compile-time constants (PRODUCT §3.4).
pub const PAGE_SLIDE_MS: u16 = 150;
pub const PANE_SLIDE_MS: u16 = 200;
pub const PAS_SLIDE_MS: u16 = 100;

/// The ride screen's own state. Zero-initialised: `Pas`, `Speed`, idle tweens.
pub struct RideScreen {
    pub page: Page,
    pub view: View,
    /// Inside the tile, `tile_from` leaves and the current glyph arrives:
    /// it comes in from the right, or from the left when `tile_back` (PWR back
    /// to PAS, or a lower PAS level). False by default, so the App stays
    /// zero-initialised.
    tile: Tween,
    tile_from: TileGlyph,
    tile_back: bool,
    /// Info-pane slide: `pane_from` leaves at the top, `view` comes up from the bottom.
    pane: Tween,
    pane_from: View,
}

impl Default for RideScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl RideScreen {
    pub const fn new() -> Self {
        Self {
            page: Page::Pas,
            view: View::Speed,
            tile: Tween::IDLE,
            tile_from: TileGlyph::Pas(0),
            tile_back: false,
            pane: Tween::IDLE,
            pane_from: View::Speed,
        }
    }

    /// M click: next page, sliding in from the right.
    pub fn next_page(&mut self, m: &Model, now: u32) {
        self.slide_tile(TileGlyph::of(self.page, m), false, PAGE_SLIDE_MS, now);
        self.page = self.page.next();
    }

    /// M double-click: the next info view slides up from the bottom.
    pub fn next_view(&mut self, now: u32) {
        self.pane_from = self.view;
        self.pane.start(now, PANE_SLIDE_MS);
        self.view = self.view.next();
    }

    /// PWR click: back to PAS, sliding in from the left (the way back round the ring).
    pub fn goto_pas(&mut self, m: &Model, now: u32) {
        if self.page != Page::Pas {
            self.slide_tile(TileGlyph::of(self.page, m), true, PAGE_SLIDE_MS, now);
            self.page = Page::Pas;
        }
    }

    /// Straight to PAS without animation (unlock).
    pub fn reset_to_pas(&mut self) {
        self.page = Page::Pas;
        self.snap();
    }

    /// PAS changed on the PAS page: the number slides like the pages do — a
    /// higher level comes in from the right, a lower one from the left.
    pub fn slide_pas(&mut self, from: u8, to: u8, now: u32) {
        if self.page == Page::Pas && from != to {
            self.slide_tile(TileGlyph::Pas(from), to < from, PAS_SLIDE_MS, now);
        }
    }

    fn slide_tile(&mut self, from: TileGlyph, back: bool, dur: u16, now: u32) {
        self.tile_from = from;
        self.tile_back = back;
        self.tile.start(now, dur);
    }

    /// Any input finishes running animations first (TECH_DESIGN §5.4).
    pub fn snap(&mut self) {
        self.tile.snap();
        self.pane.snap();
    }

    pub fn animating(&self, now: u32) -> bool {
        self.tile.running(now) || self.pane.running(now)
    }

    pub fn gesture_cfg(&self) -> GestureCfg {
        let both = BtnCfg {
            double: true,
            hold: true,
        };
        self.page
            .gestures(GestureCfg::SIMPLE.with(Btn::M, both).with(Btn::Pwr, both))
    }

    pub fn render(&self, f: &mut Frame, m: &Model, now: u32) {
        // The tile goes first: a glyph sliding out past its right edge is
        // drawn black on the black gap and pane, so the pane drawn after it
        // covers the spill.
        self.render_tile(f, m, now);
        match self.pane.eased(now) {
            Some(e) => {
                // the old view leaves at the top, the new one comes up from the
                // bottom; both live, the screen edges clip them
                let dy = travel(H, e);
                render_pane(f, self.pane_from, m, -dy);
                render_pane(f, self.view, m, H - dy);
            }
            None => render_pane(f, self.view, m, 0),
        }
        render_battery(f, m);
    }

    fn render_tile(&self, f: &mut Frame, m: &Model, now: u32) {
        let (x, y, w, h) = TILE;
        f.round_rect(x, y, w, h, 4, Mode::Set);
        let current = TileGlyph::of(self.page, m);
        // Glyphs are drawn black (Clear) on the white tile: what slides past
        // the tile edge lands off screen or on black, where it leaves no trace.
        match self.tile.eased(now) {
            Some(e) => {
                let d = travel(w, e);
                let sign = if self.tile_back { -1 } else { 1 };
                // forward: the old glyph leaves to the left, the new one arrives from the right
                draw_glyph(f, self.tile_from, m, -d * sign);
                draw_glyph(f, current, m, (w - d) * sign);
            }
            None => draw_glyph(f, current, m, 0),
        }
    }
}

/// One tile glyph, shifted right by `dx`.
fn draw_glyph(f: &mut Frame, g: TileGlyph, m: &Model, dx: i32) {
    let (x, y, w, h) = TILE;
    let x = x + dx;
    match g {
        TileGlyph::Walk => up_arrow(f, x + w / 2, y + h / 2, Mode::Clear),
        TileGlyph::Pas(pas) => {
            let mut d = [0u8; 10];
            let s = num::u32_dec(u32::from(pas), &mut d);
            let gw = W95.width(s, 0);
            let gy = y + (h - i32::from(W95.height)) / 2;
            f.text(&W95, s, x + (w - gw) / 2, gy, 0, Mode::Clear);
        }
        TileGlyph::Lights(on) => {
            let (art, label): (&Glyph, &[u8]) = if on {
                (&BULB_ON, b"ON")
            } else {
                (&BULB_OFF, b"OFF")
            };
            glyph(f, art, x + (w - GLYPH_W) / 2, y + 9, false);
            let lw = label.len() as i32 * 4 - 1;
            f.text3x5(x + (w - lw) / 2, y + 50, label, 1, Mode::Clear);
        }
        // without a subscribed phone the command pages are dithered and
        // ignore LEFT/RIGHT (PRODUCT §3.1)
        TileGlyph::Player => glyph(f, &NOTE, x + (w - GLYPH_W) / 2, y + 20, !m.commands),
        TileGlyph::Gate => glyph(f, &KEY, x + (w - GLYPH_W) / 2, y + 17, !m.commands),
    }
}

/// Page glyph: 11×16 text art drawn ×2, black on the white tile.
type Glyph = [&'static [u8; 11]; 16];
const GLYPH_W: i32 = 22;

/// `dither` keeps every other pixel: the "unavailable" look.
fn glyph(f: &mut Frame, art: &Glyph, x: i32, y: i32, dither: bool) {
    for (r, row) in art.iter().enumerate() {
        for (c, &px) in row.iter().enumerate() {
            if px != b'#' {
                continue;
            }
            let (gx, gy) = (x + 2 * c as i32, y + 2 * r as i32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                if !dither || (gx + dx + gy + dy) % 2 == 0 {
                    f.pixel(gx + dx, gy + dy, Mode::Clear);
                }
            }
        }
    }
}

const BULB_OFF: Glyph = [
    b"...#####...",
    b"..#.....#..",
    b".#.......#.",
    b"#.........#",
    b"#.........#",
    b"#.........#",
    b"#.........#",
    b".#.......#.",
    b"..#.....#..",
    b"...#...#...",
    b"...#####...",
    b"...........",
    b"...#####...",
    b"...........",
    b"....###....",
    b"...........",
];

const BULB_ON: Glyph = [
    b"...#####...",
    b"..#######..",
    b".#########.",
    b"###########",
    b"###########",
    b"###########",
    b"###########",
    b".#########.",
    b"..#######..",
    b"...#####...",
    b"...#####...",
    b"...........",
    b"...#####...",
    b"...........",
    b"....###....",
    b"...........",
];

const NOTE: Glyph = [
    b"....#######",
    b"....#######",
    b"....#.....#",
    b"....#.....#",
    b"....#.....#",
    b"....#.....#",
    b"....#.....#",
    b"....#.....#",
    b"..###...###",
    b".####..####",
    b".####..####",
    b"..##....##.",
    b"...........",
    b"...........",
    b"...........",
    b"...........",
];

const KEY: Glyph = [
    b"...#####...",
    b"..#######..",
    b".###...###.",
    b".##.....##.",
    b".###...###.",
    b"..#######..",
    b"...#####...",
    b"....###....",
    b"....###....",
    b"....#####..",
    b"....###....",
    b"....####...",
    b"....###....",
    b"....#####..",
    b"....###....",
    b"...........",
];

/// Walk-assist glyph: a solid up arrow centred on (`cx`, `cy`).
fn up_arrow(f: &mut Frame, cx: i32, cy: i32, mode: Mode) {
    for i in 0..10 {
        f.hline(cx - i, cy - 14 + i, 2 * i + 1, mode);
    }
    f.fill_rect(cx - 4, cy - 4, 9, 18, mode);
}

/// Big number centred in the pane with a small unit underneath.
fn big_value(f: &mut Frame, dy: i32, value: Option<u32>, unit: &[u8]) {
    let top = 8 + dy;
    match value {
        Some(v) => {
            let mut d = [0u8; 10];
            let s = num::u32_dec(v, &mut d);
            let w = SPEED.width(s, 2);
            f.text(&SPEED, s, PANE_X + (PANE_W - w) / 2, top, 2, Mode::Set);
        }
        None => {
            // no data yet: two dashes
            let cy = top + i32::from(SPEED.height) / 2 - 2;
            f.fill_rect(PANE_X + PANE_W / 2 - 22, cy, 18, 5, Mode::Set);
            f.fill_rect(PANE_X + PANE_W / 2 + 4, cy, 18, 5, Mode::Set);
        }
    }
    let uw = SMALL.width(unit, 1);
    f.text(
        &SMALL,
        unit,
        PANE_X + (PANE_W - uw) / 2,
        dy + 64 - i32::from(SMALL.height) - 3,
        1,
        Mode::Set,
    );
}

/// One info view, shifted down by `dy` (a vertical slide moves it off the
/// top or bottom of the screen, which clips it).
fn render_pane(f: &mut Frame, view: View, m: &Model, dy: i32) {
    match view {
        View::Speed => big_value(f, dy, m.speed_x10.map(|s| (u32::from(s) + 5) / 10), b"km/h"),
        View::Power => {
            big_value(f, dy, m.power_w.map(u32::from), b"W");
            f.text3x5(PANE_X, dy + 1, b"POWER", 1, Mode::Set);
        }
        View::Trip => trip_view(f, dy, b"TRIP", &m.trip),
        View::BattTrip => trip_view(f, dy, b"BAT", &m.batt),
        View::Ride => trip_view(f, dy, b"RIDE", &m.ride),
        View::Odo => odo_view(f, dy, m),
    }
}

/// Distance large with "km", centred in the pane. Below 1000 km with one
/// decimal, above that in whole km.
fn distance(f: &mut Frame, dy: i32, m: u32) {
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
    let x = PANE_X + (PANE_W - (vw + 3 + uw)) / 2;
    let y = dy + 7;
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

fn line(f: &mut Frame, y: i32, parts: &[&[u8]]) {
    let mut x = PANE_X;
    for p in parts {
        x = f.text3x5(x, y, p, 1, Mode::Set);
    }
}

/// PRODUCT §3.2: distance, then max and average, then charge used.
fn trip_view(f: &mut Frame, dy: i32, label: &[u8], t: &Trip) {
    f.text3x5(PANE_X, dy + 1, label, 1, Mode::Set);
    distance(f, dy, t.m);
    let (mut a, mut b) = ([0u8; 12], [0u8; 12]);
    line(
        f,
        dy + 45,
        &[
            b"MAX ",
            num::u32_dec1(u32::from(t.max_x10), &mut a),
            b"  AVG ",
            num::u32_dec1(u32::from(t.avg_x10()), &mut b),
        ],
    );
    let mut c = [0u8; 13];
    line(f, dy + 53, &[num::u32_dec2(t.mah / 10, &mut c), b" AH"]);
}

/// Total distance and the all-time max speed.
fn odo_view(f: &mut Frame, dy: i32, m: &Model) {
    f.text3x5(PANE_X, dy + 1, b"ODO", 1, Mode::Set);
    distance(f, dy, m.odo_m);
    let mut a = [0u8; 12];
    line(
        f,
        dy + 45,
        &[b"MAX ", num::u32_dec1(u32::from(m.odo_max_x10), &mut a)],
    );
}

/// Battery icon (fill = SoC) with the percentage under it.
fn render_battery(f: &mut Frame, m: &Model) {
    let (x, y, w, h) = (BAT_X + 1, 4, 10, 44);
    f.fill_rect(x + 3, y - 2, 4, 2, Mode::Set); // terminal
    f.rect(x, y, w, h, Mode::Set);
    if let Some(soc) = m.soc {
        let inner = h - 4;
        let fill = inner * i32::from(soc.min(100)) / 100;
        f.fill_rect(x + 2, y + 2 + inner - fill, w - 4, fill, Mode::Set);
        let mut d = [0u8; 10];
        let s = num::u32_dec(u32::from(soc.min(100)), &mut d);
        let tw = s.len() as i32 * 4 - 1;
        f.text3x5(BAT_X + (12 - tw) / 2 + 1, 56, s, 1, Mode::Set);
    } else {
        f.text3x5(BAT_X + 3, 56, b"--", 1, Mode::Set);
    }
    if m.sport {
        bolt(f, x + 2, y + 6);
    }
}

/// Sport mode (PRODUCT §3.3): a lightning bolt XOR-ed over the battery body,
/// black on the filled part and white on the empty part. 6 px wide, every
/// row drawn twice → 18 px tall.
fn bolt(f: &mut Frame, x: i32, y: i32) {
    const ROWS: [&[u8; 6]; 9] = [
        b"....##", b"...##.", b"..##..", b".##...", b"######", b"...##.", b"..##..", b".##...",
        b"##....",
    ];
    for (r, row) in ROWS.iter().enumerate() {
        for (c, &px) in row.iter().enumerate() {
            if px == b'#' {
                let (px_x, px_y) = (x + c as i32, y + 2 * r as i32);
                f.pixel(px_x, px_y, Mode::Xor);
                f.pixel(px_x, px_y + 1, Mode::Xor);
            }
        }
    }
}
