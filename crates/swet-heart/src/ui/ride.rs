//! The riding screen: page tile | info pane | battery (PRODUCT §3).

use super::Model;
use crate::gfx::assets::{SMALL, SPEED, W95};
use crate::gfx::{Frame, Mode, num};
use crate::input::{Btn, BtnCfg, GestureCfg};

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
}

impl Page {
    /// Ring order (PRODUCT §3.1). Lights, Player and Gate join in later milestones.
    const RING: [Page; 1] = [Page::Pas];

    fn next(self) -> Page {
        let i = Self::RING.iter().position(|&p| p == self).unwrap_or(0);
        Self::RING[(i + 1) % Self::RING.len()]
    }

    fn gestures(self, cfg: GestureCfg) -> GestureCfg {
        match self {
            // LEFT hold at level 0 = walk assist
            Page::Pas => cfg.with(
                Btn::Left,
                BtnCfg {
                    double: false,
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
}

impl View {
    /// Ring order (PRODUCT §3.2). Trip, battery trip, ride and odo come in M4.
    const RING: [View; 2] = [View::Speed, View::Power];

    fn next(self) -> View {
        let i = Self::RING.iter().position(|&v| v == self).unwrap_or(0);
        Self::RING[(i + 1) % Self::RING.len()]
    }
}

pub struct RideScreen {
    pub page: Page,
    pub view: View,
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
        }
    }

    pub fn next_page(&mut self) {
        self.page = self.page.next();
    }

    pub fn next_view(&mut self) {
        self.view = self.view.next();
    }

    pub fn goto_pas(&mut self) {
        self.page = Page::Pas;
    }

    pub fn gesture_cfg(&self) -> GestureCfg {
        let both = BtnCfg {
            double: true,
            hold: true,
        };
        self.page
            .gestures(GestureCfg::SIMPLE.with(Btn::M, both).with(Btn::Pwr, both))
    }

    pub fn render(&self, f: &mut Frame, m: &Model) {
        self.render_tile(f, m);
        render_pane(f, self.view, m);
        render_battery(f, m);
    }

    fn render_tile(&self, f: &mut Frame, m: &Model) {
        let (x, y, w, h) = TILE;
        f.round_rect(x, y, w, h, 4, Mode::Set);
        match self.page {
            Page::Pas if m.walk => up_arrow(f, x + w / 2, y + h / 2, Mode::Clear),
            Page::Pas => {
                let mut d = [0u8; 10];
                let s = num::u32_dec(u32::from(m.pas), &mut d);
                let gw = W95.width(s, 0);
                let gy = y + (h - i32::from(W95.height)) / 2;
                f.text(&W95, s, x + (w - gw) / 2, gy, 0, Mode::Clear);
            }
        }
    }
}

/// Walk-assist glyph: a solid up arrow centred on (`cx`, `cy`).
fn up_arrow(f: &mut Frame, cx: i32, cy: i32, mode: Mode) {
    for i in 0..10 {
        f.hline(cx - i, cy - 14 + i, 2 * i + 1, mode);
    }
    f.fill_rect(cx - 4, cy - 4, 9, 18, mode);
}

/// Big number centred in the pane with a small unit underneath.
fn big_value(f: &mut Frame, value: Option<u32>, unit: &[u8]) {
    let top = 8;
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
        64 - i32::from(SMALL.height) - 3,
        1,
        Mode::Set,
    );
}

fn render_pane(f: &mut Frame, view: View, m: &Model) {
    match view {
        View::Speed => big_value(f, m.speed_x10.map(|s| (u32::from(s) + 5) / 10), b"km/h"),
        View::Power => {
            big_value(f, m.power_w.map(u32::from), b"W");
            f.text3x5(PANE_X, 1, b"POWER", 1, Mode::Set);
        }
    }
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
