//! The hardware boundary. `swet-fw` implements it over FFI to the C platform
//! layer; `swet-sim` implements it in plain Rust for the emulator and tests.

use crate::gfx::Frame;

/// Size of the persistent record (TECH_DESIGN §8.1).
pub const STORE_LEN: usize = 48;

/// Pressed buttons, polarity already handled by the platform.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Buttons(pub u8);

impl Buttons {
    pub const LEFT: u8 = 1;
    pub const RIGHT: u8 = 2;
    pub const M: u8 = 4;
    pub const PWR: u8 = 8;

    pub const fn has(self, b: u8) -> bool {
        self.0 & b != 0
    }
}

/// BLE link state as bit flags.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct BleState(pub u8);

impl BleState {
    pub const CONNECTED: u8 = 1;
    pub const TELEMETRY_SUB: u8 = 2;
    pub const COMMAND_SUB: u8 = 4;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum BleChannel {
    Telemetry = 0,
    Command = 1,
    Trips = 2,
}

/// Platform measurements for diagnostics screens. Platforms return 0 for
/// anything they don't measure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Diag {
    /// Average `tick()` duration, µs.
    TickAvgUs = 0,
    /// Longest `tick()` duration since boot, µs.
    TickMaxUs = 1,
    /// 20 ms ticks that were caught up late.
    MissedTicks = 2,
    /// Stack bytes never touched since boot.
    StackFreeBytes = 3,
    /// Physical RAM size from FICR, KB.
    RamKb = 4,
    /// Lowest app RAM start the SoftDevice accepts (address).
    SdRamBase = 5,
    /// UART line errors since boot.
    UartErrors = 6,
    /// Average display flush over the last 16 flushes, µs.
    FlushAvgUs = 7,
    /// Longest display flush since boot, µs.
    FlushMaxUs = 8,
    /// Flash writes that failed since boot.
    StoreErrors = 9,
}

pub trait Hal {
    // display: 64 rows × 16 bytes, see gfx::Frame
    fn display_flush(&mut self, frame: &Frame);
    fn display_contrast(&mut self, level: u8);
    /// SH1107 orientation: bit0 = segment remap (A0/A1), bit1 = COM scan (C0/C8).
    fn display_orient(&mut self, mode: u8);

    fn buttons(&mut self) -> Buttons;

    // motor UART, 1200 8N1
    fn uart_write(&mut self, bytes: &[u8]);
    fn uart_read(&mut self) -> Option<u8>;

    // persistent storage: one small blob, async save
    fn store_load(&mut self, buf: &mut [u8; STORE_LEN]) -> bool;
    fn store_save(&mut self, buf: &[u8; STORE_LEN]);
    fn store_busy(&self) -> bool;

    // BLE
    fn ble_state(&self) -> BleState;
    fn ble_notify(&mut self, ch: BleChannel, data: &[u8]);
    fn ble_address(&self) -> [u8; 6];

    // system
    fn power_off(&mut self);
    fn reboot_to_dfu(&mut self);
    fn diag(&self, d: Diag) -> u32;
}
