//! Firmware glue between the C platform layer (platform/nrf51) and swet-heart.
//!
//! All `unsafe` in the project lives here: the FFI declarations, the single
//! static `App`, and the panic handler. Only fixed-width integers, bool and
//! pointer + length cross the boundary (TECH_DESIGN §3.2).
#![no_std]

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use swet_heart::{App, BleChannel, BleState, Buttons, Diag, Frame, Hal, STORE_LEN};

mod ffi {
    unsafe extern "C" {
        pub fn hal_display_flush(fb: *const u8);
        pub fn hal_display_contrast(level: u8);
        pub fn hal_buttons() -> u8;
        pub fn hal_uart_write(buf: *const u8, len: u8);
        pub fn hal_uart_read() -> i16;
        pub fn hal_store_load(buf: *mut u8, len: u16) -> bool;
        pub fn hal_store_save(buf: *const u8, len: u16);
        pub fn hal_store_busy() -> bool;
        pub fn hal_ble_state() -> u8;
        pub fn hal_ble_notify(ch: u8, buf: *const u8, len: u8);
        pub fn hal_ble_address(out: *mut u8);
        pub fn hal_power_off();
        pub fn hal_reboot_to_dfu();
        pub fn hal_diag(id: u8) -> u32;
        pub fn hal_panic() -> !;
    }
}

struct FwHal;

impl Hal for FwHal {
    fn display_flush(&mut self, frame: &Frame) {
        // [[u8; 16]; 64] is 1024 contiguous bytes.
        unsafe { ffi::hal_display_flush(frame.0.as_ptr().cast()) }
    }
    fn display_contrast(&mut self, level: u8) {
        unsafe { ffi::hal_display_contrast(level) }
    }
    fn buttons(&mut self) -> Buttons {
        Buttons(unsafe { ffi::hal_buttons() })
    }
    fn uart_write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(u8::MAX as usize) {
            unsafe { ffi::hal_uart_write(chunk.as_ptr(), chunk.len() as u8) }
        }
    }
    fn uart_read(&mut self) -> Option<u8> {
        let v = unsafe { ffi::hal_uart_read() };
        u8::try_from(v).ok()
    }
    fn store_load(&mut self, buf: &mut [u8; STORE_LEN]) -> bool {
        unsafe { ffi::hal_store_load(buf.as_mut_ptr(), STORE_LEN as u16) }
    }
    fn store_save(&mut self, buf: &[u8; STORE_LEN]) {
        unsafe { ffi::hal_store_save(buf.as_ptr(), STORE_LEN as u16) }
    }
    fn store_busy(&self) -> bool {
        unsafe { ffi::hal_store_busy() }
    }
    fn ble_state(&self) -> BleState {
        BleState(unsafe { ffi::hal_ble_state() })
    }
    fn ble_notify(&mut self, ch: BleChannel, data: &[u8]) {
        let len = data.len().min(20) as u8; // default ATT payload
        unsafe { ffi::hal_ble_notify(ch as u8, data.as_ptr(), len) }
    }
    fn ble_address(&self) -> [u8; 6] {
        let mut a = [0u8; 6];
        unsafe { ffi::hal_ble_address(a.as_mut_ptr()) };
        a
    }
    fn power_off(&mut self) {
        unsafe { ffi::hal_power_off() }
    }
    fn reboot_to_dfu(&mut self) {
        unsafe { ffi::hal_reboot_to_dfu() }
    }
    fn diag(&self, d: Diag) -> u32 {
        unsafe { ffi::hal_diag(d as u8) }
    }
}

/// The one application instance. Touched only from the exported functions
/// below, which the C main loop calls one at a time.
struct AppCell(UnsafeCell<App<FwHal>>);
// SAFETY: single-threaded access from the main loop, checked by `ENTERED`.
unsafe impl Sync for AppCell {}

static APP: AppCell = AppCell(UnsafeCell::new(App::new(FwHal)));
static ENTERED: AtomicBool = AtomicBool::new(false);

fn with_app(f: impl FnOnce(&mut App<FwHal>)) {
    // Re-entrancy would mean an ISR called into the core: a platform bug.
    if ENTERED.load(Ordering::Relaxed) {
        unsafe { ffi::hal_panic() }
    }
    ENTERED.store(true, Ordering::Relaxed);
    // SAFETY: guarded above; no other reference to APP exists.
    f(unsafe { &mut *APP.0.get() });
    ENTERED.store(false, Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub extern "C" fn swet_init(now_ms: u32) {
    with_app(|a| a.init(now_ms));
}

#[unsafe(no_mangle)]
pub extern "C" fn swet_tick(now_ms: u32) {
    with_app(|a| a.tick(now_ms));
}

/// # Safety
/// `data` must point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn swet_ble_control(data: *const u8, len: u8) {
    let bytes = unsafe { core::slice::from_raw_parts(data, usize::from(len)) };
    with_app(|a| a.ble_control(bytes));
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    unsafe { ffi::hal_panic() }
}
