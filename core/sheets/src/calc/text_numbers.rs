//! reading numbers, dates and times typed as text, the way excel does
//! when text is used in arithmetic or given to VALUE ("1,234", "$5",
//! "12%", "(3)", "2024-03-01", "3/1/2024", "1-Mar-24", "10:30 PM").
//! the en-US reading of dates (month first) is used.

use crate::dates::serial_from_date;
use crate::model::DateSystem;

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

/// the number a text stands for, if it reads as one.
pub fn parse_number(text: &str, system: DateSystem) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    plain_number(text)
        .or_else(|| parse_date_time(text, system))
        .or_else(|| parse_time(text))
}

/// "1,234.5", "-$12", "(3)", "12%", "1e3"
fn plain_number(text: &str) -> Option<f64> {
    let mut body = text.trim();
    let mut negative = false;
    if let Some(inner) = body.strip_prefix('(').and_then(|b| b.strip_suffix(')')) {
        negative = true;
        body = inner.trim();
    }
    if let Some(rest) = body.strip_prefix('-') {
        negative = !negative;
        body = rest.trim_start();
    } else if let Some(rest) = body.strip_prefix('+') {
        body = rest.trim_start();
    }
    body = body.strip_prefix('$').unwrap_or(body).trim_start();
    if let Some(rest) = body.strip_prefix('-') {
        negative = !negative;
        body = rest;
    }
    let percent = body.ends_with('%');
    let body = body.trim_end_matches('%').trim_end();
    if !thousands_are_well_placed(body) {
        return None;
    }
    let digits: String = body.chars().filter(|c| *c != ',').collect();
    let starts_well = digits.starts_with(|c: char| c.is_ascii_digit() || c == '.');
    if !starts_well || digits.contains(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E') {
        return None;
    }
    let mut value: f64 = digits.parse().ok()?;
    if percent {
        value /= 100.0;
    }
    Some(if negative { -value } else { value })
}

/// excel accepts "1,234" but not "12,34": commas must split groups of three
fn thousands_are_well_placed(body: &str) -> bool {
    if !body.contains(',') {
        return true;
    }
    let whole = body.split('.').next().unwrap_or("");
    let groups: Vec<&str> = whole.split(',').collect();
    let first_ok = !groups[0].is_empty() && groups[0].len() <= 3;
    first_ok
        && groups[1..]
            .iter()
            .all(|group| group.len() == 3 && group.bytes().all(|b| b.is_ascii_digit()))
}

fn month_number(word: &str) -> Option<i64> {
    let word = word.trim_end_matches('.').to_ascii_lowercase();
    if word.len() < 3 {
        return None;
    }
    MONTHS
        .iter()
        .position(|month| month.starts_with(&word))
        .map(|index| index as i64 + 1)
}

/// two-digit years: 00-29 are 2000-2029, 30-99 are 1930-1999 (excel's rule)
fn full_year(year: i64, written_digits: usize) -> i64 {
    if written_digits > 2 {
        year
    } else if year < 30 {
        2000 + year
    } else {
        1900 + year
    }
}

fn this_year() -> i64 {
    use chrono::Datelike;
    i64::from(chrono::Local::now().year())
}

fn valid_date(year: i64, month: i64, day: i64) -> bool {
    (1..=12).contains(&month)
        && day >= 1
        && day <= i64::from(crate::dates::days_in_month(year, month as u32))
        && (1900..=9999).contains(&year)
}

/// a date, with an optional time after it
fn parse_date_time(text: &str, system: DateSystem) -> Option<f64> {
    if let Some(date) = parse_date(text, system) {
        return Some(date);
    }
    // "3/1/2024 10:30": the date is everything before the first space
    // that leaves a time behind
    for (index, _) in text.match_indices(' ') {
        let (date, time) = text.split_at(index);
        if let (Some(date), Some(time)) = (parse_date(date, system), parse_time(time.trim())) {
            return Some(date + time);
        }
    }
    None
}

fn parse_date(text: &str, system: DateSystem) -> Option<f64> {
    let text = text.trim().trim_end_matches(',');
    let pieces: Vec<&str> = text
        .split(['/', '-', ' ', ','])
        .filter(|piece| !piece.is_empty())
        .collect();
    let (year, month, day) = match pieces.as_slice() {
        [a, b, c] => three_part_date(a, b, c)?,
        [a, b] => two_part_date(a, b)?,
        _ => return None,
    };
    valid_date(year, month, day).then(|| serial_from_date(year, month, day, system))
}

/// a piece made only of digits, with how many digits it had
fn number_of(piece: &str) -> Option<(i64, usize)> {
    if !piece.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let value = piece.parse().ok()?;
    Some((value, piece.len()))
}

