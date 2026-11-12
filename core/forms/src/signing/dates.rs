//! dates as signatures write them: pdf date strings in utc, readable utc
//! times for the visible signature, and calendar parts of a moment.

use std::time::SystemTime;

/// a pdf date string (D:YYYYMMDDHHmmSSZ) in utc.
pub(crate) fn pdf_date(time: SystemTime) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(time);
    format!("D:{year:04}{month:02}{day:02}{hour:02}{minute:02}{second:02}Z")
}

/// a readable utc time for the visible signature, like 2026-11-11 14:05 UTC.
pub(crate) fn readable_date(time: SystemTime) -> String {
    let (year, month, day, hour, minute, _) = utc_parts(time);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

/// splits a moment into its utc calendar parts, using the well known
/// days-to-civil-date conversion (howard hinnant's algorithm).
pub(crate) fn utc_parts(time: SystemTime) -> (i64, u32, u32, u32, u32, u32) {
    let seconds = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    let days = seconds.div_euclid(86_400);
    let in_day = seconds.rem_euclid(86_400);
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
    let year = year_of_era + era * 400 + if month <= 2 { 1 } else { 0 };
    (
        year,
        month,
        day,
        (in_day / 3600) as u32,
        (in_day % 3600 / 60) as u32,
        (in_day % 60) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn dates_are_written_in_utc() {
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_398_706);
        assert_eq!(pdf_date(time), "D:20261111120506Z");
        assert_eq!(
            readable_date(SystemTime::UNIX_EPOCH),
            "1970-01-01 00:00 UTC"
        );
        let leap_day = SystemTime::UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(pdf_date(leap_day), "D:20000229000000Z");
    }
}
