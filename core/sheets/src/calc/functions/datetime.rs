//! dates and times: TODAY, NOW, DATE, TIME, their parts, weekdays and
//! week numbers, month arithmetic, DATEDIF, working days and DAYS. dates
//! are serial numbers (see `crate::dates`).

use crate::dates::{date_from_serial, days_in_month, serial_from_date, time_from_serial, weekday};
use crate::formula::Expr;
use crate::model::{DateSystem, ErrorValue, Value};

use super::super::eval::Evaluator;
use super::super::value::{to_number, CalcValue};
use super::args::*;

pub const NAMES: &[&str] = &[
    "TODAY",
    "NOW",
    "DATE",
    "TIME",
    "YEAR",
    "MONTH",
    "DAY",
    "HOUR",
    "MINUTE",
    "SECOND",
    "WEEKDAY",
    "WEEKNUM",
    "ISOWEEKNUM",
    "EDATE",
    "EOMONTH",
    "DATEDIF",
    "NETWORKDAYS",
    "WORKDAY",
    "DAYS",
    "DATEVALUE",
    "TIMEVALUE",
    "DAYS360",
    "YEARFRAC",
];

/// the largest serial excel accepts: 31 december 9999
const LAST_SERIAL: f64 = 2_958_465.0;

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    let system = ev.system();
    let date_part = |pick: fn((i64, u32, u32)) -> f64| {
        lift_number(ev, args, &move |n| {
            valid_serial(n[0]).map(|s| pick(date_from_serial(to_second(s), system)))
        })
    };
    let time_part = |pick: fn((u32, u32, u32, u32)) -> u32| {
        lift_number(ev, args, &move |n| {
            valid_serial(n[0]).map(|s| f64::from(pick(time_from_serial(to_second(s)))))
        })
    };
    Some(match name {
        "TODAY" => CalcValue::number(ev.ctx.now.floor()),
        "NOW" => CalcValue::number(ev.ctx.now),
        "DATE" => lift_number(ev, args, &|n| date(n, system)),
        "TIME" => lift_number(ev, args, &|n| time(n)),
        "YEAR" => date_part(|(y, _, _)| y as f64),
        "MONTH" => date_part(|(_, m, _)| f64::from(m)),
        "DAY" => date_part(|(_, _, d)| f64::from(d)),
        "HOUR" => time_part(|(h, _, _, _)| h),
        "MINUTE" => time_part(|(_, m, _, _)| m),
        "SECOND" => time_part(|(_, _, s, _)| s),
        "WEEKDAY" => lift_number(ev, args, &|n| {
            weekday_number(to_second(n[0]), n.get(1).copied().unwrap_or(1.0), system)
        }),
        "WEEKNUM" => lift_number(ev, args, &|n| {
            week_number(n[0], n.get(1).copied().unwrap_or(1.0), system)
        }),
        "ISOWEEKNUM" => lift_number(ev, args, &|n| week_number(n[0], 21.0, system)),
        "EDATE" => lift_number(ev, args, &|n| {
            add_months(n[0], n.get(1).copied().unwrap_or(0.0), false, system)
        }),
        "EOMONTH" => lift_number(ev, args, &|n| {
            add_months(n[0], n.get(1).copied().unwrap_or(0.0), true, system)
        }),
        "DATEDIF" => done_number(date_difference(ev, args)),
        "NETWORKDAYS" => done_number(network_days(ev, args)),
        "WORKDAY" => done_number(workday(ev, args)),
        "DAYS" => lift_number(ev, args, &|n| {
            Ok(n[0].floor() - n.get(1).copied().unwrap_or(0.0).floor())
        }),
        "DAYS360" => lift_number(ev, args, &|n| {
            Ok(days360(
                n[0],
                n[1],
                n.get(2).copied().unwrap_or(0.0) != 0.0,
                system,
            ))
        }),
        "YEARFRAC" => lift_number(ev, args, &|n| {
            year_fraction(n[0], n[1], n.get(2).copied().unwrap_or(0.0), system)
        }),
        "DATEVALUE" => done_number(text_serial(ev, args).map(f64::floor)),
        "TIMEVALUE" => done_number(text_serial(ev, args).map(|s| s - s.floor())),
        _ => return None,
    })
}

