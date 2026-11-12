//! fraction formats: "# ?/?" (whole number and up to one-digit
//! denominators), "?/??" (improper, up to two digits), "# ?/8" (eighths).
//! the closest fraction is found with a farey search, which is how the
//! "best denominator up to n" excel shows can be found exactly.

use super::parse::Token;

pub fn render(value: f64, tokens: &[Token]) -> Option<String> {
    let slash = tokens.iter().position(|t| *t == Token::Slash)?;
    let (before, after) = (&tokens[..slash], &tokens[slash + 1..]);
    // the numerator is the run of placeholders right before the slash
    let numerator_start = before
        .iter()
        .rposition(|t| !matches!(t, Token::Digit(_)))
        .map(|p| p + 1)
        .unwrap_or(0);
    let numerator_places = before.len() - numerator_start;
    let whole_places = before[..numerator_start]
        .iter()
        .filter(|t| matches!(t, Token::Digit(_)))
        .count();
    let (denominator_places, fixed) = denominator(after);
    let magnitude = value.abs();
    let (whole, fraction) = (magnitude.trunc(), magnitude.fract());
    let (mut numerator, denominator) = match fixed {
        Some(d) => ((fraction * d as f64).round() as u64, d),
        None => closest_fraction(fraction, 10u64.pow(denominator_places.max(1) as u32) - 1),
    };
    let mut whole = whole;
    if numerator == denominator && whole_places > 0 {
        whole += 1.0;
        numerator = 0;
    }
    let numerator = if whole_places == 0 {
        numerator + (whole as u64) * denominator
    } else {
        numerator
    };
    let mut out = String::new();
    if value < 0.0 {
        out.push('-');
    }
    if whole_places > 0 && (whole > 0.0 || numerator == 0) {
        out.push_str(&format!("{whole}"));
    }
    if numerator == 0 && whole_places > 0 {
        // excel pads the missing fraction with spaces so columns line up
        out.push_str(&" ".repeat(numerator_places + 1 + denominator_places.max(1) + 1));
        return Some(out);
    }
    if whole_places > 0 {
        out.push(' ');
    }
    out.push_str(&format!(
        "{numerator:>width$}/{denominator:<dwidth$}",
        width = numerator_places,
        dwidth = denominator_places
    ));
    Some(out)
}

/// the denominator's placeholder count, or its fixed value ("/8")
fn denominator(tokens: &[Token]) -> (usize, Option<u64>) {
    let mut places = 0;
    let mut fixed = String::new();
    for token in tokens {
        match token {
            Token::Digit(c) if c.is_ascii_digit() && *c != '0' => fixed.push(*c),
            Token::Digit(_) => places += 1,
            Token::Literal(text) if text.chars().all(|c| c.is_ascii_digit()) => {
                fixed.push_str(text)
            }
            _ => break,
        }
    }
    let fixed_value = fixed.parse().ok().filter(|d| *d > 0);
    (
        if fixed_value.is_some() {
            fixed.len()
        } else {
            places
        },
        fixed_value,
    )
}

/// the fraction excel shows for `x` (0 <= x < 1) with a denominator up
/// to `limit`: the last continued fraction convergent that fits. it is
/// not always the closest fraction (excel shows 3.14159 as 3 1/7 with
/// two-digit denominators where 14/99 is closer; libreoffice shows 14/99)
pub fn closest_fraction(x: f64, limit: u64) -> (u64, u64) {
    let (mut previous_n, mut previous_d) = (1u64, 0u64);
    let (mut n, mut d) = (0u64, 1u64);
    let mut rest = x;
    for _ in 0..64 {
        if rest.fract().abs() < 1e-12 && d > 0 && (n as f64 / d as f64 - x).abs() < 1e-12 {
            break;
        }
        let inverse = 1.0 / (rest - rest.floor());
        if !inverse.is_finite() {
            break;
        }
        let term = inverse.floor() as u64;
        let next_n = term * n + previous_n;
        let next_d = term * d + previous_d;
        if next_d > limit {
            break;
        }
        previous_n = n;
        previous_d = d;
        n = next_n;
        d = next_d;
        rest = inverse;
    }
    // the convergents of a fraction start from 0/1; a value closer to 1
    // than the last convergent rounds up to 1/1
    if (1.0 - x) < (x - n as f64 / d as f64).abs() {
        return (1, 1);
    }
    (n, d)
}

/// the closest fraction of all (a farey search), kept for comparison
#[cfg(test)]
fn best_fraction(x: f64, limit: u64) -> (u64, u64) {
    let (mut low_n, mut low_d, mut high_n, mut high_d) = (0u64, 1u64, 1u64, 1u64);
    loop {
        let mediant_n = low_n + high_n;
        let mediant_d = low_d + high_d;
        if mediant_d > limit {
            break;
        }
        if (mediant_n as f64) / (mediant_d as f64) <= x {
            low_n = mediant_n;
            low_d = mediant_d;
        } else {
            high_n = mediant_n;
            high_d = mediant_d;
        }
    }
    let low_error = (x - low_n as f64 / low_d as f64).abs();
    let high_error = (high_n as f64 / high_d as f64 - x).abs();
    if low_error <= high_error {
        (low_n, low_d)
    } else {
        (high_n, high_d)
    }
}

#[cfg(test)]
mod tests {
    use super::super::render::{format_number, Locale};
    use super::*;
    use crate::model::DateSystem;

    fn show(n: f64, code: &str) -> String {
        format_number(n, code, &Locale::default(), DateSystem::Excel1900).text
    }

    #[test]
    fn fractions_like_excel() {
        assert_eq!(show(1.5, "# ?/?"), "1 1/2");
        assert_eq!(show(0.75, "# ?/?"), " 3/4");
        assert_eq!(show(3.14158, "# ??/??"), "3  1/7 ");
        assert_eq!(best_fraction(0.14159, 99), (14, 99));
        assert_eq!(show(1.5, "?/?"), "3/2");
        assert_eq!(show(0.3, "# ?/8"), " 2/8");
        assert_eq!(show(2.0, "# ?/?"), "2    ");
        assert_eq!(closest_fraction(1.0 / 3.0, 9), (1, 3));
    }
}
