//! The M-hold menu (PRODUCT §5): one item per screen with a big icon, its
//! label and position dots. LEFT/RIGHT flip (wrapping), M enters, PWR backs
//! out. Also the confirm, lights, BLE status and firmware screens it opens
//! (Special thanks has its own module, `ui/thanks.rs`).

use crate::config;
use crate::gfx::assets::{QR_REPO, TEXT};
use crate::gfx::{Frame, Mode, W, num};
use crate::lights::{Lights, LightsMode};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Item {
    Lights,
    ResetTrip,
    Ble,
    Diagnostics,
    Firmware,
    Dfu,
    Thanks,
}

impl Item {
    /// Ring order (PRODUCT §5).
    pub const ALL: [Item; 7] = [
        Item::Lights,
        Item::ResetTrip,
        Item::Ble,
        Item::Diagnostics,
        Item::Firmware,
        Item::Dfu,
        Item::Thanks,
    ];

    fn label(self) -> &'static [u8] {
        match self {
            Item::Lights => b"Lights",
            Item::ResetTrip => b"Reset trip",
            Item::Ble => b"Bluetooth",
            Item::Diagnostics => b"Diagnostics",
            Item::Firmware => b"Firmware",
            Item::Dfu => b"Update (DFU)",
            Item::Thanks => b"Special thanks",
        }
    }

    fn icon(self) -> &'static Icon {
        match self {
            Item::Lights => &ICON_BULB,
            Item::ResetTrip => &ICON_RESET,
            Item::Ble => &ICON_BLE,
            Item::Diagnostics => &ICON_WRENCH,
            Item::Firmware => &ICON_INFO,
            Item::Dfu => &ICON_DOWNLOAD,
            Item::Thanks => &ICON_HEART,
        }
    }
}

/// What a confirm screen is asking about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Confirm {
    ResetTrip,
}

pub struct Menu {
    index: u8,
    /// "Done" replaces the label until this time (after a confirmed reset).
    done_until: u32,
    done: bool,
    /// Lights screen: the highlighted choice, index into `LightsMode::ALL`.
    choice: u8,
}

impl Default for Menu {
    fn default() -> Self {
        Self::new()
    }
}

impl Menu {
    pub const fn new() -> Self {
        Self {
            index: 0,
            done_until: 0,
            done: false,
            choice: 0,
        }
    }

    /// Opening always starts at the first item (PRODUCT §5).
    pub fn open(&mut self) {
        *self = Self::new();
    }

    pub fn item(&self) -> Item {
        Item::ALL[usize::from(self.index)]
    }

    pub fn next(&mut self) {
        self.index = (self.index + 1) % Item::ALL.len() as u8;
    }

    pub fn prev(&mut self) {
        let n = Item::ALL.len() as u8;
        self.index = (self.index + n - 1) % n;
    }

    /// Lights screen opens on the mode in use.
    pub fn open_lights(&mut self, mode: LightsMode) {
        self.choice = mode as u8;
    }

    /// LEFT/RIGHT on the lights screen (wraps, like the item ring).
    pub fn choice_step(&mut self, forward: bool) {
        let n = LightsMode::ALL.len() as u8;
        self.choice = (self.choice + if forward { 1 } else { n - 1 }) % n;
    }

    pub fn choice(&self) -> LightsMode {
        LightsMode::ALL[usize::from(self.choice) % LightsMode::ALL.len()]
    }

    pub fn show_done(&mut self, now: u32) {
        self.done = true;
        self.done_until = now.wrapping_add(config::MENU_DONE_MS);
    }

    /// `lights`: the mode in use, shown on the Lights item.
    pub fn render(&self, f: &mut Frame, now: u32, lights: LightsMode) {
        let item = self.item();
        draw_icon(f, item.icon(), (W - 32) / 2, 3);
        // side arrows: there is more in both directions (the ring wraps)
        f.text3x5(6, 15, b"<", 2, Mode::Set);
        f.text3x5(W - 12, 15, b">", 2, Mode::Set);

        let showing_done = self.done && now.wrapping_sub(self.done_until) > u32::MAX / 2;
        let mut buf = [0u8; 16];
        let label = if showing_done {
            b"Done".as_slice()
        } else if item == Item::Lights {
            // "Lights: AUTO": the mode is visible without opening the item
            let (name, mode) = (item.label(), lights.label());
            let n = name.len() + 2 + mode.len();
            buf[..name.len()].copy_from_slice(name);
            buf[name.len()..name.len() + 2].copy_from_slice(b": ");
            buf[name.len() + 2..n].copy_from_slice(mode);
            &buf[..n]
        } else {
            item.label()
        };
        let x = (W - TEXT.width(label, 1)) / 2;
        f.text(&TEXT, label, x, 38, 1, Mode::Set);

        let n = Item::ALL.len() as i32;
        let x0 = (W - (n * 8 - 4)) / 2;
        for i in 0..n {
            let x = x0 + i * 8;
            if i == i32::from(self.index) {
                f.fill_rect(x, 57, 4, 4, Mode::Set);
            } else {
                f.rect(x, 57, 4, 4, Mode::Set);
            }
        }
    }
}

