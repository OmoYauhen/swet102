//! Desktop emulator: the real swet-heart App in a pixel-exact window.
//!
//! Display keys: ← → LEFT/RIGHT · ↓ or Space M · P or Esc PWR.
//! Motor knobs: W/S speed ±1 km/h · E/D current ±1 A · R/F battery ±5 % ·
//! L link on/off · F12 screenshot (PNG in the working directory).

use std::time::{Duration, Instant};

use minifb::{Key, KeyRepeat, Window, WindowOptions};
use swet_heart::gfx::{H, W};
use swet_heart::{Buttons, config};
use swet_sim::Sim;

const SCALE: usize = 6;
const LIT: u32 = 0x00E8_F4FF; // OLED white with a hint of blue
const DARK: u32 = 0x0008_0A0C;
const GAP: u32 = 0x0000_0000;

fn main() {
    let (w, h) = (W as usize * SCALE, H as usize * SCALE);
    let mut window = match Window::new("swet102 emu", w, h, WindowOptions::default()) {
        Ok(win) => win,
        Err(e) => {
            eprintln!("swet-emu: cannot open a window: {e}");
            std::process::exit(1);
        }
    };
    window.set_target_fps(0);

    let mut sim = Sim::new();
    let mut buf = vec![0u32; w * h];
    let start = Instant::now();
    let mut shot = 0u32;
    // the motor only knows whole rpm; keep the km/h target here so ±1 steps never stall
    let mut kmh = 0u32;

    while window.is_open() && !sim.hal().powered_off {
        let mut buttons = 0;
        for (keys, b) in [
            (&[Key::Left][..], Buttons::LEFT),
            (&[Key::Right][..], Buttons::RIGHT),
            (&[Key::Down, Key::Space][..], Buttons::M),
            (&[Key::P, Key::Escape][..], Buttons::PWR),
        ] {
            if keys.iter().any(|k| window.is_key_down(*k)) {
                buttons |= b;
            }
        }
        sim.hal_mut().buttons = buttons;

        let speed_step = |k| window.is_key_pressed(k, KeyRepeat::Yes);
        {
            let m = &mut sim.hal_mut().motor;
            if speed_step(Key::W) {
                kmh += 1;
                m.set_speed_kmh(kmh);
            }
            if speed_step(Key::S) {
                kmh = kmh.saturating_sub(1);
                m.set_speed_kmh(kmh);
            }
            if speed_step(Key::E) {
                m.current_x2 = m.current_x2.saturating_add(2);
            }
            if speed_step(Key::D) {
                m.current_x2 = m.current_x2.saturating_sub(2);
            }
            if speed_step(Key::R) {
                m.soc = (m.soc + 5).min(100);
            }
            if speed_step(Key::F) {
                m.soc = m.soc.saturating_sub(5);
            }
        }
        if window.is_key_pressed(Key::L, KeyRepeat::No) {
            let m = &mut sim.hal_mut().motor;
            m.online = !m.online;
            eprintln!("motor link {}", if m.online { "up" } else { "down" });
        }
        if window.is_key_pressed(Key::F12, KeyRepeat::No) {
            let path = std::path::PathBuf::from(format!("swet102-{shot:03}.png"));
            match swet_sim::write_png(&sim.screen(), &path, SCALE as u32) {
                Ok(()) => eprintln!("saved {}", path.display()),
                Err(e) => eprintln!("screenshot failed: {e}"),
            }
            shot += 1;
        }

        // Run the firmware on wall-clock time, 20 ms per tick.
        let target = start.elapsed().as_millis() as u32;
        while sim.now_ms() + config::TICK_MS <= target {
            sim.tick();
        }

        let m = &sim.hal().motor;
        let title = format!(
            "swet102 · motor {} · {kmh} km/h ({} rpm) · {} A · {} % · PAS {}",
            if m.online { "up" } else { "DOWN" },
            m.rpm,
            m.current_x2 / 2,
            m.soc,
            sim.app().state().pas,
        );
        window.set_title(&title);
        render(&sim.screen(), &mut buf, w);
        if let Err(e) = window.update_with_buffer(&buf, w, h) {
            eprintln!("swet-emu: {e}");
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn render(frame: &swet_heart::Frame, buf: &mut [u32], stride: usize) {
    for y in 0..H as usize {
        for x in 0..W as usize {
            let on = frame.get(x as i32, y as i32);
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    let edge = dx == SCALE - 1 || dy == SCALE - 1;
                    let px = if edge {
                        GAP
                    } else if on {
                        LIT
                    } else {
                        DARK
                    };
                    buf[(y * SCALE + dy) * stride + x * SCALE + dx] = px;
                }
            }
        }
    }
}
