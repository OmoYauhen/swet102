//! Build-time secrets and version (PRODUCT §2, TECH_DESIGN §12).
//!
//! `SWET_PIN_CITY` / `SWET_PIN_SPORT`: 4 digits each, different from each other.
//! When neither is set (tests, emulator, CI) the dev PINs 1111 / 2222 are used
//! and the version gets a `-dev` suffix so such a build is obvious on screen.
//! `make dfu` refuses dev PINs (see Makefile).

use std::fmt::Write as _;

fn pin(var: &str) -> Option<[u8; 4]> {
    println!("cargo:rerun-if-env-changed={var}");
    let v = std::env::var(var).ok()?;
    let digits: Vec<u8> = v.bytes().collect();
    if digits.len() != 4 || !digits.iter().all(u8::is_ascii_digit) {
        panic!("{var} must be exactly 4 digits");
    }
    Some([
        digits[0] - b'0',
        digits[1] - b'0',
        digits[2] - b'0',
        digits[3] - b'0',
    ])
}

fn main() {
    let (city, sport, dev) = match (pin("SWET_PIN_CITY"), pin("SWET_PIN_SPORT")) {
        (Some(c), Some(s)) => (c, s, false),
        (None, None) => ([1, 1, 1, 1], [2, 2, 2, 2], true),
        _ => panic!("set both SWET_PIN_CITY and SWET_PIN_SPORT, or neither (dev PINs)"),
    };
    assert!(
        city != sport,
        "SWET_PIN_CITY and SWET_PIN_SPORT must differ"
    );

    let version = format!(
        "{}{}",
        std::env::var("CARGO_PKG_VERSION").expect("cargo sets this"),
        if dev { "-dev" } else { "" }
    );
    let mut out = String::new();
    let _ = writeln!(out, "/// PIN that unlocks in city mode.");
    let _ = writeln!(out, "pub const PIN_CITY: [u8; 4] = {city:?};");
    let _ = writeln!(out, "/// PIN that unlocks in sport mode.");
    let _ = writeln!(out, "pub const PIN_SPORT: [u8; 4] = {sport:?};");
    let _ = writeln!(out, "/// Built with the public dev PINs.");
    let _ = writeln!(out, "pub const DEV_PINS: bool = {dev};");
    let _ = writeln!(out, "/// Version shown on screen and over BLE.");
    let _ = writeln!(out, "pub const VERSION: &str = {version:?};");
    let dest = std::path::Path::new(&std::env::var("OUT_DIR").expect("cargo sets this"))
        .join("build_info.rs");
    std::fs::write(dest, out).expect("write build_info.rs");
}
