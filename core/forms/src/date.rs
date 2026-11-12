//! date fields: reading a date and writing it in the field's pattern.
//!
//! pdf has no date field of its own. acrobat makes one by giving a text
//! field a small script, AFDate_FormatEx("dd/mm/yyyy"), that formats what
//! is typed. letters reads that pattern, never runs the script, and
//! formats the date itself. patterns use acrobat's letters: yyyy, yy,
//! mmmm (January), mmm (Jan), mm, m, dd, d.

use crate::error::{FormError, FormResult};

/// a calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// the patterns behind acrobat's numbered AFDate_Format(n) choices.
const NUMBERED_PATTERNS: [&str; 12] = [
    "m/d",
    "m/d/yy",
    "mm/dd/yy",
    "mm/yy",
    "d-mmm",
    "d-mmm-yy",
    "dd-mmm-yy",
    "yy-mm-dd",
    "mmm-yy",
    "mmmm-yy",
    "mmm d, yyyy",
    "mmmm d, yyyy",
];

/// the pattern used when a new date field is given none.
pub const DEFAULT_PATTERN: &str = "yyyy-mm-dd";

impl Date {
    pub fn new(year: i32, month: u32, day: u32) -> Option<Self> {
        let valid = (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month);
        valid.then_some(Self { year, month, day })
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// finds the date pattern in a field's format script, if it has one.
pub fn pattern_from_script(script: &str) -> Option<String> {
    for function in ["AFDate_FormatEx", "AFDate_KeystrokeEx"] {
        if let Some(start) = script.find(function) {
            let rest = &script[start + function.len()..];
            let quote_start = rest.find(['"', '\''])?;
            let quote = rest[quote_start..].chars().next()?;
            let after = &rest[quote_start + 1..];
            let end = after.find(quote)?;
            return Some(after[..end].to_owned());
        }
    }
    let start = script.find("AFDate_Format(")?;
    let rest = &script[start + "AFDate_Format(".len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let index: usize = digits.parse().ok()?;
    NUMBERED_PATTERNS
        .get(index)
        .map(|pattern| (*pattern).to_owned())
}

/// the script letters writes into a new date field so other pdf apps see
/// it as a date field too.
pub fn format_script(pattern: &str) -> String {
    format!("AFDate_FormatEx(\"{}\");", pattern.replace('"', ""))
}

pub fn keystroke_script(pattern: &str) -> String {
    format!("AFDate_KeystrokeEx(\"{}\");", pattern.replace('"', ""))
}

/// reads what someone typed: an iso date (2026-11-11), or a date already
/// written in the field's own pattern.
pub fn parse_date(input: &str, pattern: &str) -> FormResult<Date> {
    let input = input.trim();
    parse_with_pattern(input, DEFAULT_PATTERN)
        .or_else(|| parse_with_pattern(input, pattern))
        .ok_or_else(|| FormError::BadDate(input.to_owned()))
}

/// writes a date in a pattern such as "dd/mm/yyyy" or "mmmm d, yyyy".
pub fn format_date(date: Date, pattern: &str) -> String {
    let mut out = String::new();
    for token in tokenize(pattern) {
        match token {
            Token::Year4 => out.push_str(&format!("{:04}", date.year)),
            Token::Year2 => out.push_str(&format!("{:02}", date.year.rem_euclid(100))),
            Token::MonthName => out.push_str(MONTH_NAMES[date.month as usize - 1]),
            Token::MonthShort => out.push_str(&MONTH_NAMES[date.month as usize - 1][..3]),
            Token::Month2 => out.push_str(&format!("{:02}", date.month)),
            Token::Month1 => out.push_str(&date.month.to_string()),
            Token::Day2 => out.push_str(&format!("{:02}", date.day)),
            Token::Day1 => out.push_str(&date.day.to_string()),
            Token::Literal(character) => out.push(character),
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Token {
    Year4,
    Year2,
    MonthName,
    MonthShort,
    Month2,
    Month1,
    Day2,
    Day1,
    Literal(char),
}

/// splits a pattern into its pieces. the longest run of a letter wins,
/// so "mmmm" is one month name and not four months.
fn tokenize(pattern: &str) -> Vec<Token> {
    let characters: Vec<char> = pattern.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let letter = characters[index];
        let run = characters[index..]
            .iter()
            .take_while(|&&next| next == letter)
            .count();
        let token = match (letter, run) {
            ('y', 4..) => Some((Token::Year4, 4)),
            ('y', _) => Some((Token::Year2, run.min(2))),
            ('m', 4..) => Some((Token::MonthName, 4)),
            ('m', 3) => Some((Token::MonthShort, 3)),
            ('m', 2) => Some((Token::Month2, 2)),
            ('m', 1) => Some((Token::Month1, 1)),
            ('d', 2..) => Some((Token::Day2, 2)),
            ('d', 1) => Some((Token::Day1, 1)),
            _ => None,
        };
        match token {
            Some((token, used)) => {
                tokens.push(token);
                index += used;
            }
            None => {
                tokens.push(Token::Literal(letter));
                index += 1;
            }
        }
    }
    tokens
}

/// reads a date written in a pattern, or none if it doesn't fit.
fn parse_with_pattern(input: &str, pattern: &str) -> Option<Date> {
    let mut rest = input;
    let (mut year, mut month, mut day) = (None, None, None);
    for token in tokenize(pattern) {
        match token {
            Token::Literal(character) => {
                rest = rest.strip_prefix(character)?;
            }
            Token::Year4 => year = Some(take_number(&mut rest, 4, 4)? as i32),
            Token::Year2 => year = Some(2000 + take_number(&mut rest, 2, 2)? as i32),
            Token::Month2 | Token::Month1 => month = Some(take_number(&mut rest, 1, 2)?),
            Token::Day2 | Token::Day1 => day = Some(take_number(&mut rest, 1, 2)?),
            Token::MonthName | Token::MonthShort => month = Some(take_month_name(&mut rest)?),
        }
    }
    if !rest.is_empty() {
        return None;
    }
    Date::new(year?, month?, day.unwrap_or(1))
}

fn take_number(rest: &mut &str, min_digits: usize, max_digits: usize) -> Option<u32> {
    let digits: String = rest
        .chars()
        .take_while(char::is_ascii_digit)
        .take(max_digits)
        .collect();
    if digits.len() < min_digits {
        return None;
    }
    *rest = &rest[digits.len()..];
    digits.parse().ok()
}

fn take_month_name(rest: &mut &str) -> Option<u32> {
    let letters: String = rest.chars().take_while(char::is_ascii_alphabetic).collect();
    if letters.len() < 3 {
        return None;
    }
    let lower = letters.to_ascii_lowercase();
    let month = MONTH_NAMES.iter().position(|name| {
        name.to_ascii_lowercase().starts_with(&lower) || lower == name[..3].to_ascii_lowercase()
    })?;
    *rest = &rest[letters.len()..];
    Some(month as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_common_patterns() {
        let date = Date::new(2026, 3, 7).unwrap();
        assert_eq!(format_date(date, "dd/mm/yyyy"), "07/03/2026");
        assert_eq!(format_date(date, "m/d/yy"), "3/7/26");
        assert_eq!(format_date(date, "mmmm d, yyyy"), "March 7, 2026");
        assert_eq!(format_date(date, "d-mmm-yy"), "7-Mar-26");
    }

    #[test]
    fn reads_iso_and_the_fields_own_pattern() {
        assert_eq!(
            parse_date("2026-11-11", "dd.mm.yyyy").unwrap(),
            Date::new(2026, 11, 11).unwrap()
        );
        assert_eq!(
            parse_date("11.12.2026", "dd.mm.yyyy").unwrap(),
            Date::new(2026, 12, 11).unwrap()
        );
        assert_eq!(
            parse_date("March 7, 2026", "mmmm d, yyyy").unwrap(),
            Date::new(2026, 3, 7).unwrap()
        );
    }

    #[test]
    fn refuses_impossible_dates() {
        assert!(parse_date("2026-02-30", "yyyy-mm-dd").is_err());
        assert!(parse_date("2024-02-29", "yyyy-mm-dd").is_ok());
        assert!(parse_date("tomorrow", "yyyy-mm-dd").is_err());
    }

    #[test]
    fn finds_the_pattern_in_acrobat_scripts() {
        assert_eq!(
            pattern_from_script("AFDate_FormatEx(\"dd/mm/yyyy\");").unwrap(),
            "dd/mm/yyyy"
        );
        assert_eq!(
            pattern_from_script("AFDate_KeystrokeEx('yyyy-mm-dd')").unwrap(),
            "yyyy-mm-dd"
        );
        assert_eq!(
            pattern_from_script("AFDate_Format(2);").unwrap(),
            "mm/dd/yy"
        );
        assert_eq!(pattern_from_script("AFNumber_Format(2)"), None);
    }
}
