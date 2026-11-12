//! dates and times as spreadsheet serial numbers.
//!
//! in the 1900 system day 1 is 1 january 1900 and, copying lotus 1-2-3,
//! day 60 is the 29 february 1900 that never was; so from 1 march 1900
//! on, serial n is n days after 30 december 1899. in the 1904 system day 0
//! is 1 january 1904. the fraction is the time of day. both checked
//! against excel: DATE(1900,3,1) is 61 and DATE(1900,2,29) is 60.

use crate::model::DateSystem;

/// days from 1 january of year 0 (proleptic gregorian) to a date, using
/// the usual civil-from-days arithmetic.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z - era * 146_097;
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

/// 30 december 1899, the zero the 1900 system counts from after february 1900
fn epoch_1900() -> i64 {
    days_from_civil(1899, 12, 30)
}

fn epoch_1904() -> i64 {
    days_from_civil(1904, 1, 1)
}

/// the serial for a calendar date. month and day may be out of range and
/// roll over (month 13 is january next year), as DATE does.
pub fn serial_from_date(year: i64, month: i64, day: i64, system: DateSystem) -> f64 {
    let total_months = year * 12 + (month - 1);
    let year = total_months.div_euclid(12);
    let month = total_months.rem_euclid(12) + 1;
    let first_of_month = match system {
        DateSystem::Excel1904 => days_from_civil(year, month, 1) - epoch_1904(),
        DateSystem::Excel1900 => first_of_month_1900(year, month),
    };
    (first_of_month + day - 1) as f64
}

/// the serial of the first of a month in excel's 1900 calendar, where
/// february 1900 has 29 days: DATE(1900,2,29) is 60 and DATE(1900,3,0)
/// is 60 too, as excel gives them
fn first_of_month_1900(year: i64, month: i64) -> i64 {
    match (year, month) {
        (1900, 1) => 1,
        (1900, 2) => 32,
        _ => {
            let serial = days_from_civil(year, month, 1) - epoch_1900();
            // before 1900 the count runs one day lower, like january 1900
            if serial < 61 {
                serial - 1
            } else {
                serial
            }
        }
    }
}

/// (year, month, day) of a serial's whole days. serial 60 in the 1900
/// system gives 29 february 1900, as excel shows it.
pub fn date_from_serial(serial: f64, system: DateSystem) -> (i64, u32, u32) {
    let whole = serial.floor() as i64;
    match system {
        DateSystem::Excel1904 => civil_from_days(whole + epoch_1904()),
        DateSystem::Excel1900 => {
            if whole == 60 {
                (1900, 2, 29)
            } else if whole == 0 {
                // excel's day zero: YEAR(0) is 1900, MONTH(0) 1 and DAY(0) 0
                (1900, 1, 0)
            } else if whole < 60 {
                civil_from_days(whole + epoch_1900() + 1)
            } else {
                civil_from_days(whole + epoch_1900())
            }
        }
    }
}

/// hours, minutes, seconds and milliseconds of a serial's fraction,
/// rounded to the millisecond as excel does before showing it.
pub fn time_from_serial(serial: f64) -> (u32, u32, u32, u32) {
    let fraction = serial - serial.floor();
    let total_ms = (fraction * 86_400_000.0).round() as u64;
    let total_ms = total_ms.min(86_399_999);
    let ms = (total_ms % 1000) as u32;
    let seconds = total_ms / 1000;
    (
        (seconds / 3600) as u32,
        ((seconds / 60) % 60) as u32,
        (seconds % 60) as u32,
        ms,
    )
}

/// day of the week, 0 for sunday. serial 1 (1 january 1900) was a
/// sunday in excel's calendar because of the fake leap day.
pub fn weekday(serial: f64, system: DateSystem) -> u32 {
    let whole = serial.floor() as i64;
    let shift = match system {
        DateSystem::Excel1900 => 6,
        // 1 january 1904 was a friday
        DateSystem::Excel1904 => 5,
    };
    (whole + shift).rem_euclid(7) as u32
}

pub fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap_year(year) => 29,
        _ => 28,
    }
}

/// reads an iso 8601 date or date-time ("2024-03-01", "2024-03-01T12:30:00")
/// into a serial, as xlsx `t="d"` cells and ods `office:date-value` store them.
pub fn serial_from_iso(text: &str, system: DateSystem) -> Option<f64> {
    let (date, time) = match text.split_once('T') {
        Some((date, time)) => (date, Some(time)),
        None => (text, None),
    };
    let mut parts = date.splitn(3, '-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.get(..2)?.parse().ok()?;
    let mut serial = serial_from_date(year, month, day, system);
    if let Some(time) = time {
        let time = time.trim_end_matches('Z');
        let mut pieces = time.split(':');
        let hours: f64 = pieces.next()?.parse().ok()?;
        let minutes: f64 = pieces.next().unwrap_or("0").parse().ok()?;
        let seconds: f64 = pieces.next().unwrap_or("0").parse().ok()?;
        serial += (hours * 3600.0 + minutes * 60.0 + seconds) / 86_400.0;
    }
    Some(serial)
}

/// a serial as iso 8601 ("2024-03-01" or with "T12:30:00").
pub fn iso_from_serial(serial: f64, system: DateSystem) -> String {
    let (year, month, day) = date_from_serial(serial, system);
    let (hours, minutes, seconds, _) = time_from_serial(serial);
    if serial.fract() == 0.0 {
        format!("{year:04}-{month:02}-{day:02}")
    } else {
        format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DateSystem::*;

    #[test]
    fn serials_match_excel_including_the_fake_leap_day() {
        assert_eq!(serial_from_date(1900, 1, 1, Excel1900), 1.0);
        assert_eq!(serial_from_date(1900, 2, 28, Excel1900), 59.0);
        assert_eq!(serial_from_date(1900, 3, 1, Excel1900), 61.0);
        assert_eq!(serial_from_date(2024, 2, 29, Excel1900), 45351.0);
        assert_eq!(date_from_serial(60.0, Excel1900), (1900, 2, 29));
        assert_eq!(date_from_serial(45351.0, Excel1900), (2024, 2, 29));
        assert_eq!(serial_from_date(1904, 1, 1, Excel1904), 0.0);
        assert_eq!(
            serial_from_date(2024, 14, 1, Excel1900),
            serial_from_date(2025, 2, 1, Excel1900)
        );
        assert_eq!(
            serial_from_date(2024, 3, 0, Excel1900),
            serial_from_date(2024, 2, 29, Excel1900)
        );
    }

    #[test]
    fn every_day_round_trips() {
        for serial in 61..80_000 {
            let (y, m, d) = date_from_serial(f64::from(serial), Excel1900);
            assert_eq!(
                serial_from_date(y, i64::from(m), i64::from(d), Excel1900),
                f64::from(serial)
            );
        }
    }

    #[test]
    fn times_and_weekdays() {
        assert_eq!(time_from_serial(0.75), (18, 0, 0, 0));
        assert_eq!(time_from_serial(45000.5 + 1.0 / 86_400.0), (12, 0, 1, 0));
        // 1 january 2024 was a monday
        assert_eq!(
            weekday(serial_from_date(2024, 1, 1, Excel1900), Excel1900),
            1
        );
        assert_eq!(
            serial_from_iso("2024-03-01T12:00:00", Excel1900),
            Some(45352.5)
        );
        assert_eq!(iso_from_serial(45352.5, Excel1900), "2024-03-01T12:00:00");
    }
}
