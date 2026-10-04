//! Every product constant lives here (PRODUCT.md §2 "Compile-time parameters").

/// Firmware version shown on screen and over BLE.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Main loop tick.
pub const TICK_MS: u32 = 20;

/// Button hold threshold (PRODUCT §4).
pub const HOLD_MS: u32 = 1000;

/// SH1107 orientation used at boot: bit0 = segment remap, bit1 = COM scan.
/// Unverified on hardware (TECH_DESIGN §16); the M0 test pattern can cycle it.
pub const DISPLAY_ORIENT: u8 = 0;
