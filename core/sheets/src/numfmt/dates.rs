//! date and time sections: "d mmm yyyy", "h:mm AM/PM", "[h]:mm:ss",
//! "mm:ss.0". the time is rounded to the smallest unit the format shows
//! (seconds, or tenths and so on); without seconds, minutes are cut, not
//! rounded, as excel does.

use crate::dates::{date_from_serial, time_from_serial, weekday};
use crate::model::DateSystem;

use super::parse::{DatePart, Section, Token};

const MONTHS: [&str; 12] = [
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
const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// what excel shows when a date format meets a negative number or one
/// past the year 9999
const OUT_OF_RANGE: &str = "########";

fn smallest_unit(tokens: &[Token]) -> f64 {
    let mut unit = 0.0;
    for token in tokens {
        match token {
            Token::Date(DatePart::SubSecond(digits)) => {
                return 1.0 / 10f64.powi(i32::from(*digits)) / 86_400.0
            }
            Token::Date(DatePart::Second | DatePart::Second2 | DatePart::ElapsedSeconds(_)) => {
                unit = 1.0 / 86_400.0
            }
            _ => {}
        }
    }
    unit
}

pub fn render_date(serial: f64, section: &Section, system: DateSystem) -> String {
    if !(0.0..2_958_466.0).contains(&serial) {
        return OUT_OF_RANGE.into();
    }
    let unit = smallest_unit(&section.tokens);
    let serial = if unit > 0.0 {
        (serial / unit).round() * unit
    } else {
        serial
    };
    let twelve_hour = section
        .tokens
        .iter()
        .any(|t| matches!(t, Token::Date(DatePart::AmPm | DatePart::AP)));
    let (year, month, day) = date_from_serial(serial, system);
    let (hours, minutes, seconds, millis) = time_from_serial(serial);
    let mut out = String::new();
    for token in &section.tokens {
        match token {
            Token::Date(part) => out.push_str(&date_part(
                *part,
                serial,
                system,
                (year, month, day),
                (hours, minutes, seconds, millis),
                twelve_hour,
            )),
            Token::Literal(text) => out.push_str(text),
            Token::Space => out.push(' '),
            Token::DecimalPoint => out.push('.'),
            Token::Slash => out.push('/'),
            Token::Digit(c) => out.push(*c),
            Token::Thousands => out.push(','),
            _ => {}
        }
    }
    out
}

fn date_part(
    part: DatePart,
    serial: f64,
    system: DateSystem,
    (year, month, day): (i64, u32, u32),
    (hours, minutes, seconds, millis): (u32, u32, u32, u32),
    twelve_hour: bool,
) -> String {
    let clock_hours = if twelve_hour {
        match hours % 12 {
            0 => 12,
            h => h,
        }
    } else {
        hours
    };
    let month_name = MONTHS[(month as usize).saturating_sub(1) % 12];
    let day_name = DAYS[weekday(serial, system) as usize];
    match part {
        DatePart::Year2 => format!("{:02}", year % 100),
        DatePart::Year4 => format!("{year:04}"),
        DatePart::Month => month.to_string(),
        DatePart::Month2 => format!("{month:02}"),
        DatePart::MonthShort => month_name[..3].to_string(),
        DatePart::MonthLong => month_name.to_string(),
        DatePart::MonthLetter => month_name[..1].to_string(),
        // excel's day 0 ("1900-01-00") shows as 0
        DatePart::Day => {
            if serial < 1.0 {
                "0".into()
            } else {
                day.to_string()
            }
        }
        DatePart::Day2 => {
            if serial < 1.0 {
                "00".into()
            } else {
                format!("{day:02}")
            }
        }
        DatePart::DayShort => day_name[..3].to_string(),
        DatePart::DayLong => day_name.to_string(),
        DatePart::Hour => clock_hours.to_string(),
        DatePart::Hour2 => format!("{clock_hours:02}"),
        DatePart::Minute => minutes.to_string(),
        DatePart::Minute2 => format!("{minutes:02}"),
        DatePart::Second => seconds.to_string(),
        DatePart::Second2 => format!("{seconds:02}"),
        DatePart::SubSecond(digits) => {
            let text = format!("{millis:03}");
            format!(".{}", &text[..usize::from(digits.min(3))])
        }
        DatePart::AmPm => if hours < 12 { "AM" } else { "PM" }.to_string(),
        DatePart::AP => if hours < 12 { "A" } else { "P" }.to_string(),
        DatePart::ElapsedHours(width) => elapsed(serial * 24.0, width),
        DatePart::ElapsedMinutes(width) => elapsed(serial * 1440.0, width),
        DatePart::ElapsedSeconds(width) => elapsed(serial * 86_400.0, width),
    }
}

/// a whole count of hours, minutes or seconds, padded to the brackets' width
fn elapsed(total: f64, width: u8) -> String {
    // a hair of tolerance so 1.5 days shows 36 hours, not 35
    let whole = (total + 1e-9).floor() as i64;
    format!("{whole:0width$}", width = usize::from(width))
}

#[cfg(test)]
mod tests {
    use super::super::render::{format_number, Locale};
    use crate::model::DateSystem;

    fn show(n: f64, code: &str) -> String {
        format_number(n, code, &Locale::default(), DateSystem::Excel1900).text
    }

    #[test]
    fn dates_and_times_like_excel() {
        let date = 45352.75; // 1 march 2024, 18:00
        assert_eq!(show(date, "yyyy-mm-dd"), "2024-03-01");
        assert_eq!(show(date, "d mmm yyyy"), "1 Mar 2024");
        assert_eq!(show(date, "dddd, mmmm d"), "Friday, March 1");
        assert_eq!(show(date, "h:mm AM/PM"), "6:00 PM");
        assert_eq!(show(date, "hh:mm:ss"), "18:00:00");
        assert_eq!(show(1.5, "[h]:mm"), "36:00");
        assert_eq!(show(0.000_011_6, "mm:ss.0"), "00:01.0");
        assert_eq!(show(10.0 / 24.0 + 59.6 / 86_400.0, "h:mm:ss"), "10:01:00");
        assert_eq!(show(10.0 / 24.0 + 59.6 / 86_400.0, "h:mm"), "10:00");
        assert_eq!(show(-1.0, "d/m/yyyy"), "########");
        assert_eq!(show(60.0, "yyyy-mm-dd"), "1900-02-29");
        assert_eq!(show(date, "mmmmm"), "M");
    }
}
