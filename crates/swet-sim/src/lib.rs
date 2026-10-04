//! Swet102 simulator: the real `swet-heart` App on a virtual clock, with a
//! plain-Rust Hal, a fake motor and in-memory storage. No FFI and no globals,
//! so tests can run many `Sim`s in parallel.

pub mod motor;

use std::path::{Path, PathBuf};

use swet_heart::gfx::{H, W};
use swet_heart::{App, BleChannel, BleState, Buttons, Diag, Frame, Hal, STORE_LEN, config};

pub use motor::FakeMotor;

/// How long a simulated flash write takes (FDS update incl. a page erase).
pub const STORE_WRITE_US: u64 = 40_000;

#[derive(Default)]
pub struct SimHal {
    pub now_us: u64,
    pub buttons: u8,
    pub motor: FakeMotor,
    pub uart_log: Vec<u8>,
    pub flushes: u32,
    pub last_flush: Option<Frame>,
    pub contrast: u8,
    /// What is in "flash".
    pub store: Option<[u8; STORE_LEN]>,
    /// A write in progress and when it lands (FDS is asynchronous; a write
    /// still in flight when power is cut is lost).
    pub store_inflight: Option<([u8; STORE_LEN], u64)>,
    pub store_writes: u32,
    pub ble: BleState,
    pub ble_notifies: Vec<(BleChannel, Vec<u8>)>,
    pub powered_off: bool,
    pub dfu_requested: bool,
}

impl Hal for SimHal {
    fn display_flush(&mut self, frame: &Frame) {
        self.flushes += 1;
        self.last_flush = Some(frame.clone());
    }
    fn display_contrast(&mut self, level: u8) {
        self.contrast = level;
    }
    fn buttons(&mut self) -> Buttons {
        Buttons(self.buttons)
    }
    fn uart_write(&mut self, bytes: &[u8]) {
        self.uart_log.extend_from_slice(bytes);
        self.motor.receive(self.now_us, bytes);
    }
    fn uart_read(&mut self) -> Option<u8> {
        self.motor.pop_arrived(self.now_us)
    }
    fn store_load(&mut self, buf: &mut [u8; STORE_LEN]) -> bool {
        match self.store {
            Some(s) => {
                *buf = s;
                true
            }
            None => false,
        }
    }
    fn store_save(&mut self, buf: &[u8; STORE_LEN]) {
        self.store_inflight = Some((*buf, self.now_us + STORE_WRITE_US));
    }
    fn store_busy(&self) -> bool {
        self.store_inflight.is_some()
    }
    fn ble_state(&self) -> BleState {
        self.ble
    }
    fn ble_notify(&mut self, ch: BleChannel, data: &[u8]) {
        self.ble_notifies.push((ch, data.to_vec()));
    }
    fn ble_address(&self) -> [u8; 6] {
        [0xC0, 0xFF, 0xEE, 0x10, 0x21, 0x02]
    }
    fn power_off(&mut self) {
        self.powered_off = true;
        self.store_inflight = None; // cut mid-write: lost
    }
    fn reboot_to_dfu(&mut self) {
        self.dfu_requested = true;
    }
    fn diag(&self, d: Diag) -> u32 {
        match d {
            Diag::RamKb => 32,
            Diag::SdRamBase => 0x2000_2C00,
            _ => 0,
        }
    }
}

pub struct Sim {
    app: App<SimHal>,
    now_ms: u32,
}

impl Default for Sim {
    fn default() -> Self {
        Self::new()
    }
}

impl Sim {
    /// A powered-on display with empty flash: `init()` done, no ticks run yet.
    pub fn new() -> Self {
        Self::with_store(None)
    }

    /// A powered-on display whose flash already holds `store`.
    pub fn with_store(store: Option<[u8; STORE_LEN]>) -> Self {
        let hal = SimHal {
            store,
            ..SimHal::default()
        };
        let mut app = App::new(hal);
        app.init(0);
        Self { app, now_ms: 0 }
    }

    /// Power-cycle: a fresh display that boots from what reached flash.
    pub fn reboot(&self) -> Self {
        Self::with_store(self.hal().store)
    }

    pub fn now_ms(&self) -> u32 {
        self.now_ms
    }

    /// Advance one 20 ms tick.
    pub fn tick(&mut self) {
        if self.hal().powered_off {
            return; // nothing runs without power
        }
        self.now_ms += config::TICK_MS;
        let hal = self.app.hal_mut();
        hal.now_us = u64::from(self.now_ms) * 1000;
        if let Some((buf, at)) = hal.store_inflight
            && hal.now_us >= at
        {
            hal.store = Some(buf);
            hal.store_inflight = None;
            hal.store_writes += 1;
        }
        self.app.tick(self.now_ms);
    }