pub fn render_confirm(f: &mut Frame, c: Confirm) {
    let q: &[u8] = match c {
        Confirm::ResetTrip => b"Reset trip?",
    };
    f.text(&TEXT, q, (W - TEXT.width(q, 1)) / 2, 14, 1, Mode::Set);
    let hint = b"M = YES    PWR = NO";
    f.text3x5(
        (W - (hint.len() as i32 * 4 - 1)) / 2,
        44,
        hint,
        1,
        Mode::Set,
    );
}

/// Menu → Lights: OFF / ON / AUTO side by side, the highlighted one
/// inverted; when AUTO switches, and the hint.
pub fn render_lights(f: &mut Frame, choice: LightsMode, l: &Lights, now: u32) {
    let title = b"Lights";
    f.text(
        &TEXT,
        title,
        (W - TEXT.width(title, 1)) / 2,
        1,
        1,
        Mode::Set,
    );
    const BOX_W: i32 = 38;
    const GAP: i32 = 4;
    let x0 = (W - (3 * BOX_W + 2 * GAP)) / 2;
    for (i, m) in LightsMode::ALL.iter().enumerate() {
        let x = x0 + i as i32 * (BOX_W + GAP);
        let label = m.label();
        let tx = x + (BOX_W - TEXT.width(label, 1)) / 2;
        if *m == choice {
            f.round_rect(x, 19, BOX_W, 17, 3, Mode::Set);
            f.text(&TEXT, label, tx, 21, 1, Mode::Clear);
        } else {
            f.text(&TEXT, label, tx, 21, 1, Mode::Set);
        }
    }

    // AUTO needs the phone's time; say whether it has it and what it does
    let mut x = 4;
    match (l.clock.secs_of_day(now), l.clock.dark_window()) {
        (Some(t), Some((from, until))) => {
            for s in [
                b"NOW ".as_slice(),
                &hhmm(t / 60),
                b"  AUTO ON ",
                &hhmm(u32::from(from)),
                b"-",
                &hhmm(u32::from(until)),
            ] {
                x = f.text3x5(x, 43, s, 1, Mode::Set);
            }
        }
        _ => {
            f.text3x5(x, 43, b"AUTO: NO TIME FROM PHONE", 1, Mode::Set);
        }
    }
    let hint = b"M = SET    PWR = BACK";
    f.text3x5(
        (W - (hint.len() as i32 * 4 - 1)) / 2,
        56,
        hint,
        1,
        Mode::Set,
    );
}

/// Minutes since midnight as `HH:MM`.
fn hhmm(min: u32) -> [u8; 5] {
    let (h, m) = (min / 60 % 24, min % 60);
    [
        b'0' + (h / 10) as u8,
        b'0' + (h % 10) as u8,
        b':',
        b'0' + (m / 10) as u8,
        b'0' + (m % 10) as u8,
    ]
}

/// Phone link state for the BLE screen.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct BleInfo {
    pub connected: bool,
    pub commands: bool,
    /// Little-endian, as the SoftDevice reports it.
    pub address: [u8; 6],
    /// Commands sent to the phone since power-on.
    pub sent: u32,
    /// Notifications the stack had no buffer for.
    pub dropped: u32,
}

