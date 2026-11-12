//! reading a table cell's text as a spreadsheet value: numbers become
//! numbers (with a display format that shows them as they were printed:
//! currency, percent, thousands separators, decimals) and dates become
//! dates. anything else, or anything doubtful, stays text.

/// a cell's value and how to show it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CellValue {
    Empty,
    Text(String),
    Number {
        value: f64,
        /// an excel number format, none for "general"
        format: Option<String>,
    },
}

pub(crate) fn parse_cell(text: &str) -> CellValue {
    let text = text.trim();
    if text.is_empty() {
        return CellValue::Empty;
    }
    parse_date(text)
        .or_else(|| parse_number(text))
        .unwrap_or_else(|| CellValue::Text(text.to_string()))
}

/// a currency symbol at the start or end, and the format code for it.
const CURRENCIES: &[(&str, &str)] = &[
    ("$", "\"$\""),
    ("€", "\"€\""),
    ("£", "\"£\""),
    ("¥", "\"¥\""),
];

fn parse_number(text: &str) -> Option<CellValue> {
    let mut rest = text.replace('\u{a0}', " ");
    // brackets around an amount mean it is negative, in accounts
    let bracketed = rest.starts_with('(') && rest.ends_with(')');
    if bracketed {
        rest = rest[1..rest.len() - 1].trim().to_string();
    }
    let mut negative = bracketed;
    if let Some(stripped) = rest.strip_prefix(['-', '−']) {
        negative = !negative;
        rest = stripped.trim_start().to_string();
    }
    let mut currency = None;
    for (symbol, code) in CURRENCIES {
        if let Some(stripped) = rest.strip_prefix(symbol) {
            currency = Some((*code, true));
            rest = stripped.trim_start().to_string();
        } else if let Some(stripped) = rest.strip_suffix(symbol) {
            currency = Some((*code, false));
            rest = stripped.trim_end().to_string();
        }
    }
    if let Some(stripped) = rest.strip_prefix('-') {
        // "$-5.00"
        negative = !negative;
        rest = stripped.to_string();
    }
    let percent = rest.ends_with('%');
    if percent {
        rest = rest[..rest.len() - 1].trim_end().to_string();
    }
    let shape = number_shape(&rest)?;
    let mut value: f64 = rest.replace(',', "").parse().ok()?;
    if negative {
        value = -value;
    }
    if percent {
        value /= 100.0;
    }
    let digits = if shape.decimals > 0 {
        format!("0.{}", "0".repeat(shape.decimals))
    } else {
        "0".to_string()
    };
    let body = if shape.thousands {
        format!("#,##{digits}")
    } else {
        digits.clone()
    };
    let format = match (currency, percent) {
        (Some((code, before)), _) => {
            let shown = if before {
                format!("{code}{body}")
            } else {
                format!("{body} {code}")
            };
            if bracketed {
                Some(format!("{shown};({shown})"))
            } else {
                Some(shown)
            }
        }
        (None, true) => Some(format!("{digits}%")),
        (None, false) if shape.thousands || shape.decimals > 0 || bracketed => {
            if bracketed {
                Some(format!("{body};({body})"))
            } else {
                Some(body)
            }
        }
        _ => None,
    };
    Some(CellValue::Number { value, format })
}

struct NumberShape {
    thousands: bool,
    decimals: usize,
}

/// "1,234.50" has thousands separators and two decimals. numbers that
/// would lose something as a number stay text: leading zeros ("007"),
/// more digits than a spreadsheet keeps (card or phone numbers), and
/// separators in the wrong places.
fn number_shape(text: &str) -> Option<NumberShape> {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    if whole.is_empty() && fraction.is_none() {
        return None;
    }
    if let Some(fraction) = fraction {
        if fraction.is_empty() || !fraction.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
    }
    let groups: Vec<&str> = whole.split(',').collect();
    let thousands = groups.len() > 1;
    let groups_ok = groups.iter().enumerate().all(|(i, group)| {
        group.chars().all(|c| c.is_ascii_digit())
            && if i == 0 {
                !group.is_empty() || !thousands
            } else {
                group.len() == 3
            }
    }) && (!thousands || groups[0].len() <= 3);
    if !groups_ok {
        return None;
    }
    let digits: String = whole.chars().filter(char::is_ascii_digit).collect();
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    let significant = digits.len() + fraction.map_or(0, str::len);
    if significant > 15 || significant == 0 {
        return None;
    }
    Some(NumberShape {
        thousands,
        decimals: fraction.map_or(0, str::len),
    })
}

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

fn month_number(name: &str) -> Option<(u32, bool)> {
    let lower = name.trim_end_matches('.').to_lowercase();
    MONTHS
        .iter()
        .position(|month| *month == lower)
        .map(|i| (i as u32 + 1, true))
        .or_else(|| {
            (lower.len() == 3)
                .then(|| MONTHS.iter().position(|month| month.starts_with(&lower)))
                .flatten()
                .map(|i| (i as u32 + 1, false))
        })
}

