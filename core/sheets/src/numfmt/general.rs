//! excel's "General" format as a cell shows it: as many digits as fit in
//! about 11 characters, rounding the decimals, and scientific notation
//! (1.23457E+11) when even the whole part does not fit. checked against
//! excel's display of a default-width column.

use crate::calc::value::general_text;

/// the characters "General" may use in a cell of the default width
pub const GENERAL_WIDTH: usize = 11;

pub fn general(n: f64, decimal: char) -> String {
    let text = general_within(n, GENERAL_WIDTH);
    if decimal == '.' {
        text
    } else {
        text.replace('.', &decimal.to_string())
    }
}

/// the shortest faithful form of `n` in at most `width` characters
pub fn general_within(n: f64, width: usize) -> String {
    if n == 0.0 {
        return "0".into();
    }
    let full = general_text(n);
    if full.len() <= width && !full.contains('E') {
        return full;
    }
    let scientific = scientific_within(n, width);
    // excel keeps whichever form shows more significant digits, fixed
    // notation on a tie: 0.0000123456789 shows as 1.23457E-05
    match fixed_within(n, width) {
        Some(fixed) if significant_digits(&fixed) >= significant_digits(&scientific) => fixed,
        _ => scientific,
    }
}

/// fixed notation with as many decimals as fit, when the whole part fits
fn fixed_within(n: f64, width: usize) -> Option<String> {
    let sign = usize::from(n < 0.0);
    let exponent = n.abs().log10().floor() as i32;
    if exponent > width as i32 - 1 - sign as i32 {
        return None;
    }
    let whole_digits = if exponent >= 0 {
        exponent as usize + 1
    } else {
        1
    };
    let room = width.saturating_sub(sign + whole_digits + 1);
    (0..=room)
        .rev()
        .map(|decimals| trim_zeros(format!("{:.*}", decimals, round_half_away(n, decimals))))
        .find(|text| text.len() <= width && text.trim_start_matches('-') != "0")
}

/// digits that carry information: no sign, leading zeros or exponent
fn significant_digits(text: &str) -> usize {
    let mantissa = text.split('E').next().unwrap_or(text);
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.trim_start_matches('0').len()
}

fn round_half_away(n: f64, decimals: usize) -> f64 {
    crate::calc::functions::math::round_digits(
        n,
        decimals as f64,
        crate::calc::functions::math::Rounding::Nearest,
    )
}

fn trim_zeros(text: String) -> String {
    if !text.contains('.') {
        return text;
    }
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// "1.23457E+11": as many mantissa decimals as the width allows
fn scientific_within(n: f64, width: usize) -> String {
    let sign = if n < 0.0 { "-" } else { "" };
    for decimals in (0..=5usize).rev() {
        let text = format!("{:.*e}", decimals, n.abs());
        let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
        let exponent: i32 = exponent.parse().unwrap_or(0);
        let mantissa = trim_zeros(mantissa.to_string());
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        let result = format!("{sign}{mantissa}E{exponent_sign}{:02}", exponent.abs());
        if result.len() <= width || decimals == 0 {
            return result;
        }
    }
    general_text(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_fits_eleven_characters() {
        assert_eq!(general_within(1.0 / 3.0, 11), "0.333333333");
        assert_eq!(general_within(123_456_789.123, 11), "123456789.1");
        assert_eq!(general_within(12_345_678_901.0, 11), "12345678901");
        assert_eq!(general_within(123_456_789_012.0, 11), "1.23457E+11");
        assert_eq!(general_within(-1.0 / 3.0, 11), "-0.33333333");
        assert_eq!(general_within(0.000_012_345_678_9, 11), "1.23457E-05");
        assert_eq!(general_within(42.0, 11), "42");
        assert_eq!(general_within(0.1 + 0.2, 11), "0.3");
    }
}
