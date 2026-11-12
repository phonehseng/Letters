//! numbers as the standard formats read and write them: reading typed
//! numbers in a field's separator style, rounding, and grouping digits.

use super::eval::parse_number;
use super::format::SeparatorStyle;

/// a number typed in the field's style, the plain style, or with the
/// currency and spaces around it. parentheses mean negative.
pub(super) fn read_number(text: &str, separator: SeparatorStyle, currency: &str) -> Option<f64> {
    let mut text = text.trim().to_owned();
    let currency = currency.trim();
    if !currency.is_empty() {
        text = text.replace(currency, "");
    }
    let mut negative = false;
    let mut cleaned = text.trim().to_owned();
    if cleaned.starts_with('(') && cleaned.ends_with(')') {
        negative = true;
        cleaned = cleaned[1..cleaned.len() - 1].trim().to_owned();
    }
    cleaned.retain(|c| !c.is_whitespace() && c != '\'' && c != '\u{a0}');
    if let Some(rest) = cleaned.strip_prefix('-') {
        negative = !negative;
        cleaned = rest.to_owned();
    }
    let comma_decimal = separator.decimal_mark() == ',';
    let plain = if comma_decimal {
        // 1.234,56: dots group, the comma starts the decimals
        if cleaned.contains(',') {
            cleaned.replace('.', "").replace(',', ".")
        } else if cleaned.matches('.').count() > 1 {
            cleaned.replace('.', "")
        } else {
            cleaned
        }
    } else {
        cleaned.replace(',', "")
    };
    if plain.starts_with('-') || plain.starts_with('+') && plain.len() == 1 {
        return None;
    }
    let value = parse_number(&plain)?;
    Some(if negative { -value } else { value })
}

pub(super) fn example(separator: SeparatorStyle) -> &'static str {
    match separator {
        SeparatorStyle::CommaDot => "1,234.50",
        SeparatorStyle::Dot => "1234.50",
        SeparatorStyle::DotComma => "1.234,50",
        SeparatorStyle::Comma => "1234,50",
        SeparatorStyle::ApostropheDot => "1'234.50",
    }
}

/// rounds half away from zero, as people expect money to round.
pub(super) fn round_to(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals.min(10) as i32);
    let scaled = value * factor;
    // a tiny nudge so 1.005 (stored as 1.00499...) rounds the way it reads
    let nudged = scaled + scaled.signum() * 1e-9;
    nudged.round() / factor
}

/// digits with grouping and the decimal mark of the style.
pub(super) fn group(value: f64, decimals: u32, separator: SeparatorStyle) -> String {
    let fixed = format!("{:.*}", decimals.min(10) as usize, value);
    let (whole, fraction) = match fixed.split_once('.') {
        Some((whole, fraction)) => (whole.to_owned(), Some(fraction.to_owned())),
        None => (fixed, None),
    };
    let mut grouped = String::new();
    let count = whole.len();
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (count - index) % 3 == 0 {
            if let Some(mark) = separator.grouping() {
                grouped.push(mark);
            }
        }
        grouped.push(digit);
    }
    if let Some(fraction) = fraction {
        grouped.push(separator.decimal_mark());
        grouped.push_str(&fraction);
    }
    grouped
}
