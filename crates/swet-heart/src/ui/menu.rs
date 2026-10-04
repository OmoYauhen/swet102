//! The M-hold menu (PRODUCT §5): one item per screen with a big icon, its
//! label and position dots. LEFT/RIGHT flip (wrapping), M enters, PWR backs
//! out. Also the confirm, BLE status and firmware screens it opens.

use crate::config;
use crate::gfx::assets::TEXT;
use crate::gfx::{Frame, Mode, W, num};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Item {
    ResetTrip,
    Ble,
    Diagnostics,
    Firmware,
    Dfu,
}

impl Item {
    /// Ring order (PRODUCT §5).
    pub const ALL: [Item; 5] = [
        Item::ResetTrip,
        Item::Ble,
        Item::Diagnostics,
        Item::Firmware,
        Item::Dfu,
    ];

    fn label(self) -> &'static [u8] {
        match self {
            Item::ResetTrip => b"Reset trip",
            Item::Ble => b"Bluetooth",
            Item::Diagnostics => b"Diagnostics",
            Item::Firmware => b"Firmware",
            Item::Dfu => b"Update (DFU)",
        }
    }

    fn icon(self) -> &'static Icon {
        match self {
            Item::ResetTrip => &ICON_RESET,
            Item::Ble => &ICON_BLE,
            Item::Diagnostics => &ICON_WRENCH,
            Item::Firmware => &ICON_INFO,
            Item::Dfu => &ICON_DOWNLOAD,
        }
    }
}

/// What a confirm screen is asking about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Confirm {
    ResetTrip,
    Dfu,
}

pub struct Menu {
    index: u8,
    /// "Done" replaces the label until this time (after a confirmed reset).
    done_until: u32,
    done: bool,
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

    pub fn show_done(&mut self, now: u32) {
        self.done = true;
        self.done_until = now.wrapping_add(config::MENU_DONE_MS);
    }

    pub fn render(&self, f: &mut Frame, now: u32) {
        let item = self.item();
        draw_icon(f, item.icon(), (W - 32) / 2, 3);
        // side arrows: there is more in both directions (the ring wraps)
        f.text3x5(6, 15, b"<", 2, Mode::Set);
        f.text3x5(W - 12, 15, b">", 2, Mode::Set);

        let showing_done = self.done && now.wrapping_sub(self.done_until) > u32::MAX / 2;
        let label = if showing_done {
            b"Done".as_slice()
        } else {
            item.label()
        };
        f.text(
            &TEXT,
            label,
            (W - TEXT.width(label, 1)) / 2,
            38,
            1,
            Mode::Set,
        );

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
        Confirm::Dfu => b"Reboot to DFU?",
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

/// Phone link state for the BLE screen.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct BleInfo {
    pub connected: bool,
    pub commands: bool,
    /// Little-endian, as the SoftDevice reports it.
    pub address: [u8; 6],
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
    line(f, 40, b"ADDRESS:");
    line(f, 48, &addr);
}

pub fn render_firmware(f: &mut Frame) {
    let title = b"Swet102";
    f.text(
        &TEXT,
        title,
        (W - TEXT.width(title, 1)) / 2,
        1,
        1,
        Mode::Set,
    );
    let rows: [(&[u8], &[u8]); 3] = [
        (b"VERSION ", config::VERSION.as_bytes()),
        (b"BUILD ", config::BUILD_NUM.as_bytes()),
        (b"GIT ", config::GIT_HASH.as_bytes()),
    ];
    for (i, (k, v)) in rows.iter().enumerate() {
        let y = 22 + i as i32 * 10;
        let x = f.text3x5(4, y, k, 1, Mode::Set);
        f.text3x5(x, y, v, 1, Mode::Set);
    }
}

/// 16×16 icon as text art, drawn ×2.
type Icon = [&'static [u8; 16]; 16];

fn draw_icon(f: &mut Frame, icon: &Icon, x: i32, y: i32) {
    for (r, row) in icon.iter().enumerate() {
        for (c, &px) in row.iter().enumerate() {
            if px == b'#' {
                f.fill_rect(x + 2 * c as i32, y + 2 * r as i32, 2, 2, Mode::Set);
            }
        }
    }
}

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
        assert_eq!(m.item(), Item::Dfu);
        m.next();
        m.next();
        assert_eq!(m.item(), Item::Ble);
        m.open();
        assert_eq!(m.item(), Item::ResetTrip);
    }
}
