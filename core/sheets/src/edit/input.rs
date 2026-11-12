//! what a person types into a cell, turned into a value or formula and,
//! like excel, a number format when the typing says one ("12%" is 0.12
//! shown as a percentage, "3/1/2024" a date, "$5" currency).

use crate::calc::text_numbers::parse_number;
use crate::error::SheetError;
use crate::model::{DateSystem, ErrorValue, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct Typed {
    pub value: Value,
    /// excel syntax without "="
    pub formula: Option<String>,
    /// a format excel would apply to show the typed value as typed
    pub number_format: Option<String>,
}

pub fn read_input(input: &str, system: DateSystem) -> Result<Typed, SheetError> {
    let plain = |value| Typed {
        value,
        formula: None,
        number_format: None,
    };
    if let Some(formula) = input.strip_prefix('=').filter(|f| !f.trim().is_empty()) {
        return read_formula(formula);
    }
    // a leading apostrophe keeps the rest as text ("'007")
    if let Some(text) = input.strip_prefix('\'') {
        return Ok(plain(Value::text(text)));
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(plain(Value::Blank));
    }
    if trimmed.eq_ignore_ascii_case("TRUE") || trimmed.eq_ignore_ascii_case("FALSE") {
        return Ok(plain(Value::Bool(trimmed.eq_ignore_ascii_case("TRUE"))));
    }
    if let Some(error) = ErrorValue::parse(trimmed) {
        return Ok(plain(Value::Error(error)));
    }
    match parse_number(trimmed, system) {
        Some(n) => Ok(Typed {
            value: Value::Number(n),
            formula: None,
            number_format: implied_format(trimmed),
        }),
        None => Ok(plain(Value::text(input))),
    }
}

fn read_formula(formula: &str) -> Result<Typed, SheetError> {
    match crate::formula::parse(formula) {
        Ok(expr) => Ok(Typed {
            value: Value::Blank,
            formula: Some(crate::formula::to_string(&expr)),
            number_format: None,
        }),
        Err(error) => Err(SheetError::Invalid(format!(
            "There’s a problem with this formula: {error}."
        ))),
    }
}

/// the format excel gives a cell from how a number was typed
fn implied_format(typed: &str) -> Option<String> {
    let decimals = typed
        .split('.')
        .nth(1)
        .map(|d| d.chars().take_while(char::is_ascii_digit).count())
        .unwrap_or(0);
    let places = if decimals > 0 {
        format!(".{}", "0".repeat(decimals.min(10)))
    } else {
        String::new()
    };
    if typed.ends_with('%') {
        return Some(format!("0{places}%"));
    }
    if typed.contains('$') {
        return Some("\"$\"#,##0.00".into());
    }
    let looks_like_time = typed.contains(':');
    match (looks_like_date(typed), looks_like_time) {
        (true, true) => Some("m/d/yyyy h:mm".into()),
        (true, false) => Some("m/d/yyyy".into()),
        (false, true) if typed.to_ascii_uppercase().contains('M') => Some("h:mm AM/PM".into()),
        (false, true) if typed.matches(':').count() == 2 => Some("h:mm:ss".into()),
        (false, true) => Some("h:mm".into()),
        _ if typed.contains(',') => Some(format!("#,##0{places}")),
        _ => None,
    }
}

/// "3/1/2024", "1-Mar-24" (a dash and a month name) or "2024-03-01"
/// (two dashes, not a minus sign)
fn looks_like_date(typed: &str) -> bool {
    let dashes = typed.matches('-').count();
    let has_letters = typed.chars().any(|c| c.is_alphabetic());
    typed.contains('/') || (dashes >= 1 && has_letters) || (dashes == 2 && !typed.starts_with('-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(input: &str) -> Typed {
        read_input(input, DateSystem::Excel1900).unwrap()
    }

    #[test]
    fn typing_is_read_like_excel() {
        assert_eq!(read("42").value, Value::Number(42.0));
        assert_eq!(read("12%").number_format.as_deref(), Some("0%"));
        assert_eq!(read("12.5%").number_format.as_deref(), Some("0.0%"));
        assert_eq!(read("3/1/2024").number_format.as_deref(), Some("m/d/yyyy"));
        assert_eq!(read("3/1/2024").value, Value::Number(45352.0));
        assert_eq!(read("$5").number_format.as_deref(), Some("\"$\"#,##0.00"));
        assert_eq!(
            read("10:30 PM").number_format.as_deref(),
            Some("h:mm AM/PM")
        );
        assert_eq!(read("1,000").number_format.as_deref(), Some("#,##0"));
        assert_eq!(read("'007").value, Value::text("007"));
        assert_eq!(read("true").value, Value::Bool(true));
        assert_eq!(read("hello").value, Value::text("hello"));
        assert_eq!(read("=sum(a1:a3)").formula.as_deref(), Some("SUM(A1:A3)"));
        assert!(read_input("=SUM(", DateSystem::Excel1900).is_err());
        assert_eq!(read("-5").number_format, None);
    }
}