/// excel rounds a serial to the nearest second before taking it apart:
/// DAY(45352.999999) is the next day and HOUR of it 0 (libreoffice does not)
fn to_second(serial: f64) -> f64 {
    (serial * 86_400.0).round() / 86_400.0
}

fn valid_serial(serial: f64) -> Outcome<f64> {
    if !(0.0..LAST_SERIAL + 1.0).contains(&serial) {
        return Err(ErrorValue::Num);
    }
    Ok(serial)
}

/// DATE(year, month, day): years 0-1899 are counted from 1900 and months
/// and days roll over, as in excel
fn date(n: &[f64], system: DateSystem) -> Outcome<f64> {
    let [year, month, day] = n else {
        return Err(ErrorValue::Value);
    };
    let mut year = year.trunc() as i64;
    if (0..1900).contains(&year) {
        year += 1900;
    }
    if !(1900..10_000).contains(&year) {
        return Err(ErrorValue::Num);
    }
    let serial = serial_from_date(year, month.trunc() as i64, day.trunc() as i64, system);
    valid_serial(serial)
}

/// TIME(hour, minute, second): the fraction of a day, rolling over past 24 hours
fn time(n: &[f64]) -> Outcome<f64> {
    let [hours, minutes, seconds] = n else {
        return Err(ErrorValue::Value);
    };
    let total = hours.trunc() * 3600.0 + minutes.trunc() * 60.0 + seconds.trunc();
    if total < 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(total.rem_euclid(86_400.0) / 86_400.0)
}

/// WEEKDAY's return types: 1 (sunday 1), 2 (monday 1), 3 (monday 0),
/// 11-17 (the week starts on monday..sunday, that day is 1)
fn weekday_number(serial: f64, kind: f64, system: DateSystem) -> Outcome<f64> {
    let day = f64::from(weekday(valid_serial(serial)?, system)); // 0 is sunday
    let kind = kind.trunc() as i64;
    let shifted = |first_day: f64| ((day - first_day).rem_euclid(7.0)) + 1.0;
    Ok(match kind {
        1 | 17 => day + 1.0,
        2 | 11 => shifted(1.0),
        3 => shifted(1.0) - 1.0,
        12..=16 => shifted((kind - 10) as f64),
        _ => return Err(ErrorValue::Num),
    })
}

/// WEEKNUM: week 1 holds 1 january, weeks start on the day `kind` says;
/// kind 21 is the iso week (weeks start on monday, week 1 holds a thursday)
fn week_number(serial: f64, kind: f64, system: DateSystem) -> Outcome<f64> {
    let serial = valid_serial(serial)?.floor();
    let kind = kind.trunc() as i64;
    if kind == 21 {
        return Ok(iso_week(serial, system));
    }
    let first_weekday = match kind {
        1 | 17 => 0.0,
        2 | 11 => 1.0,
        12..=16 => (kind - 10) as f64,
        _ => return Err(ErrorValue::Num),
    };
    let (year, _, _) = date_from_serial(serial, system);
    let january_first = serial_from_date(year, 1, 1, system);
    let offset = (f64::from(weekday(january_first, system)) - first_weekday).rem_euclid(7.0);
    Ok(((serial - january_first + offset) / 7.0).floor() + 1.0)
}

fn iso_week(serial: f64, system: DateSystem) -> f64 {
    let monday_based = (f64::from(weekday(serial, system)) + 6.0).rem_euclid(7.0); // 0 is monday
    let thursday = serial - monday_based + 3.0;
    let (year, _, _) = date_from_serial(thursday, system);
    let january_first = serial_from_date(year, 1, 1, system);
    ((thursday - january_first) / 7.0).floor() + 1.0
}

