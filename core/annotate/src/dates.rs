//! pdf dates ("D:20261111183000+00'00'") and their ISO 8601 form for the ui.

use std::time::{SystemTime, UNIX_EPOCH};

/// the current time as a pdf date, in utc.
pub(crate) fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    pdf_date_from_unix(seconds)
}

/// a moment in seconds since 1970 as a pdf date in utc.
pub(crate) fn pdf_date_from_unix(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let in_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "D:{year:04}{month:02}{day:02}{:02}{:02}{:02}+00'00'",
        in_day / 3600,
        in_day % 3600 / 60,
        in_day % 60
    )
}

/// turns a pdf date into ISO 8601 ("2026-11-11T18:30:00Z"), or none if
/// it can't be read. pdf dates may stop after any field, so missing
/// parts take their earliest value, as the pdf standard says.
pub(crate) fn iso_from_pdf_date(text: &str) -> Option<String> {
    let text = text.trim();
    let text = text.strip_prefix("D:").unwrap_or(text);
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let field = |start: usize, default: u32| -> u32 {
        digits
            .get(start..start + 2)
            .and_then(|part| part.parse().ok())
            .unwrap_or(default)
    };
    let year: u32 = digits[0..4].parse().ok()?;
    let (month, day) = (field(4, 1), field(6, 1));
    let (hour, minute, second) = (field(8, 0), field(10, 0), field(12, 0));
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    let zone = zone_suffix(&text[digits.len()..]);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{:02}{zone}",
        second.min(59)
    ))
}

/// the time zone part of a pdf date ("Z", "+01'00'", "-05'30") in ISO form.
fn zone_suffix(rest: &str) -> String {
    let mut chars = rest.chars();
    match chars.next() {
        Some(sign @ ('+' | '-')) => {
            let digits: String = chars.filter(char::is_ascii_digit).take(4).collect();
            let hours = digits.get(0..2).unwrap_or("00");
            let minutes = digits.get(2..4).unwrap_or("00");
            if hours == "00" && minutes == "00" {
                "Z".into()
            } else {
                format!("{sign}{hours}:{minutes}")
            }
        }
        // no zone means "unknown"; utc is the closest honest guess
        _ => "Z".into(),
    }
}

/// year, month and day for a count of days since 1970-01-01.
///
/// this is the well known "civil from days" calculation by Howard
/// Hinnant, which counts in 400 year cycles starting in march so that
/// leap days fall at the end of each year.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_pdf_dates_in_utc() {
        assert_eq!(pdf_date_from_unix(0), "D:19700101000000+00'00'");
        // 2024 was a leap year: the 29th of february exists
        assert_eq!(pdf_date_from_unix(1_709_210_096), "D:20240229123456+00'00'");
    }

    #[test]
    fn reads_full_and_partial_pdf_dates() {
        assert_eq!(
            iso_from_pdf_date("D:20261111183000+00'00'").as_deref(),
            Some("2026-11-11T18:30:00Z")
        );
        assert_eq!(
            iso_from_pdf_date("D:20260102030405-05'30'").as_deref(),
            Some("2026-01-02T03:04:05-05:30")
        );
        assert_eq!(
            iso_from_pdf_date("D:2025").as_deref(),
            Some("2025-01-01T00:00:00Z")
        );
        assert_eq!(iso_from_pdf_date("yesterday"), None);
    }

    #[test]
    fn now_is_a_readable_date() {
        assert!(iso_from_pdf_date(&now()).is_some());
    }
}
