//! autofill: what comes next when a person drags the fill handle.
//! numbers follow their straight-line trend (excel fits a line when given
//! two or more), dates step by days, months or years, day and month
//! names cycle, "Item 1" counts up, "Q1" runs to "Q4" and around; anything
//! else is repeated.

use crate::dates::{date_from_serial, days_in_month, serial_from_date};
use crate::model::{DateSystem, Value};

const DAYS: [&str; 7] = [
    "sunday",
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
];
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

/// one source cell of a fill
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesItem {
    pub value: Value,
    /// shown as a date, so it steps by calendar units
    pub is_date: bool,
}

/// the next `count` values after `items`
pub fn extend(items: &[SeriesItem], count: usize, system: DateSystem) -> Vec<Value> {
    if items.is_empty() {
        return vec![Value::Blank; count];
    }
    if let Some(values) = dates(items, count, system) {
        return values;
    }
    if let Some(values) = numbers(items, count) {
        return values;
    }
    if let Some(values) = names(items, count) {
        return values;
    }
    if let Some(values) = numbered_text(items, count) {
        return values;
    }
    (0..count)
        .map(|i| items[i % items.len()].value.clone())
        .collect()
}

fn all_numbers(items: &[SeriesItem]) -> Option<Vec<f64>> {
    items.iter().map(|item| item.value.as_number()).collect()
}

/// two or more numbers: the least squares line through them, as excel does
fn numbers(items: &[SeriesItem], count: usize) -> Option<Vec<Value>> {
    let values = all_numbers(items)?;
    if values.len() < 2 {
        return None;
    }
    let n = values.len() as f64;
    let mean_x = (n - 1.0) / 2.0;
    let mean_y = values.iter().sum::<f64>() / n;
    let spread: f64 = (0..values.len()).map(|i| (i as f64 - mean_x).powi(2)).sum();
    let slope = (0..values.len())
        .map(|i| (i as f64 - mean_x) * (values[i] - mean_y))
        .sum::<f64>()
        / spread;
    let start = mean_y - slope * mean_x;
    Some(
        (0..count)
            .map(|i| {
                let x = n + i as f64;
                Value::Number(crate::calc::value::round_significant(start + slope * x))
            })
            .collect(),
    )
}

/// dates: one date steps a day; two step by their difference, in months
/// when both fall on the same day of the month, in years when also in
/// the same month
fn dates(items: &[SeriesItem], count: usize, system: DateSystem) -> Option<Vec<Value>> {
    if !items.iter().all(|item| item.is_date) {
        return None;
    }
    let serials = all_numbers(items)?;
    let last = *serials.last()?;
    if serials.len() == 1 {
        return Some(
            (1..=count)
                .map(|i| Value::Number(last + i as f64))
                .collect(),
        );
    }
    let (y1, m1, d1) = date_from_serial(serials[serials.len() - 2], system);
    let (y2, m2, d2) = date_from_serial(last, system);
    if d1 == d2 && (y1, m1) != (y2, m2) {
        let step = (y2 - y1) * 12 + i64::from(m2) - i64::from(m1);
        let values = (1..=count as i64)
            .map(|i| {
                let total = y2 * 12 + i64::from(m2) - 1 + step * i;
                let (year, month) = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
                let day = d2.min(days_in_month(year, month));
                Value::Number(serial_from_date(
                    year,
                    i64::from(month),
                    i64::from(day),
                    system,
                ))
            })
            .collect();
        return Some(values);
    }
    let step = last - serials[serials.len() - 2];
    Some(
        (1..=count)
            .map(|i| Value::Number(last + step * i as f64))
            .collect(),
    )
}

/// the case a name was written in, applied to another name
fn same_case(example: &str, word: &str) -> String {
    if example.chars().all(|c| !c.is_lowercase()) {
        word.to_uppercase()
    } else if example.chars().next().is_some_and(char::is_uppercase) {
        let mut chars = word.chars();
        chars
            .next()
            .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
            .unwrap_or_default()
    } else {
        word.to_string()
    }
}

/// day and month names, full or three letters, and quarters "Q1".."Q4"
fn names(items: &[SeriesItem], count: usize) -> Option<Vec<Value>> {
    let Value::Text(last) = &items.last()?.value else {
        return None;
    };
    let lower = last.to_lowercase();
    let short = lower.len() == 3;
    for list in [&DAYS[..], &MONTHS[..]] {
        let position = list.iter().position(|name| {
            if short {
                name.starts_with(&lower)
            } else {
                **name == lower
            }
        });
        if let Some(position) = position {
            let step = name_step(items, list, short).unwrap_or(1);
            let values = (1..=count as i64)
                .map(|i| {
                    let index = (position as i64 + step * i).rem_euclid(list.len() as i64) as usize;
                    let name = if short {
                        &list[index][..3]
                    } else {
                        list[index]
                    };
                    Value::Text(same_case(last, name))
                })
                .collect();
            return Some(values);
        }
    }
    let quarter = lower
        .strip_prefix('q')
        .and_then(|n| n.parse::<i64>().ok())
        .filter(|n| (1..=4).contains(n))?;
    let prefix = &last[..1];
    Some(
        (1..=count as i64)
            .map(|i| Value::Text(format!("{prefix}{}", (quarter - 1 + i).rem_euclid(4) + 1)))
            .collect(),
    )
}