/// EDATE and EOMONTH: the same day (or the month's last day) `months` later;
/// a day past the end of the target month becomes its last day
fn add_months(serial: f64, months: f64, end_of_month: bool, system: DateSystem) -> Outcome<f64> {
    let (year, month, day) = date_from_serial(valid_serial(serial)?, system);
    let total = year * 12 + i64::from(month) - 1 + months.trunc() as i64;
    let (year, month) = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
    if !(1900..10_000).contains(&year) {
        return Err(ErrorValue::Num);
    }
    let last = days_in_month(year, month);
    let day = if end_of_month { last } else { day.min(last) };
    Ok(serial_from_date(
        year,
        i64::from(month),
        i64::from(day),
        system,
    ))
}

/// DATEDIF(start, end, unit) with units Y, M, D, MD, YM, YD
fn date_difference(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    arity(args, 3, 3)?;
    let system = ev.system();
    let start = valid_serial(number(ev, &args[0])?)?.floor();
    let end = valid_serial(number(ev, &args[1])?)?.floor();
    if start > end {
        return Err(ErrorValue::Num);
    }
    let unit = text(ev, &args[2])?.to_ascii_uppercase();
    let (y1, m1, d1) = date_from_serial(start, system);
    let (y2, m2, d2) = date_from_serial(end, system);
    let months = (y2 - y1) * 12 + i64::from(m2) - i64::from(m1) - i64::from(d2 < d1);
    Ok(match unit.as_str() {
        "D" => end - start,
        "M" => months as f64,
        "Y" => (months / 12) as f64,
        "YM" => (months % 12) as f64,
        "MD" => {
            // excel's own arithmetic, which gives -1 for 31 january to 1
            // march in a leap year: the length of the month before `end`
            // is added, whatever the start day (libreoffice agrees)
            if d2 >= d1 {
                f64::from(d2 - d1)
            } else {
                let (py, pm) = if m2 == 1 { (y2 - 1, 12) } else { (y2, m2 - 1) };
                f64::from(d2) + f64::from(days_in_month(py, pm)) - f64::from(d1)
            }
        }
        "YD" => {
            let mut anniversary = serial_from_date(y2, i64::from(m1), i64::from(d1), system);
            if anniversary > end {
                anniversary = serial_from_date(y2 - 1, i64::from(m1), i64::from(d1), system);
            }
            end - anniversary
        }
        _ => return Err(ErrorValue::Num),
    })
}

fn holidays(ev: &Evaluator, args: &[Expr], index: usize) -> Outcome<Vec<f64>> {
    match args.get(index) {
        None | Some(Expr::Missing) => Ok(Vec::new()),
        Some(expr) => Ok(numbers(ev, std::slice::from_ref(expr))?
            .into_iter()
            .map(f64::floor)
            .collect()),
    }
}

fn is_workday(serial: f64, holidays: &[f64], system: DateSystem) -> bool {
    let day = weekday(serial, system);
    day != 0 && day != 6 && !holidays.contains(&serial)
}

/// NETWORKDAYS(start, end, [holidays]): working days with both ends counted
fn network_days(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    arity(args, 2, 3)?;
    let system = ev.system();
    let start = valid_serial(number(ev, &args[0])?)?.floor();
    let end = valid_serial(number(ev, &args[1])?)?.floor();
    let holidays = holidays(ev, args, 2)?;
    let (low, high, sign) = if start <= end {
        (start, end, 1.0)
    } else {
        (end, start, -1.0)
    };
    let count = (low as i64..=high as i64)
        .filter(|day| is_workday(*day as f64, &holidays, system))
        .count();
    Ok(sign * count as f64)
}

/// WORKDAY(start, days, [holidays]): the working day `days` after start
fn workday(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    arity(args, 2, 3)?;
    let system = ev.system();
    let mut day = valid_serial(number(ev, &args[0])?)?.floor();
    let mut remaining = number(ev, &args[1])?.trunc() as i64;
    let holidays = holidays(ev, args, 2)?;
    let step = if remaining < 0 { -1.0 } else { 1.0 };
    while remaining != 0 {
        day += step;
        if is_workday(day, &holidays, system) {
            remaining -= step as i64;
        }
        valid_serial(day)?;
    }
    Ok(day)
}