pub fn render_ble(f: &mut Frame, b: &BleInfo) {
    let title = b"Bluetooth";
    f.text(
        &TEXT,
        title,
        (W - TEXT.width(title, 1)) / 2,
        1,
        1,
        Mode::Set,
    );
    let line = |f: &mut Frame, y: i32, s: &[u8]| {
        f.text3x5(4, y, s, 1, Mode::Set);
    };
    line(
        f,
        20,
        if b.connected {
            b"PHONE: CONNECTED"
        } else {
            b"PHONE: NOT CONNECTED"
        },
    );
    line(
        f,
        30,
        if b.commands {
            b"COMMANDS: ON"
        } else {
            b"COMMANDS: OFF"
        },
    );
    // address most significant byte first, as phones show it
    let mut addr = [0u8; 17];
    for (i, byte) in b.address.iter().rev().enumerate() {
        let mut h = [0u8; 8];
        addr[i * 3..i * 3 + 2].copy_from_slice(num::u32_hex(u32::from(*byte), 2, &mut h));
        if i < 5 {
            addr[i * 3 + 2] = b':';
        }
    }
    line(f, 39, b"ADDRESS:");
    line(f, 46, &addr);
    let (mut a, mut d) = ([0u8; 10], [0u8; 10]);
    let mut x = 4;
    for s in [
        b"SENT ".as_slice(),
        num::u32_dec(b.sent, &mut a),
        b"  DROPPED ",
        num::u32_dec(b.dropped, &mut d),
    ] {
        x = f.text3x5(x, 56, s, 1, Mode::Set);
    }
}

/// A QR code to the GitHub repo on the left, version info on the right.
pub fn render_firmware(f: &mut Frame) {
    const TX: i32 = 68;
    f.text(&TEXT, b"Swet102", TX, 1, 1, Mode::Set);
    let rows: [(&[u8], i32); 5] = [
        (config::VERSION.as_bytes(), 20),
        (b"BUILD", 31),
        (config::BUILD_NUM.as_bytes(), 38),
        (b"GIT", 49),
        (config::GIT_HASH.as_bytes(), 56),
    ];
    for (s, y) in rows {
        f.text3x5(TX, y, s, 1, Mode::Set);
    }

    // QR: dark modules on a lit square, 2 px per module; the lit margin around
    // it is the quiet zone scanners need (3 px = 1.5 modules for version 3).
    const QR_X: i32 = 0;
    f.fill_rect(QR_X, 0, 64, 64, Mode::Set);
    let (qw, qh) = (i32::from(QR_REPO.w), i32::from(QR_REPO.h));
    let (x0, y0) = (QR_X + (64 - 2 * qw) / 2, (64 - 2 * qh) / 2);
    let stride = usize::from(QR_REPO.stride);
    for y in 0..qh {
        for x in 0..qw {
            let byte = QR_REPO.bits[y as usize * stride + (x >> 3) as usize];
            if byte & (1 << (x & 7)) != 0 {
                f.fill_rect(x0 + 2 * x, y0 + 2 * y, 2, 2, Mode::Clear);
            }
        }
    }
}

/// Update, step 1 (menu → Update, or `DFU!` from the phone): how to get into
/// the bootloader's update mode. Its only way in that keeps the display
/// powered is PWR held for 5 s from its start: the reset releases our power
/// latch, and the held button keeps the board on until the bootloader sets
/// it again (TECH_DESIGN §9.5).
pub fn render_update_arm(f: &mut Frame) {
    update_screen(
        f,
        &[
            (b"PRESS AND HOLD PWR", 22),
            (b"FOR 5 S TO START", 30),
            (b"UPDATE MODE", 38),
            (b"M: BACK", 54),
        ],
    );
}

/// Update, step 2: the last frame before the reset, so it is what the OLED
/// keeps while PWR is held and the bootloader runs.
pub fn render_update_hold(f: &mut Frame) {
    update_screen(
        f,
        &[
            (b"KEEP HOLDING PWR", 22),
            (b"FOR 5 S, THEN:", 30),
            (b"NRF TOOLBOX: DFU", 38),
            (b"DEVICE SW102_DFU", 46),
            (b"AFTER: OFF, THEN ON", 56),
        ],
    );
}

fn update_screen(f: &mut Frame, lines: &[(&[u8], i32)]) {
    draw_icon(f, &ICON_DOWNLOAD, 3, 14);
    const TX: i32 = 42;
    f.text(&TEXT, b"Update", TX, 2, 1, Mode::Set);
    for &(s, y) in lines {
        f.text3x5(TX, y, s, 1, Mode::Set);
    }
}

