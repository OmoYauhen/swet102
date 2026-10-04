//! Desktop emulator: the real swet-heart App in a pixel-exact window, with a
//! panel for the fake BBSHD motor and the display's state (like the Swang
//! Stodva emulator's motor panel).
//!
//! Flash is kept in `emu-store.bin` (working directory), so PAS, lock, mode and
//! trips survive a restart like on the bike. `--fresh` starts with empty flash.
//! PWR hold powers off: the window closes once the save has landed.
//!
//! `--snapshot=FILE.png` runs a short scripted ride without a window and saves
//! the whole emulator view (display + panel) to a PNG.

mod canvas;

use std::time::{Duration, Instant};

use canvas::{Canvas, Chip, UiFont};
use minifb::{Key, KeyRepeat, Window, WindowOptions};
use swet_heart::gfx::{H, W};
use swet_heart::motor::codec;
use swet_heart::ui::popup::{Fault, Popup};
use swet_heart::{Buttons, Power, config};
use swet_sim::Sim;

const SCALE: usize = 6;
const STORE_FILE: &str = "emu-store.bin";

// OLED pixels
const LIT: u32 = 0x00E8_F4FF; // white with a hint of blue
const DARK: u32 = 0x0008_0A0C;
const GAP: u32 = 0x0000_0000;
// window chrome
const BG: u32 = 0x001C_1F24;
const FRAME: u32 = 0x003A_3F47;
const GRAY: u32 = 0x008A_929C;
const WHITE: u32 = 0x00E8_ECF0;
const CYAN: u32 = 0x005F_C8E8;
const YELLOW: u32 = 0x00E8_C35F;
const GREEN: u32 = 0x004C_D964;
const RED: u32 = 0x00FF_5B5B;

const BUTTON_CHIP: Chip = Chip {
    on: WHITE,
    off: GRAY,
    bg: BG,
};
const KEY_CHIP: Chip = Chip {
    on: YELLOW,
    off: GRAY,
    bg: BG,
};

const MARGIN: i32 = 14;
const DISP_X: i32 = MARGIN;
const DISP_Y: i32 = 44;
const DISP_W: i32 = W * SCALE as i32;
const DISP_H: i32 = H * SCALE as i32;
const PANEL_X: i32 = DISP_X + DISP_W + 24;
const PANEL_W: i32 = 470;
const WIN_W: i32 = PANEL_X + PANEL_W + MARGIN;
const WIN_H: i32 = 700;
const ROW: i32 = 32;

/// Keys that drive the display's buttons.
const DISPLAY_KEYS: [(&[Key], u8, &str); 4] = [
    (&[Key::Left], Buttons::LEFT, "LEFT  <"),
    (&[Key::Right], Buttons::RIGHT, "RIGHT  >"),
    (&[Key::Down, Key::Space], Buttons::M, "M  v / Space"),
    (&[Key::P, Key::Escape], Buttons::PWR, "PWR  P / Esc"),
];