/// DAYS360: the us (nasd) method, or the european one when `european`
fn days360(start: f64, end: f64, european: bool, system: DateSystem) -> f64 {
    let (y1, m1, mut d1) = date_from_serial(start, system);
    let (y2, m2, mut d2) = date_from_serial(end, system);
    if european {
        d1 = d1.min(30);
        d2 = d2.min(30);
    } else {
        let last_of_february = |y: i64, m: u32, d: u32| m == 2 && d == days_in_month(y, 2);
        if last_of_february(y1, m1, d1) {
            d1 = 30;
        }
        if d1 == 31 {
            d1 = 30;
        }
        if d2 == 31 && d1 >= 30 {
            d2 = 30;
        }
    }
    ((y2 - y1) * 360 + (i64::from(m2) - i64::from(m1)) * 30 + i64::from(d2) - i64::from(d1)) as f64
}

/// YEARFRAC with bases 0 (30/360 us), 1 (actual/actual), 2 (actual/360),
/// 3 (actual/365), 4 (30/360 european)
fn year_fraction(start: f64, end: f64, basis: f64, system: DateSystem) -> Outcome<f64> {
    let (start, end) = (
        start.floor().min(end.floor()),
        start.floor().max(end.floor()),
    );
    Ok(match basis.trunc() as i64 {
        0 => days360(start, end, false, system) / 360.0,
        1 => actual_year_fraction(start, end, system),
        2 => (end - start) / 360.0,
        3 => (end - start) / 365.0,
        4 => days360(start, end, true, system) / 360.0,
        _ => return Err(ErrorValue::Num),
    })
}

fn actual_year_fraction(start: f64, end: f64, system: DateSystem) -> f64 {
    let (y1, _, _) = date_from_serial(start, system);
    let (y2, _, _) = date_from_serial(end, system);
    let year_length = |y: i64| {
        if crate::dates::is_leap_year(y) {
            366.0
        } else {
            365.0
        }
    };
    let average = if y1 == y2 {
        year_length(y1)
    } else {
        let days = serial_from_date(y2 + 1, 1, 1, system) - serial_from_date(y1, 1, 1, system);
        days / (y2 - y1 + 1) as f64
    };
    (end - start) / average
}

/// DATEVALUE and TIMEVALUE read text only
fn text_serial(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    arity(args, 1, 1)?;
    match ev.scalar(&args[0]) {
        Value::Text(text) => to_number(&Value::Text(text), ev.system()),
        Value::Error(e) => Err(e),
        _ => Err(ErrorValue::Value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DateSystem::Excel1900 as S;

    #[test]
    fn dates_roll_over_like_excel() {
        assert_eq!(date(&[2024.0, 13.0, 1.0], S), Ok(45658.0));
        assert_eq!(
            date(&[100.0, 1.0, 1.0], S),
            Ok(serial_from_date(2000, 1, 1, S))
        );
        assert_eq!(time(&[25.0, 0.0, 0.0]), Ok(1.0 / 24.0));
        assert_eq!(
            add_months(serial_from_date(2024, 1, 31, S), 1.0, false, S),
            Ok(serial_from_date(2024, 2, 29, S))
        );
        assert_eq!(
            add_months(serial_from_date(2024, 1, 15, S), -1.0, true, S),
            Ok(serial_from_date(2023, 12, 31, S))
        );
    }

    #[test]
    fn weekdays_and_weeks() {
        let monday = serial_from_date(2024, 1, 1, S);
        assert_eq!(weekday_number(monday, 1.0, S), Ok(2.0));
        assert_eq!(weekday_number(monday, 2.0, S), Ok(1.0));
        assert_eq!(weekday_number(monday, 3.0, S), Ok(0.0));
        assert_eq!(
            week_number(serial_from_date(2024, 1, 7, S), 1.0, S),
            Ok(2.0)
        );
        assert_eq!(
            week_number(serial_from_date(2021, 1, 3, S), 21.0, S),
            Ok(53.0)
        );
        assert_eq!(
            days360(
                serial_from_date(2024, 1, 31, S),
                serial_from_date(2024, 3, 31, S),
                false,
                S
            ),
            60.0
        );
    }
}