/// "2024-03-01", "3/1/2024", "1-Mar-2024", "Mar 1 2024"
fn three_part_date(a: &str, b: &str, c: &str) -> Option<(i64, i64, i64)> {
    if let (Some((y, 4)), Some((m, _)), Some((d, _))) = (number_of(a), number_of(b), number_of(c)) {
        return Some((y, m, d));
    }
    if let (Some((m, _)), Some((d, _)), Some((y, digits))) =
        (number_of(a), number_of(b), number_of(c))
    {
        return Some((full_year(y, digits), m, d));
    }
    if let (Some((d, _)), Some(m), Some((y, digits))) =
        (number_of(a), month_number(b), number_of(c))
    {
        return Some((full_year(y, digits), m, d));
    }
    if let (Some(m), Some((d, _)), Some((y, digits))) =
        (month_number(a), number_of(b), number_of(c))
    {
        return Some((full_year(y, digits), m, d));
    }
    None
}

/// "3/1" and "1-Mar" are this year; "Mar-24" and "Mar 2024" are the first
/// of that month
fn two_part_date(a: &str, b: &str) -> Option<(i64, i64, i64)> {
    if let (Some((m, _)), Some((d, _))) = (number_of(a), number_of(b)) {
        return Some((this_year(), m, d));
    }
    if let (Some((d, _)), Some(m)) = (number_of(a), month_number(b)) {
        return Some((this_year(), m, d));
    }
    if let (Some(m), Some((n, digits))) = (month_number(a), number_of(b)) {
        // a number that can be a day is one ("Mar 15" is 15 march this
        // year); otherwise it is a year ("Mar 2024" is 1 march 2024)
        if digits <= 2 && (1..=31).contains(&n) {
            return Some((this_year(), m, n));
        }
        return Some((full_year(n, digits), m, 1));
    }
    None
}

/// "10:30", "10:30:15", "10:30 PM", "7 AM", "1:02:03.5"
fn parse_time(text: &str) -> Option<f64> {
    let upper = text.trim().to_ascii_uppercase();
    let (clock, meridiem) =
        if let Some(rest) = upper.strip_suffix("AM").or_else(|| upper.strip_suffix('A')) {
            (rest.trim(), Some(false))
        } else if let Some(rest) = upper.strip_suffix("PM").or_else(|| upper.strip_suffix('P')) {
            (rest.trim(), Some(true))
        } else {
            (upper.as_str(), None)
        };
    if !clock.contains(':') && meridiem.is_none() {
        return None;
    }
    let mut parts = clock.split(':');
    let hours: f64 = parts.next()?.trim().parse().ok()?;
    let minutes: f64 = parts
        .next()
        .map(|m| m.trim().parse())
        .transpose()
        .ok()?
        .unwrap_or(0.0);
    let seconds: f64 = parts
        .next()
        .map(|s| s.trim().parse())
        .transpose()
        .ok()?
        .unwrap_or(0.0);
    if parts.next().is_some() || minutes >= 60.0 || seconds >= 60.0 {
        return None;
    }
    let hours = match meridiem {
        Some(_) if !(1.0..=12.0).contains(&hours) => return None,
        Some(true) if hours < 12.0 => hours + 12.0,
        Some(false) if hours == 12.0 => 0.0,
        _ => hours,
    };
    Some((hours * 3600.0 + minutes * 60.0 + seconds) / 86_400.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Option<f64> {
        parse_number(text, DateSystem::Excel1900)
    }

    #[test]
    fn plain_numbers() {
        assert_eq!(read("1,234.5"), Some(1234.5));
        assert_eq!(read("-$12"), Some(-12.0));
        assert_eq!(read("(3)"), Some(-3.0));
        assert_eq!(read("12%"), Some(0.12));
        assert_eq!(read("1e3"), Some(1000.0));
        assert_eq!(read(".5"), Some(0.5));
        assert_eq!(read("12,34"), None);
        assert_eq!(read("abc"), None);
        assert_eq!(read(""), None);
        assert_eq!(read("TRUE"), None);
    }

    #[test]
    fn dates_and_times() {
        assert_eq!(read("2024-03-01"), Some(45352.0));
        assert_eq!(read("3/1/2024"), Some(45352.0));
        assert_eq!(read("1-Mar-2024"), Some(45352.0));
        assert_eq!(read("March 1, 2024"), Some(45352.0));
        assert_eq!(read("Mar 2024"), Some(45352.0));
        assert_eq!(read("10:30"), Some(0.4375));
        assert_eq!(read("10:30 PM"), Some(22.5 / 24.0));
        assert_eq!(read("12:00 AM"), Some(0.0));
        assert_eq!(read("3/1/2024 12:00"), Some(45352.5));
        assert_eq!(read("2/30/2024"), None);
    }
}