fn main() {
    let (w, h) = (WIN_W as usize, WIN_H as usize);
    if let Some(path) =
        std::env::args().find_map(|a| a.strip_prefix("--snapshot=").map(String::from))
    {
        return snapshot(&path, w, h);
    }
    let mut window = match Window::new("swet102 emulator", w, h, WindowOptions::default()) {
        Ok(win) => win,
        Err(e) => {
            eprintln!("swet-emu: cannot open a window: {e}");
            std::process::exit(1);
        }
    };
    window.set_target_fps(0);

    let store_path = std::path::PathBuf::from(STORE_FILE);
    let fresh = std::env::args().any(|a| a == "--fresh");
    let stored = if fresh {
        None
    } else {
        std::fs::read(&store_path)
            .ok()
            .and_then(|b| <[u8; swet_heart::STORE_LEN]>::try_from(b.as_slice()).ok())
    };
    let font = UiFont::load();
    let mut sim = Sim::with_store(stored);
    let mut persisted = stored;
    let mut buf = vec![0u32; w * h];
    let start = Instant::now();
    let mut shot = 0u32;
    // the motor only knows whole rpm; keep the km/h target here so ±1 steps never stall
    let mut kmh = 0u32;

    while window.is_open() && !sim.hal().powered_off && !sim.hal().dfu_requested {
        let mut buttons = 0;
        for (keys, b, _) in DISPLAY_KEYS {
            if keys.iter().any(|k| window.is_key_down(*k)) {
                buttons |= b;
            }
        }
        sim.hal_mut().buttons = buttons;

        let step = |k| window.is_key_pressed(k, KeyRepeat::Yes);
        let once = |k| window.is_key_pressed(k, KeyRepeat::No);
        {
            let m = &mut sim.hal_mut().motor;
            if step(Key::W) {
                kmh += 1;
                m.set_speed_kmh(kmh);
            }
            if step(Key::S) {
                kmh = kmh.saturating_sub(1);
                m.set_speed_kmh(kmh);
            }
            if step(Key::E) {
                m.current_x2 = m.current_x2.saturating_add(2);
            }
            if step(Key::D) {
                m.current_x2 = m.current_x2.saturating_sub(2);
            }
            if step(Key::R) {
                m.soc = (m.soc + 5).min(100);
            }
            if step(Key::F) {
                m.soc = m.soc.saturating_sub(5);
            }
            if once(Key::L) {
                m.online = !m.online;
            }
            if once(Key::B) {
                m.status = if m.status == codec::STATUS_BRAKING {
                    codec::STATUS_NORMAL
                } else {
                    codec::STATUS_BRAKING
                };
            }
            if once(Key::X) {
                m.status = if m.status == 0x21 {
                    codec::STATUS_NORMAL
                } else {
                    0x21
                };
            }
        }
        if once(Key::F12) {
            let path = std::path::PathBuf::from(format!("swet102-{shot:03}.png"));
            match swet_sim::write_png(&sim.screen(), &path, SCALE as u32) {
                Ok(()) => eprintln!("saved {}", path.display()),
                Err(e) => eprintln!("screenshot failed: {e}"),
            }
            shot += 1;
        }

        // Run the firmware on wall-clock time, 20 ms per tick.
        let target = start.elapsed().as_millis() as u32;
        while sim.now_ms() + config::TICK_MS <= target && !sim.hal().powered_off {
            sim.tick();
        }

        if sim.hal().store != persisted {
            persisted = sim.hal().store;
            if let Some(bytes) = persisted
                && let Err(e) = std::fs::write(&store_path, bytes)
            {
                eprintln!("swet-emu: cannot write {STORE_FILE}: {e}");
            }
        }

        let held = |keys: &[Key]| keys.iter().any(|k| window.is_key_down(*k));
        let mut c = Canvas {
            buf: &mut buf,
            w,
            h,
            font: &font,
        };
        draw(&mut c, &sim, kmh, &held);
        if let Err(e) = window.update_with_buffer(&buf, w, h) {
            eprintln!("swet-emu: {e}");
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    if sim.hal().dfu_requested {
        eprintln!("swet-emu: the firmware rebooted into DFU mode (menu → Update)");
    }
}

fn draw(c: &mut Canvas, sim: &Sim, kmh: u32, held: &dyn Fn(&[Key]) -> bool) {
    c.fill(0, 0, WIN_W, WIN_H, BG);

    // --- the display ---
    c.text("SW102 display", DISP_X, 12, YELLOW);
    let ver = format!("swet102 v{}", config::VERSION);
    c.text(&ver, DISP_X + DISP_W - c.text_width(&ver), 12, GRAY);
    c.outline(DISP_X - 2, DISP_Y - 2, DISP_W + 4, DISP_H + 4, FRAME);
    oled(c, &sim.screen());

    // buttons, lit while held, and the motor link
    let by = DISP_Y + DISP_H + 14;
    let mut x = DISP_X;
    for (keys, _, label) in DISPLAY_KEYS {
        x = c.chip(label, x, by, held(keys), BUTTON_CHIP);
    }
    let m = &sim.hal().motor;
    let link_up = sim.app().motor().link_up();
    let (dot, txt) = match (m.online, link_up) {
        (false, _) => (RED, "motor off-line"),
        (true, false) => (YELLOW, "motor: no link yet"),
        (true, true) => (GREEN, "motor link up"),
    };
    let lx = DISP_X + DISP_W - c.text_width(txt);
    c.dot(lx - 12, by + 8, 5, dot);
    c.text(txt, lx, by + 2, GRAY);

    // --- the panel ---
    let x0 = PANEL_X;
    let (vx, kx) = (x0 + 136, x0 + 360);
    let mut y = 12;
    let header = |c: &mut Canvas, y: &mut i32, t: &str| {
        c.text(t, x0, *y, CYAN);
        c.fill(x0, *y + 24, PANEL_W, 1, FRAME);
        *y += 34;
    };
    let row = |c: &mut Canvas, y: &mut i32, label: &str, value: &str| {
        c.text(label, x0, *y, GRAY);
        c.text(value, vx, *y, WHITE);
        *y += ROW;
    };
    let keys = |c: &mut Canvas, y: i32, ks: &[(&str, &[Key])]| {
        let mut x = kx;
        for (label, k) in ks {
            x = c.chip(label, x, y - 3, held(k), KEY_CHIP);
        }
    };

    header(c, &mut y, "BBSHD motor (fake)");
    keys(c, y, &[("L", &[Key::L])]);
    row(
        c,
        &mut y,
        "Link",
        if m.online { "answering" } else { "silent" },
    );
    keys(c, y, &[("W", &[Key::W]), ("S", &[Key::S])]);
    row(c, &mut y, "Speed", &format!("{kmh} km/h  ({} rpm)", m.rpm));
    keys(c, y, &[("E", &[Key::E]), ("D", &[Key::D])]);
    row(
        c,
        &mut y,
        "Current",
        &format!("{}.{} A", m.current_x2 / 2, (m.current_x2 % 2) * 5),
    );
    keys(c, y, &[("R", &[Key::R]), ("F", &[Key::F])]);
    row(c, &mut y, "Battery", &format!("{} %", m.soc));
    keys(c, y, &[("B", &[Key::B]), ("X", &[Key::X])]);
    let status = match m.status {
        codec::STATUS_NORMAL => "01 normal".to_string(),
        codec::STATUS_BRAKING => "03 braking".to_string(),
        s => format!("{s:02X} error"),
    };
    row(c, &mut y, "Status", &status);
    y += 4;
    c.text("received from the display", x0, y, GRAY);
    y += ROW;
    let pas = match m.pas_code {
        None => "-".to_string(),
        Some(codec::PAS_WALK) => "walk assist".to_string(),
        Some(code) => (0..=config::PAS_MAX)
            .find(|&l| codec::pas_code(l) == code)
            .map_or_else(|| format!("code {code:02X}"), |l| l.to_string()),
    };
    row(c, &mut y, "  PAS", &pas);
    let lights = match m.lights {
        None => "-",
        Some(true) => "on",
        Some(false) => "off",
    };
    row(c, &mut y, "  Lights", lights);
    let limit = m.speed_limit_wire.map_or_else(
        || "-".to_string(),
        |w| {
            let kmh = if config::SPEED_LIMIT_AS_RPM {
                (u32::from(w) * 60 * config::WHEEL_MM + 500_000) / 1_000_000
            } else {
                u32::from(w) / 10
            };
            format!("{kmh} km/h  (wire {w})")
        },
    );
    row(c, &mut y, "  Limit", &limit);

    y += 8;
    header(c, &mut y, "Display");
    let app = sim.app();
    let st = app.state();
    let pas = if st.walk {
        "walk".to_string()
    } else {
        st.pas.to_string()
    };
    row(c, &mut y, "PAS", &pas);
    let mode = if st.is_sport() { "sport" } else { "city" };
    row(
        c,
        &mut y,
        "Mode",
        &format!("{mode} ({} km/h)", st.speed_limit),
    );
    let lock = if st.locked {
        "locked: PIN at power-on"
    } else {
        "unlocked"
    };
    row(c, &mut y, "Lock", lock);
    let screen = match app.screen() {
        swet_heart::ui::Screen::Menu => format!("Menu: {:?}", app.menu_item()),
        s => format!("{s:?}"),
    };
    row(c, &mut y, "Screen", &screen);
    let popup = match app.popup() {
        Popup::None => "-".to_string(),
        Popup::Fault(Fault::LinkLost) => "motor link lost".to_string(),
        Popup::Fault(Fault::Code(c)) => format!("error {c:02X}"),
        Popup::BatteryTrip(km) => format!("last charge {}.{} km", km / 10, km % 10),
    };
    row(c, &mut y, "Popup", &popup);
    let r = app.rides();
    let km = |m: u32| format!("{}.{}", m / 1000, m / 100 % 10);
    row(
        c,
        &mut y,
        "Trip / Bat",
        &format!("{} / {} km", km(r.trip.m), km(r.batt.m)),
    );
    row(
        c,
        &mut y,
        "Ride / Odo",
        &format!("{} / {} km", km(r.ride.m), km(r.odo_m)),
    );
    let power = match app.power() {
        Power::On => "on",
        Power::Locking { .. } => "locking",
        Power::Off { .. } => "powering off",
        Power::Dfu { .. } => "rebooting to DFU",
    };
    row(c, &mut y, "Power", power);
    let writes = sim.hal().store_writes;
    let plural = if writes == 1 { "" } else { "s" };
    row(c, &mut y, "Flash", &format!("{writes} write{plural}"));

    // legend, under the display
    let ly = by + 52;
    c.fill(DISP_X, ly - 10, DISP_W, 1, FRAME);
    for (i, line) in [
        "F12: PNG of the display     --fresh: start with empty flash",
        "Dev-build PINs: city 1111, sport 2222",
        "Motor: B brake, X error 21, L link, W/S speed, E/D current, R/F battery",
    ]
    .iter()
    .enumerate()
    {
        c.text(line, DISP_X, ly + i as i32 * ROW, GRAY);
    }
}

fn oled(c: &mut Canvas, frame: &swet_heart::Frame) {
    let s = SCALE as i32;
    for y in 0..H {
        for x in 0..W {
            let on = frame.get(x, y);
            let (px, py) = (DISP_X + x * s, DISP_Y + y * s);
            c.fill(px, py, s, s, GAP);
            c.fill(px, py, s - 1, s - 1, if on { LIT } else { DARK });
        }
    }
}

/// Headless: ride a little on fresh flash and save the whole view as a PNG.
fn snapshot(path: &str, w: usize, h: usize) {
    let mut sim = Sim::new();
    let kmh = 27;
    sim.motor().set_speed_kmh(kmh);
    sim.motor().current_x2 = 24;
    sim.boot_to_ride();
    sim.click(Buttons::RIGHT);
    sim.click(Buttons::RIGHT);
    sim.run_ms(90_000);
    let mut buf = vec![0u32; w * h];
    let font = UiFont::load();
    draw(
        &mut Canvas {
            buf: &mut buf,
            w,
            h,
            font: &font,
        },
        &sim,
        kmh,
        &|_| false,
    );
    let rgb: Vec<u8> = buf
        .iter()
        .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8])
        .collect();
    let result = std::fs::File::create(path)
        .map_err(|e| e.to_string())
        .and_then(|f| {
            let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()
                .and_then(|mut wr| wr.write_image_data(&rgb))
                .map_err(|e| e.to_string())
        });
    match result {
        Ok(()) => eprintln!("saved {path}"),
        Err(e) => {
            eprintln!("swet-emu: snapshot failed: {e}");
            std::process::exit(1);
        }
    }
}