    pub fn run_ms(&mut self, ms: u32) {
        let end = self.now_ms + ms;
        while self.now_ms < end && !self.hal().powered_off {
            self.tick();
        }
    }

    pub fn press(&mut self, b: u8) {
        self.app.hal_mut().buttons |= b;
    }

    pub fn release(&mut self, b: u8) {
        self.app.hal_mut().buttons &= !b;
    }

    /// Press, hold for `ms`, release, and run until the release is debounced.
    pub fn hold(&mut self, b: u8, ms: u32) {
        self.press(b);
        self.run_ms(ms);
        self.release(b);
        self.run_ms(
            u32::from(swet_heart::config::DEBOUNCE_SAMPLES + 1) * swet_heart::config::TICK_MS,
        );
    }

    pub fn click(&mut self, b: u8) {
        self.hold(b, 60);
    }

    /// Run until the fake motor link is up (first reply decoded) plus one more tick.
    pub fn boot_to_ride(&mut self) -> &mut Self {
        for _ in 0..100 {
            self.tick();
            if self.app.motor().link_up() {
                break;
            }
        }
        self.run_ms(1000); // let every value arrive once
        self
    }

    pub fn motor(&mut self) -> &mut FakeMotor {
        &mut self.app.hal_mut().motor
    }

    pub fn double_click(&mut self, b: u8) {
        self.hold(b, 60);
        self.run_ms(100);
        self.hold(b, 60);
    }

    pub fn app_mut(&mut self) -> &mut App<SimHal> {
        &mut self.app
    }

    pub fn app(&self) -> &App<SimHal> {
        &self.app
    }

    pub fn hal(&self) -> &SimHal {
        self.app.hal()
    }

    pub fn hal_mut(&mut self) -> &mut SimHal {
        self.app.hal_mut()
    }

    /// The frame most recently sent to the display.
    pub fn screen(&self) -> Frame {
        self.hal().last_flush.clone().unwrap_or_default()
    }

    /// Compare the current screen with `golden/<name>.png`. With
    /// `UPDATE_GOLDEN=1` the golden file is (re)written instead.
    pub fn assert_screen(&self, name: &str) {
        let path = golden_dir().join(format!("{name}.png"));
        let frame = self.screen();
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::create_dir_all(golden_dir()).expect("golden dir");
            write_png(&frame, &path, 1).expect("write golden");
            return;
        }
        let golden = read_png(&path).unwrap_or_else(|e| {
            panic!(
                "missing golden {}: {e}; run with UPDATE_GOLDEN=1",
                path.display()
            )
        });
        if golden != frame {
            let actual = path.with_extension("actual.png");
            write_png(&frame, &actual, 1).expect("write actual");
            panic!(
                "screen differs from {}; see {}",
                path.display(),
                actual.display()
            );
        }
    }
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("golden")
}

/// Save a frame as a grayscale PNG, `scale`× magnified (lit = white).
pub fn write_png(frame: &Frame, path: &Path, scale: u32) -> std::io::Result<()> {
    let (w, h) = (W as u32 * scale, H as u32 * scale);
    let mut data = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let on = frame.get((x / scale) as i32, (y / scale) as i32);
            data.push(if on { 0xFF } else { 0x00 });
        }
    }
    let file = std::fs::File::create(path)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Grayscale);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().map_err(std::io::Error::other)?;
    writer
        .write_image_data(&data)
        .map_err(std::io::Error::other)
}

/// Load a 128×64 grayscale PNG written by [`write_png`] with scale 1.
pub fn read_png(path: &Path) -> std::io::Result<Frame> {
    let file = std::io::BufReader::new(std::fs::File::open(path)?);
    let mut reader = png::Decoder::new(file)
        .read_info()
        .map_err(std::io::Error::other)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| std::io::Error::other("png too large"))?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(std::io::Error::other)?;
    if info.width != W as u32
        || info.height != H as u32
        || info.color_type != png::ColorType::Grayscale
    {
        return Err(std::io::Error::other("golden must be 128x64 grayscale"));
    }
    let mut f = Frame::new();
    for y in 0..H {
        for x in 0..W {
            if buf[(y * W + x) as usize] >= 0x80 {
                f.pixel(x, y, swet_heart::gfx::Mode::Set);
            }
        }
    }
    Ok(f)
}
