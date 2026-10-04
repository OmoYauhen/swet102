//! Number → ASCII without `core::fmt` (keeps the firmware image small).

/// Decimal digits of `n`, written right-aligned into `buf`. Returns the used tail.
pub fn u32_dec(n: u32, buf: &mut [u8; 10]) -> &[u8] {
    let mut n = n;
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    &buf[i..]
}

/// `n` tenths as "12.3" (always one decimal, at least "0.0").
pub fn u32_dec1(n: u32, buf: &mut [u8; 12]) -> &[u8] {
    let mut whole = [0u8; 10];
    let s = u32_dec(n / 10, &mut whole);
    let len = s.len();
    buf[..len].copy_from_slice(s);
    buf[len] = b'.';
    buf[len + 1] = b'0' + (n % 10) as u8;
    &buf[..len + 2]
}

/// `n` hundredths as "7.85" (always two decimals).
pub fn u32_dec2(n: u32, buf: &mut [u8; 13]) -> &[u8] {
    let mut whole = [0u8; 10];
    let s = u32_dec(n / 100, &mut whole);
    let len = s.len();
    buf[..len].copy_from_slice(s);
    buf[len] = b'.';
    buf[len + 1] = b'0' + (n / 10 % 10) as u8;
    buf[len + 2] = b'0' + (n % 10) as u8;
    &buf[..len + 3]
}

/// Uppercase hex of `n`, exactly `digits` long (1..=8).
pub fn u32_hex(n: u32, digits: usize, buf: &mut [u8; 8]) -> &[u8] {
    let digits = digits.clamp(1, 8);
    for (i, out) in buf.iter_mut().take(digits).enumerate() {
        let nib = ((n >> ((digits - 1 - i) * 4)) & 0xF) as u8;
        *out = if nib < 10 {
            b'0' + nib
        } else {
            b'A' + nib - 10
        };
    }
    &buf[..digits]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dec() {
        let mut b = [0; 10];
        assert_eq!(u32_dec(0, &mut b), b"0");
        assert_eq!(u32_dec(42, &mut b), b"42");
        assert_eq!(u32_dec(u32::MAX, &mut b), b"4294967295");
    }

    #[test]
    fn dec1() {
        let mut b = [0; 12];
        assert_eq!(u32_dec1(427, &mut b), b"42.7");
        assert_eq!(u32_dec1(5, &mut b), b"0.5");
        assert_eq!(u32_dec1(0, &mut b), b"0.0");
    }

    #[test]
    fn dec2() {
        let mut b = [0; 13];
        assert_eq!(u32_dec2(785, &mut b), b"7.85");
        assert_eq!(u32_dec2(5, &mut b), b"0.05");
    }

    #[test]
    fn hex() {
        let mut b = [0; 8];
        assert_eq!(u32_hex(0x2000_2C00, 8, &mut b), b"20002C00");
        assert_eq!(u32_hex(0xB1, 2, &mut b), b"B1");
    }
}