/// the step between the last two names, when there are two
fn name_step(items: &[SeriesItem], list: &[&str], short: bool) -> Option<i64> {
    let index_of = |item: &SeriesItem| match &item.value {
        Value::Text(text) => {
            let lower = text.to_lowercase();
            list.iter().position(|name| {
                if short {
                    name.starts_with(&lower)
                } else {
                    **name == lower
                }
            })
        }
        _ => None,
    };
    let len = items.len();
    if len < 2 {
        return None;
    }
    let (a, b) = (index_of(&items[len - 2])?, index_of(&items[len - 1])?);
    Some((b as i64 - a as i64).rem_euclid(list.len() as i64))
}

/// "Item 1", "Week 3", "A1": text ending in a number counts on
fn numbered_text(items: &[SeriesItem], count: usize) -> Option<Vec<Value>> {
    let split = |item: &SeriesItem| -> Option<(String, i64, usize)> {
        let Value::Text(text) = &item.value else {
            return None;
        };
        let digits = text.chars().rev().take_while(char::is_ascii_digit).count();
        if digits == 0 || digits == text.len() {
            return None;
        }
        let (stem, number) = text.split_at(text.len() - digits);
        Some((stem.to_string(), number.parse().ok()?, digits))
    };
    let (stem, last, width) = split(items.last()?)?;
    let step = match items.len() {
        1 => 1,
        len => {
            let (other_stem, before, _) = split(&items[len - 2])?;
            if other_stem != stem {
                return None;
            }
            last - before
        }
    };
    Some(
        (1..=count as i64)
            .map(|i| {
                Value::Text(format!(
                    "{stem}{:0width$}",
                    (last + step * i).max(0),
                    width = width
                ))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(values: &[Value]) -> Vec<SeriesItem> {
        values
            .iter()
            .map(|value| SeriesItem {
                value: value.clone(),
                is_date: false,
            })
            .collect()
    }

    fn texts(values: Vec<Value>) -> Vec<String> {
        values
            .into_iter()
            .map(|v| match v {
                Value::Text(t) => t,
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn series_like_excel() {
        let system = DateSystem::Excel1900;
        let numbers = extend(&plain(&[Value::Number(1.0), Value::Number(3.0)]), 3, system);
        assert_eq!(
            numbers,
            vec![Value::Number(5.0), Value::Number(7.0), Value::Number(9.0)]
        );
        // excel fits a line: 1, 2, 4 continues 5.17, 6.67
        let fitted = extend(
            &plain(&[Value::Number(1.0), Value::Number(2.0), Value::Number(4.0)]),
            1,
            system,
        );
        assert!((fitted[0].as_number().unwrap() - 5.333_333_333_333_33).abs() < 1e-9);
        assert_eq!(
            extend(&plain(&[Value::Number(7.0)]), 2, system),
            vec![Value::Number(7.0); 2]
        );
        assert_eq!(
            texts(extend(&plain(&[Value::text("Mon")]), 2, system)),
            vec!["Tue", "Wed"]
        );
        assert_eq!(
            texts(extend(&plain(&[Value::text("DECEMBER")]), 1, system)),
            vec!["JANUARY"]
        );
        assert_eq!(
            texts(extend(&plain(&[Value::text("Item 9")]), 2, system)),
            vec!["Item 10", "Item 11"]
        );
        assert_eq!(
            texts(extend(&plain(&[Value::text("Q3")]), 3, system)),
            vec!["Q4", "Q1", "Q2"]
        );
        assert_eq!(
            texts(extend(
                &plain(&[Value::text("a"), Value::text("b")]),
                3,
                system
            )),
            vec!["a", "b", "a"]
        );
        let jan31 = SeriesItem {
            value: Value::Number(serial_from_date(2024, 1, 31, system)),
            is_date: true,
        };
        let feb29 = SeriesItem {
            value: Value::Number(serial_from_date(2024, 2, 29, system)),
            is_date: true,
        };
        let monthly = extend(
            &[
                jan31.clone(),
                SeriesItem {
                    value: Value::Number(serial_from_date(2024, 2, 29, system) + 2.0),
                    is_date: true,
                },
            ],
            1,
            system,
        );
        assert_eq!(monthly.len(), 1);
        assert_eq!(
            extend(&[feb29], 1, system),
            vec![Value::Number(serial_from_date(2024, 3, 1, system))]
        );
        let first = SeriesItem {
            value: Value::Number(serial_from_date(2024, 1, 15, system)),
            is_date: true,
        };
        let second = SeriesItem {
            value: Value::Number(serial_from_date(2024, 2, 15, system)),
            is_date: true,
        };
        assert_eq!(
            extend(&[first, second], 1, system),
            vec![Value::Number(serial_from_date(2024, 3, 15, system))]
        );
    }
}
