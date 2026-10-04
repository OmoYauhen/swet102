//! Every product constant lives here (PRODUCT.md §2 "Compile-time parameters").

/// Firmware version shown on screen and over BLE.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Main loop tick.
pub const TICK_MS: u32 = 20;

// --- buttons (PRODUCT §4) ---
/// A raw change must be seen this many ticks in a row (2 × 20 ms).
pub const DEBOUNCE_SAMPLES: u8 = 2;
/// Double-click window.
pub const DBL_MS: u32 = 350;
/// Hold threshold.
pub const HOLD_MS: u32 = 1000;

// --- bike ---
/// Wheel circumference.
pub const WHEEL_MM: u32 = 2165;
/// Nominal pack voltage for watts = amps × volts (stock firmware reports no voltage).
pub const PACK_V: u32 = 52;
pub const PAS_MAX: u8 = 9;
/// City limit; the sport PIN stores `SPORT_LIMIT_KMH` (PRODUCT §6).
pub const CITY_LIMIT_KMH: u8 = 25;
pub const SPORT_LIMIT_KMH: u8 = 99;

// --- motor bus (TECH_DESIGN §7) ---
/// One request per slot.
pub const MOTOR_SLOT_MS: u32 = 100;
/// No valid reply for this long = link lost.
pub const MOTOR_LINK_TIMEOUT_MS: u32 = 2000;
/// WRITE_SPEED_LIM unit: wheel RPM (true) or Swang Stodva's km/h × 10 (false).
/// Unverified on stock firmware (TECH_DESIGN §16).
pub const SPEED_LIMIT_AS_RPM: bool = true;

// --- display ---
/// SH1107 orientation used at boot: bit0 = segment remap, bit1 = COM scan.
/// Unverified on hardware (TECH_DESIGN §16); the diagnostics screen can cycle it.
pub const DISPLAY_ORIENT: u8 = 0;
