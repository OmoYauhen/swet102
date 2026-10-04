//! Diagnostics screen: platform measurements, motor bus counters and the live
//! button state (the M0 hardware-trial pattern, kept for bench work).
//! M click cycles the SH1107 orientation, PWR click goes back.

use crate::config;
use crate::gfx::{Frame, Mode, num};
use crate::hal::Buttons;
use crate::motor::{MotorDiag, MotorValues};

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct DiagData {
    pub orient: u8,
    pub buttons: Buttons,
    pub tick_avg_us: u32,
    pub tick_max_us: u32,
    pub missed: u32,
    pub stack_free: u32,
    pub ram_kb: u32,
    pub sd_ram_base: u32,
    pub uart_errors: u32,
    pub lcd_avg_us: u32,
    pub lcd_max_us: u32,
    pub saves: u32,
    pub store_errors: u32,
    pub motor: MotorDiag,
    pub values: MotorValues,
}

fn line(f: &mut Frame, y: i32, parts: &[&[u8]]) {
    let mut x = 3;
    for p in parts {
        x = f.text3x5(x, y, p, 1, Mode::Set);
    }
}

pub fn render(f: &mut Frame, d: &DiagData) {
    let (mut a, mut b, mut c) = ([0u8; 10], [0u8; 10], [0u8; 10]);
    let mut h2 = [0u8; 10];
    let mut h = [0u8; 8];
    f.rect(0, 0, 128, 64, Mode::Set);

    // Orientation markers: up arrow + TOP at the top-left, pixel at (1,1),
    // 3×3 block at the bottom-right.
    f.pixel(1, 1, Mode::Set);
    f.vline(5, 3, 8, Mode::Set);
    f.hline(4, 4, 3, Mode::Set);
    f.hline(3, 5, 5, Mode::Set);
    f.text3x5(10, 3, b"TOP", 1, Mode::Set);
    f.fill_rect(124, 60, 3, 3, Mode::Set);
    let x = f.text3x5(30, 3, b"SWET102 V", 1, Mode::Set);
    f.text3x5(x, 3, config::VERSION.as_bytes(), 1, Mode::Set);
    let x = f.text3x5(100, 3, b"OR", 1, Mode::Set);
    f.text3x5(
        x + 2,
        3,
        num::u32_dec(u32::from(d.orient), &mut a),
        1,
        Mode::Set,
    );

    line(
        f,
        12,
        &[
            b"TICK US ",
            num::u32_dec(d.tick_avg_us, &mut a),
            b"/",
            num::u32_dec(d.tick_max_us, &mut b),
            b" MISS ",
            num::u32_dec(d.missed, &mut c),
        ],
    );
    line(
        f,
        19,
        &[
            b"RAM ",
            num::u32_dec(d.ram_kb, &mut a),
            b"K SD ",
            num::u32_hex(d.sd_ram_base, 8, &mut h),
            b" STK ",
            num::u32_dec(d.stack_free, &mut b),
        ],
    );
    let m = &d.motor;
    line(
        f,
        26,
        &[
            b"MOT REQ ",
            num::u32_dec(m.requests, &mut a),
            b" OK ",
            num::u32_dec(m.replies, &mut b),
            b" WR ",
            num::u32_dec(m.writes, &mut c),
        ],
    );
    line(
        f,
        33,
        &[
            b"TMO ",
            num::u32_dec(m.timeouts, &mut a),
            b" CHK ",
            num::u32_dec(m.bad_checksums, &mut b),
            b" STRAY ",
            num::u32_dec(m.stray_bytes, &mut c),
            b" UE ",
            num::u32_dec(d.uart_errors, &mut h2),
        ],
    );
    let v = &d.values;
    fn opt(x: Option<u32>, buf: &mut [u8; 10]) -> &[u8] {
        match x {
            Some(n) => num::u32_dec(n, buf),
            None => b"-",
        }
    }
    let mut e = [0u8; 10];
    line(
        f,
        40,
        &[
            b"RPM ",
            opt(v.rpm.map(u32::from), &mut a),
            b" A ",
            opt(v.current_x2.map(|c| u32::from(c) / 2), &mut b),
            b" SOC ",
            opt(v.soc.map(u32::from), &mut c),
            b" ST ",
            match v.status {
                Some(s) => num::u32_hex(u32::from(s), 2, &mut h),
                None => b"-",
            },
        ],
    );
    let mut e2 = [0u8; 10];
    line(
        f,
        47,
        &[
            b"LCD US ",
            num::u32_dec(d.lcd_avg_us, &mut e),
            b"/",
            num::u32_dec(d.lcd_max_us, &mut e2),
        ],
    );

    for (i, (mask, label)) in [
        (Buttons::LEFT, b"L"),
        (Buttons::RIGHT, b"R"),
        (Buttons::M, b"M"),
        (Buttons::PWR, b"P"),
    ]
    .iter()
    .enumerate()
    {
        let bx = 74 + i as i32 * 13;
        f.rect(bx, 47, 11, 11, Mode::Set);
        if d.buttons.has(*mask) {
            f.fill_rect(bx + 1, 48, 9, 9, Mode::Set);
        }
        f.text3x5(bx + 4, 50, *label, 1, Mode::Xor);
    }
    let (mut s1, mut s2) = ([0u8; 10], [0u8; 10]);
    line(
        f,
        56,
        &[
            b"SAVE ",
            num::u32_dec(d.saves, &mut s1),
            b" SERR ",
            num::u32_dec(d.store_errors, &mut s2),
        ],
    );
}
