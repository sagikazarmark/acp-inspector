//! Stock serde_json's numeric policy, independent of Cargo feature unification.
//!
//! Adapted from serde_json 1.0.151 src/de.rs (MIT OR Apache-2.0;
//! copyright its contributors): parse_integer, parse_decimal, parse_exponent,
//! parse_long_integer, parse_decimal_overflow and f64_from_parts. This is not
//! a JSON validator: callers supply an already validated Number token.
//! Keep the u64 truncation and order of floating-point operations unchanged.

/// Read a validated JSON numeric token using serde_json's non-float_roundtrip
/// algorithm. None means stock serde_json would reject an infinite result.
pub(super) fn float(token: &str) -> Option<f64> {
    let negative = token.starts_with('-');
    let bytes = token.as_bytes();
    let mut at = usize::from(negative);
    let mut significand = 0_u64;
    let mut exponent = 0_i32;
    let mut truncated = false;
    while let Some(&digit @ b'0'..=b'9') = bytes.get(at) {
        if !truncated {
            if let Some(next) = append(significand, digit) {
                significand = next;
            } else {
                truncated = true;
            }
        }
        if truncated {
            exponent += 1;
        }
        at += 1;
    }
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        // Even after integer overflow, stock attempts to append fractional
        // digits to the retained significand until the first overflow.
        while let Some(&digit @ b'0'..=b'9') = bytes.get(at) {
            match append(significand, digit) {
                Some(next) => {
                    significand = next;
                    exponent -= 1;
                    at += 1;
                }
                None => break,
            }
        }
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
    }
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        let negative_exp = bytes.get(at) == Some(&b'-');
        if matches!(bytes.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let mut explicit = 0_i32;
        for &digit in &bytes[at..] {
            let Some(next) = explicit
                .checked_mul(10)
                .and_then(|n| n.checked_add(i32::from(digit - b'0')))
            else {
                return if significand != 0 && !negative_exp {
                    None
                } else {
                    Some(if negative { -0.0 } else { 0.0 })
                };
            };
            explicit = next;
        }
        exponent = if negative_exp {
            exponent.saturating_sub(explicit)
        } else {
            exponent.saturating_add(explicit)
        };
    }
    let mut value = significand as f64;
    loop {
        if exponent.unsigned_abs() <= 308 {
            // Rust reads the power itself with correct rounding, the same bits
            // as serde_json's literal POW10 table. Do not use powi: repeated
            // multiplication would round differently for some powers.
            static POWERS: std::sync::OnceLock<[f64; 309]> = std::sync::OnceLock::new();
            let powers =
                POWERS.get_or_init(|| std::array::from_fn(|n| format!("1e{n}").parse().unwrap()));
            let power = powers[exponent.unsigned_abs() as usize];
            if exponent >= 0 {
                value *= power;
            } else {
                value /= power;
            }
            break;
        }
        if value == 0.0 {
            break;
        }
        if exponent >= 0 {
            return None;
        }
        value /= 1e308;
        exponent += 308;
    }
    value
        .is_finite()
        .then_some(if negative { -value } else { value })
}

fn append(significand: u64, digit: u8) -> Option<u64> {
    significand
        .checked_mul(10)?
        .checked_add(u64::from(digit - b'0'))
}