/// 16×16 icon as text art, drawn ×2.
type Icon = [&'static [u8; 16]; 16];

fn draw_icon(f: &mut Frame, icon: &Icon, x: i32, y: i32) {
    draw_art(f, icon, x, y, 2);
}

/// Text art (`#` = lit), each pixel drawn `px`×`px`, top-left at (`x`, `y`).
pub fn draw_art<R: AsRef<[u8]>>(f: &mut Frame, art: &[R], x: i32, y: i32, px: i32) {
    for (r, row) in art.iter().enumerate() {
        for (c, &p) in row.as_ref().iter().enumerate() {
            if p == b'#' {
                f.fill_rect(x + px * c as i32, y + px * r as i32, px, px, Mode::Set);
            }
        }
    }
}

/// Small filled heart, 9×8: above the names on the Special thanks screen.
pub const HEART: [&[u8; 9]; 8] = [
    b".##...##.",
    b"####.####",
    b"#########",
    b"#########",
    b".#######.",
    b"..#####..",
    b"...###...",
    b"....#....",
];

const ICON_HEART: Icon = [
    b"................",
    b"..###......###..",
    b".#...#....#...#.",
    b"#.....#..#.....#",
    b"#......##......#",
    b"#..............#",
    b"#..............#",
    b".#............#.",
    b"..#..........#..",
    b"...#........#...",
    b"....#......#....",
    b".....#....#.....",
    b"......#..#......",
    b".......##.......",
    b"................",
    b"................",
];

const ICON_BULB: Icon = [
    b".....######.....",
    b"...##......##...",
    b"..#..........#..",
    b".#............#.",
    b".#............#.",
    b".#............#.",
    b".#............#.",
    b"..#..........#..",
    b"...#........#...",
    b"....#......#....",
    b"....########....",
    b"................",
    b"....########....",
    b"................",
    b".....######.....",
    b"................",
];

const ICON_RESET: Icon = [
    b"......#####.....",
    b"....##.....##.#.",
    b"...#.........##.",
    b"..#.........###.",
    b"..#.............",
    b".#..............",
    b".#..............",
    b".#..............",
    b".#..............",
    b".#.............#",
    b"..#............#",
    b"..#...........#.",
    b"...#.........#..",
    b"....##.....##...",
    b"......#####.....",
    b"................",
];

const ICON_BLE: Icon = [
    b".......#........",
    b".......##.......",
    b".......#.#......",
    b"...#...#..#.....",
    b"....#..#...#....",
    b".....#.#..#.....",
    b"......###.......",
    b".......#........",
    b"......###.......",
    b".....#.#..#.....",
    b"....#..#...#....",
    b"...#...#..#.....",
    b".......#.#......",
    b".......##.......",
    b".......#........",
    b"................",
];

const ICON_WRENCH: Icon = [
    b"...........###..",
    b"..........#..##.",
    b".........#....#.",
    b".........#...#.#",
    b"........#...##..",
    b".......#...#....",
    b"......#...#.....",
    b".....#...#......",
    b"....#...#.......",
    b"...#...#........",
    b"..#...#.........",
    b".#...#..........",
    b"#...#...........",
    b"#..#............",
    b".##.............",
    b"................",
];

const ICON_INFO: Icon = [
    b".....######.....",
    b"...##......##...",
    b"..#....##....#..",
    b".#.....##.....#.",
    b".#............#.",
    b"#.....###......#",
    b"#......##......#",
    b"#......##......#",
    b"#......##......#",
    b"#......##......#",
    b".#.....##.....#.",
    b".#....####....#.",
    b"..#..........#..",
    b"...##......##...",
    b".....######.....",
    b"................",
];

const ICON_DOWNLOAD: Icon = [
    b".......##.......",
    b".......##.......",
    b".......##.......",
    b".......##.......",
    b".......##.......",
    b".......##.......",
    b"...##..##..##...",
    b"....##.##.##....",
    b".....######.....",
    b"......####......",
    b".......##.......",
    b"#..............#",
    b"#..............#",
    b"#..............#",
    b"################",
    b"................",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_wraps_both_ways_and_reopens_at_the_top() {
        let mut m = Menu::new();
        m.prev();
        assert_eq!(m.item(), Item::Thanks, "last in the ring");
        m.prev();
        assert_eq!(m.item(), Item::Dfu);
        m.next();
        m.next();
        m.next();
        assert_eq!(m.item(), Item::ResetTrip);
        m.open();
        assert_eq!(m.item(), Item::Lights);
    }

    #[test]
    fn lights_choice_starts_on_the_mode_in_use_and_wraps() {
        let mut m = Menu::new();
        m.open_lights(LightsMode::Auto);
        assert_eq!(m.choice(), LightsMode::Auto);
        m.choice_step(true);
        assert_eq!(m.choice(), LightsMode::Off);
        m.choice_step(false);
        m.choice_step(false);
        assert_eq!(m.choice(), LightsMode::On);
    }

    #[test]
    fn hhmm_pads() {
        assert_eq!(&hhmm(0), b"00:00");
        assert_eq!(&hhmm(18 * 60 + 5), b"18:05");
        assert_eq!(&hhmm(1439), b"23:59");
    }
}
