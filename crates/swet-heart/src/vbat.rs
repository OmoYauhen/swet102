//! Pack voltage from the display's battery sense (AIN2 behind the 300k/16k
//! divider, TECH_DESIGN §16 #11, confirmed on hardware 2026-10-17).

use crate::config;

/// Millivolts from raw ADC counts: 1024 counts = 3.6 V on the pin, times the
/// divider ratio.
pub fn mv(raw: u32) -> u32 {
    let (top, bottom) = config::VBAT_DIVIDER_K;
    raw * 3600 * (top + bottom) / (1024 * bottom)
}

/// The pack voltage in millivolts, or `None` before the first sample and for
/// readings no 14S pack gives (a floating pin, a display on a bench supply).
pub fn pack_mv(raw: u32) -> Option<u32> {
    Some(mv(raw)).filter(|v| config::VBAT_VALID_MV.contains(v))
}

/// Volts for watts = amps × volts: the measured pack, else the nominal one.
pub fn volts_mv(pack_mv: Option<u32>) -> u32 {
    pack_mv.unwrap_or(config::PACK_V * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn through_the_divider() {
        assert_eq!(mv(0), 0);
        // full 14S pack, 58.8 V: 2.98 V on the pin
        assert_eq!(mv(847) / 100, 588);
        assert_eq!(mv(1023), 71_030);
    }

    #[test]
    fn implausible_readings_are_dropped() {
        assert_eq!(pack_mv(0), None, "no sample yet");
        assert_eq!(pack_mv(300), None, "25 V: not a 14S pack");
        assert_eq!(pack_mv(1023), None, "71 V: floating pin");
        assert_eq!(pack_mv(847).map(|v| v / 100), Some(588));
        assert_eq!(volts_mv(None), 52_000);
        assert_eq!(volts_mv(Some(54_300)), 54_300);
    }
}