/// "2024-03-15", "15/03/2024" (when the day is over 12, so it can't be a
/// month), "15 March 2024", "Mar 15, 2024". a date that could be read two
/// ways ("03/04/2024") stays text rather than risk the wrong day.
fn parse_date(text: &str) -> Option<CellValue> {
    let (year, month, day, format) = iso_date(text)
        .or_else(|| slash_date(text))
        .or_else(|| named_month_date(text))?;
    let serial = excel_serial(year, month, day)?;
    Some(CellValue::Number {
        value: serial as f64,
        format: Some(format.to_string()),
    })
}

fn iso_date(text: &str) -> Option<(i32, u32, u32, &'static str)> {
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
        "yyyy-mm-dd",
    ))
}

fn slash_date(text: &str) -> Option<(i32, u32, u32, &'static str)> {
    let parts: Vec<&str> = text.split('/').collect();
    if parts.len() != 3 || parts[2].len() != 4 || parts.iter().any(|p| p.is_empty() || p.len() > 4)
    {
        return None;
    }
    let (a, b): (u32, u32) = (parts[0].parse().ok()?, parts[1].parse().ok()?);
    let year = parts[2].parse().ok()?;
    match (a > 12, b > 12) {
        (true, false) => Some((year, b, a, "dd/mm/yyyy")),
        (false, true) => Some((year, a, b, "mm/dd/yyyy")),
        _ => None,
    }
}

fn named_month_date(text: &str) -> Option<(i32, u32, u32, &'static str)> {
    let words: Vec<&str> = text.split([' ', ',']).filter(|w| !w.is_empty()).collect();
    if words.len() != 3 || words[2].len() != 4 {
        return None;
    }
    let year = words[2].parse().ok()?;
    if let Some((month, long)) = month_number(words[0]) {
        let day = words[1].parse().ok()?;
        return Some((
            year,
            month,
            day,
            if long { "mmmm d, yyyy" } else { "mmm d, yyyy" },
        ));
    }
    let (month, long) = month_number(words[1])?;
    let day = words[0].parse().ok()?;
    Some((
        year,
        month,
        day,
        if long { "d mmmm yyyy" } else { "d mmm yyyy" },
    ))
}

/// the day number spreadsheets store dates as: days since 30 december
/// 1899 (which is how excel counts once past its 1900 leap year mistake).
fn excel_serial(year: i32, month: u32, day: u32) -> Option<i64> {
    if !(1900..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
    {
        return None;
    }
    let serial = days_from_civil(year, month, day) - days_from_civil(1899, 12, 30);
    // dates before 1 march 1900 count differently in excel; leave them as text
    (serial >= 61).then_some(serial)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 31,
    }
}

/// days since 1 january 1970 (howard hinnant's civil calendar formula).
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(text: &str) -> (f64, Option<String>) {
        match parse_cell(text) {
            CellValue::Number { value, format } => (value, format),
            other => panic!("{text} gave {other:?}"),
        }
    }

    #[test]
    fn numbers_keep_their_look() {
        assert_eq!(number("42"), (42.0, None));
        assert_eq!(number("12.50"), (12.5, Some("0.00".into())));
        assert_eq!(number("1,234,567"), (1_234_567.0, Some("#,##0".into())));
        assert_eq!(number("$1,234.50"), (1234.5, Some("\"$\"#,##0.00".into())));
        assert_eq!(number("-€3"), (-3.0, Some("\"€\"0".into())));
    }

    #[test]
    fn percentages_and_brackets() {
        assert_eq!(number("12.5%"), (0.125, Some("0.0%".into())));
        assert_eq!(
            number("($5.00)"),
            (-5.0, Some("\"$\"0.00;(\"$\"0.00)".into()))
        );
        assert_eq!(number("(12)"), (-12.0, Some("0;(0)".into())));
    }

    #[test]
    fn doubtful_numbers_stay_text() {
        for text in [
            "007",
            "1234567890123456",
            "1,23",
            "12.3.4",
            "Total",
            "4 apples",
            "12,5 %",
        ] {
            assert!(matches!(parse_cell(text), CellValue::Text(_)), "{text}");
        }
        assert_eq!(parse_cell("  "), CellValue::Empty);
    }

    #[test]
    fn dates_become_day_numbers() {
        // excel shows 45366 as 15 march 2024
        assert_eq!(number("2024-03-15"), (45366.0, Some("yyyy-mm-dd".into())));
        assert_eq!(number("15/03/2024"), (45366.0, Some("dd/mm/yyyy".into())));
        assert_eq!(number("03/15/2024"), (45366.0, Some("mm/dd/yyyy".into())));
        assert_eq!(
            number("15 March 2024"),
            (45366.0, Some("d mmmm yyyy".into()))
        );
        assert_eq!(
            number("Mar 15, 2024"),
            (45366.0, Some("mmm d, yyyy".into()))
        );
        assert!(matches!(parse_cell("03/04/2024"), CellValue::Text(_)));
        assert!(matches!(parse_cell("2024-02-30"), CellValue::Text(_)));
    }
}
