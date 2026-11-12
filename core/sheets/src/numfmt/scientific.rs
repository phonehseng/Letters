//! scientific formats: "0.00E+00", "##0.0E+0" (engineering, exponent a
//! multiple of the whole placeholders when they start with '#').

use crate::calc::functions::math::{round_digits, Rounding};

use super::parse::Token;
use super::render::Locale;

pub fn render(value: f64, tokens: &[Token], locale: &Locale) -> String {
    let exponent_at = tokens
        .iter()
        .position(|t| matches!(t, Token::Exponent { .. }))
        .unwrap_or(tokens.len());
    let plus = matches!(
        tokens.get(exponent_at),
        Some(Token::Exponent { plus: true })
    );
    let (mantissa_tokens, exponent_tokens) = tokens.split_at(exponent_at);
    let exponent_tokens = &exponent_tokens[1.min(exponent_tokens.len())..];
    let point = mantissa_tokens
        .iter()
        .position(|t| *t == Token::DecimalPoint);
    let digits_before: Vec<char> =
        digit_chars(&mantissa_tokens[..point.unwrap_or(mantissa_tokens.len())]);
    let decimals = point
        .map(|p| digit_chars(&mantissa_tokens[p + 1..]).len())
        .unwrap_or(0);
    let exponent_width = digit_chars(exponent_tokens).len().max(1);
    let (mantissa, exponent) = split(value.abs(), &digits_before, decimals);
    let mut out = String::new();
    if value < 0.0 {
        out.push('-');
    }
    for token in mantissa_tokens {
        if let Token::Literal(text) = token {
            if !out.ends_with(char::is_numeric) {
                out.push_str(text);
            }
        }
    }
    let mantissa_text =
        format!("{:.*}", decimals, mantissa).replace('.', &locale.decimal.to_string());
    out.push_str(&mantissa_text);
    out.push('E');
    if exponent < 0 {
        out.push('-');
    } else if plus {
        out.push('+');
    }
    out.push_str(&format!(
        "{:0width$}",
        exponent.abs(),
        width = exponent_width
    ));
    out
}

fn digit_chars(tokens: &[Token]) -> Vec<char> {
    tokens
        .iter()
        .filter_map(|t| {
            if let Token::Digit(c) = t {
                Some(*c)
            } else {
                None
            }
        })
        .collect()
}

/// the mantissa (rounded to `decimals`) and exponent
fn split(value: f64, whole_places: &[char], decimals: usize) -> (f64, i32) {
    if value == 0.0 {
        return (0.0, 0);
    }
    let places = whole_places.len().max(1) as i32;
    let engineering = places > 1 && whole_places.first() == Some(&'#');
    let magnitude = value.log10().floor() as i32;
    let mut exponent = if engineering {
        magnitude.div_euclid(places) * places
    } else {
        magnitude - (places - 1)
    };
    let mut mantissa = round_digits(
        value / 10f64.powi(exponent),
        decimals as f64,
        Rounding::Nearest,
    );
    // rounding can carry into another digit: 9.99 -> 10.0
    let limit = 10f64.powi(places);
    if mantissa >= limit {
        let step = if engineering { places } else { 1 };
        exponent += step;
        mantissa = round_digits(
            value / 10f64.powi(exponent),
            decimals as f64,
            Rounding::Nearest,
        );
    }
    (mantissa, exponent)
}

#[cfg(test)]
mod tests {
    use super::super::render::format_number;
    use super::*;
    use crate::model::DateSystem;

    fn show(n: f64, code: &str) -> String {
        format_number(n, code, &Locale::default(), DateSystem::Excel1900).text
    }

    #[test]
    fn scientific_and_engineering() {
        assert_eq!(show(12345.0, "0.00E+00"), "1.23E+04");
        assert_eq!(show(0.000123, "0.00E+00"), "1.23E-04");
        assert_eq!(show(9.999, "0.00E+00"), "1.00E+01");
        assert_eq!(show(12345.0, "##0.0E+0"), "12.3E+3");
        assert_eq!(show(-12345.0, "0.0E+00"), "-1.2E+04");
        assert_eq!(show(0.0, "0.00E+00"), "0.00E+00");
    }
}
