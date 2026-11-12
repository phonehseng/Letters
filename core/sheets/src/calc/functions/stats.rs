//! statistics: averages, counts, extremes, median and mode, spread
//! (standard deviation and variance), ranks, percentiles and quartiles,
//! correlation, the k-th largest and smallest.

use crate::formula::Expr;
use crate::model::{ErrorValue, Value};

use super::super::eval::Evaluator;
use super::super::value::{compare_numbers, CalcValue};
use super::args::*;

pub const NAMES: &[&str] = &[
    "AVERAGE",
    "AVERAGEA",
    "COUNT",
    "COUNTA",
    "COUNTBLANK",
    "MIN",
    "MAX",
    "MINA",
    "MAXA",
    "MEDIAN",
    "MODE",
    "MODE.SNGL",
    "STDEV",
    "STDEV.S",
    "STDEV.P",
    "STDEVP",
    "VAR",
    "VAR.S",
    "VAR.P",
    "VARP",
    "RANK",
    "RANK.EQ",
    "RANK.AVG",
    "PERCENTILE",
    "PERCENTILE.INC",
    "PERCENTILE.EXC",
    "QUARTILE",
    "QUARTILE.INC",
    "QUARTILE.EXC",
    "CORREL",
    "PEARSON",
    "LARGE",
    "SMALL",
    "GEOMEAN",
    "AVEDEV",
    "DEVSQ",
];

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    let over = |f: fn(&[f64]) -> Outcome<f64>| done_number(numbers(ev, args).and_then(|n| f(&n)));
    Some(match name {
        "AVERAGE" => over(average),
        "AVERAGEA" => done_number(numbers_a(ev, args).and_then(|n| average(&n))),
        "COUNT" => CalcValue::number(count(ev, args) as f64),
        "COUNTA" => CalcValue::number(count_all(ev, args) as f64),
        "COUNTBLANK" => done_number(count_blank(ev, args)),
        "MIN" => over(|n| {
            Ok(empty_is_zero(
                n.iter().copied().fold(f64::INFINITY, f64::min),
            ))
        }),
        "MAX" => over(|n| {
            Ok(empty_is_zero(
                n.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            ))
        }),
        "MINA" => done_number(
            numbers_a(ev, args)
                .map(|n| empty_is_zero(n.iter().copied().fold(f64::INFINITY, f64::min))),
        ),
        "MAXA" => done_number(
            numbers_a(ev, args)
                .map(|n| empty_is_zero(n.iter().copied().fold(f64::NEG_INFINITY, f64::max))),
        ),
        "MEDIAN" => over(median),
        "MODE" | "MODE.SNGL" => done_number(numbers_only(ev, args).and_then(|n| mode(&n))),
        "STDEV" | "STDEV.S" => over(|n| variance(n, true).map(f64::sqrt)),
        "STDEV.P" | "STDEVP" => over(|n| variance(n, false).map(f64::sqrt)),
        "VAR" | "VAR.S" => over(|n| variance(n, true)),
        "VAR.P" | "VARP" => over(|n| variance(n, false)),
        "DEVSQ" => over(|n| {
            if n.is_empty() {
                Err(ErrorValue::Num)
            } else {
                Ok(squared_deviations(n))
            }
        }),
        "AVEDEV" => over(average_deviation),
        "GEOMEAN" => over(geometric_mean),
        "RANK" | "RANK.EQ" | "RANK.AVG" => done_number(rank(ev, args, name == "RANK.AVG")),
        "PERCENTILE" | "PERCENTILE.INC" => done_number(percentile_of(ev, args, 1.0, false)),
        "PERCENTILE.EXC" => done_number(percentile_of(ev, args, 1.0, true)),
        "QUARTILE" | "QUARTILE.INC" => done_number(percentile_of(ev, args, 4.0, false)),
        "QUARTILE.EXC" => done_number(percentile_of(ev, args, 4.0, true)),
        "CORREL" | "PEARSON" => done_number(correlation(ev, args)),
        "LARGE" | "SMALL" => done_number(kth(ev, args, name == "LARGE")),
        _ => return None,
    })
}

/// MODE skips typed text and logical values too: MODE(1,2,"3",TRUE) is
/// #N/A in excel, not 1
fn numbers_only(ev: &Evaluator, args: &[Expr]) -> Outcome<Vec<f64>> {
    let kept: Vec<Expr> = args
        .iter()
        .filter(|arg| !matches!(arg, Expr::Text { .. } | Expr::Bool { .. }))
        .cloned()
        .collect();
    numbers(ev, &kept)
}

/// MIN and MAX of nothing are 0 in excel
fn empty_is_zero(n: f64) -> f64 {
    if n.is_finite() {
        n
    } else {
        0.0
    }
}

fn average(n: &[f64]) -> Outcome<f64> {
    if n.is_empty() {
        return Err(ErrorValue::Div0);
    }
    Ok(n.iter().sum::<f64>() / n.len() as f64)
}

/// numbers for the "A" functions: logical values in ranges count (TRUE
/// is 1) and text in ranges counts as 0
fn numbers_a(ev: &Evaluator, args: &[Expr]) -> Outcome<Vec<f64>> {
    let mut out = Vec::new();
    for arg in args {
        match ev.eval(arg) {
            CalcValue::Ref(areas) => {
                for area in areas {
                    let mut error = None;
                    ev.for_each_value(area, &mut |_, _, value| match value {
                        Value::Number(n) => out.push(*n),
                        Value::Bool(b) => out.push(if *b { 1.0 } else { 0.0 }),
                        Value::Text(_) => out.push(0.0),
                        Value::Error(e) => error = error.or(Some(*e)),
                        Value::Blank => {}
                    });
                    if let Some(e) = error {
                        return Err(e);
                    }
                }
            }
            other => out.extend(numbers(ev, std::slice::from_ref(&value_expr(other)))?),
        }
    }
    Ok(out)
}

/// a calculated value back as an expression, to reuse `numbers`
fn value_expr(value: CalcValue) -> Expr {
    match value {
        CalcValue::Scalar(Value::Number(n)) => Expr::number(n),
        CalcValue::Scalar(Value::Text(t)) => Expr::Text { value: t },
        CalcValue::Scalar(Value::Bool(b)) => Expr::Bool { value: b },
        CalcValue::Scalar(Value::Error(e)) => Expr::Error { value: e },
        CalcValue::Array(array) => Expr::Array {
            rows: array
                .rows_iter()
                .map(|row| {
                    row.iter()
                        .map(|v| value_expr(CalcValue::Scalar(v.clone())))
                        .collect()
                })
                .collect(),
        },
        _ => Expr::Missing,
    }
}

/// COUNT: numbers in ranges and arrays; typed arguments that read as
/// numbers (including TRUE and "3"). errors are not counted, never raised.
fn count(ev: &Evaluator, args: &[Expr]) -> usize {
    let mut total = 0;
    for arg in args {
        match ev.eval(arg) {
            CalcValue::Ref(areas) => {
                for area in areas {
                    ev.for_each_value(area, &mut |_, _, value| {
                        total += usize::from(matches!(value, Value::Number(_)))
                    });
                }
            }
            CalcValue::Array(array) => {
                total += array
                    .items
                    .iter()
                    .filter(|v| matches!(v, Value::Number(_)))
                    .count()
            }
            CalcValue::Scalar(value) => {
                let counts = match &value {
                    Value::Number(_) | Value::Bool(_) => true,
                    Value::Text(text) => {
                        super::super::text_numbers::parse_number(text, ev.system()).is_some()
                    }
                    _ => false,
                };
                total += usize::from(counts && !matches!(arg, Expr::Missing));
            }
        }
    }
    total
}

/// COUNTA: everything that is not empty, errors included
fn count_all(ev: &Evaluator, args: &[Expr]) -> usize {
    let mut total = 0;
    for arg in args {
        match ev.eval(arg) {
            CalcValue::Ref(areas) => {
                for area in areas {
                    ev.for_each_value(area, &mut |_, _, _| total += 1);
                }
            }
            CalcValue::Array(array) => {
                total += array.items.iter().filter(|v| !v.is_blank()).count()
            }
            CalcValue::Scalar(_) if matches!(arg, Expr::Missing) => {}
            CalcValue::Scalar(_) => total += 1,
        }
    }
    total
}

/// COUNTBLANK: empty cells and cells showing "" in one range
fn count_blank(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    arity(args, 1, 1)?;
    let area = area(ev, &args[0])?;
    let mut filled = 0u64;
    ev.for_each_value(area, &mut |_, _, value| {
        if !matches!(value, Value::Text(t) if t.is_empty()) {
            filled += 1;
        }
    });
    Ok((area.range.cell_count() - filled) as f64)
}

fn sorted(n: &[f64]) -> Vec<f64> {
    let mut sorted = n.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    sorted
}

fn median(n: &[f64]) -> Outcome<f64> {
    if n.is_empty() {
        return Err(ErrorValue::Num);
    }
    let s = sorted(n);
    let middle = s.len() / 2;
    Ok(if s.len().is_multiple_of(2) {
        (s[middle - 1] + s[middle]) / 2.0
    } else {
        s[middle]
    })
}

/// the most frequent value; ties go to the one that appears first, and
/// no repeated value is #N/A (as in excel)
fn mode(n: &[f64]) -> Outcome<f64> {
    let mut best: Option<(f64, usize)> = None;
    for (index, value) in n.iter().enumerate() {
        let seen_before = n[..index].iter().any(|v| v == value);
        if seen_before {
            continue;
        }
        let times = n.iter().filter(|v| *v == value).count();
        if times > 1 && best.is_none_or(|(_, most)| times > most) {
            best = Some((*value, times));
        }
    }
    best.map(|(value, _)| value).ok_or(ErrorValue::NA)
}

fn squared_deviations(n: &[f64]) -> f64 {
    let mean = n.iter().sum::<f64>() / n.len().max(1) as f64;
    n.iter().map(|x| (x - mean).powi(2)).sum()
}

fn variance(n: &[f64], sample: bool) -> Outcome<f64> {
    let divisor = if sample {
        n.len() as f64 - 1.0
    } else {
        n.len() as f64
    };
    if divisor <= 0.0 {
        return Err(ErrorValue::Div0);
    }
    Ok(squared_deviations(n) / divisor)
}

fn average_deviation(n: &[f64]) -> Outcome<f64> {
    let mean = average(n).map_err(|_| ErrorValue::Num)?;
    Ok(n.iter().map(|x| (x - mean).abs()).sum::<f64>() / n.len() as f64)
}

fn geometric_mean(n: &[f64]) -> Outcome<f64> {
    if n.is_empty() || n.iter().any(|x| *x <= 0.0) {
        return Err(ErrorValue::Num);
    }
    Ok((n.iter().map(|x| x.ln()).sum::<f64>() / n.len() as f64).exp())
}

/// RANK(number, range, order): 1 for the largest unless order is set;
/// RANK.AVG gives tied numbers the average of their places
fn rank(ev: &Evaluator, args: &[Expr], average_ties: bool) -> Outcome<f64> {
    arity(args, 2, 3)?;
    let target = number(ev, &args[0])?;
    let list = numbers(ev, &args[1..2])?;
    let ascending = number_or(ev, args, 2, 0.0)? != 0.0;
    let better = list
        .iter()
        .filter(|x| {
            if ascending {
                compare_numbers(**x, target).is_lt()
            } else {
                compare_numbers(**x, target).is_gt()
            }
        })
        .count();
    let ties = list
        .iter()
        .filter(|x| compare_numbers(**x, target).is_eq())
        .count();
    if ties == 0 {
        return Err(ErrorValue::NA);
    }
    let first_place = better as f64 + 1.0;
    Ok(if average_ties {
        first_place + (ties as f64 - 1.0) / 2.0
    } else {
        first_place
    })
}

/// PERCENTILE and QUARTILE (`scale` 4 for quartiles), inclusive or
/// exclusive interpolation as excel defines them
fn percentile_of(ev: &Evaluator, args: &[Expr], scale: f64, exclusive: bool) -> Outcome<f64> {
    arity(args, 2, 2)?;
    let list = sorted(&numbers(ev, &args[..1])?);
    let mut k = number(ev, &args[1])?;
    if scale == 4.0 {
        k = k.trunc();
        if !(0.0..=4.0).contains(&k) || (exclusive && (k == 0.0 || k == 4.0)) {
            return Err(ErrorValue::Num);
        }
        k /= 4.0;
    }
    percentile(&list, k, exclusive)
}

pub fn percentile(sorted: &[f64], k: f64, exclusive: bool) -> Outcome<f64> {
    let n = sorted.len() as f64;
    if sorted.is_empty() || !(0.0..=1.0).contains(&k) {
        return Err(ErrorValue::Num);
    }
    let position = if exclusive {
        k * (n + 1.0) - 1.0
    } else {
        k * (n - 1.0)
    };
    if position < 0.0 || position > n - 1.0 {
        return Err(ErrorValue::Num);
    }
    let low = position.floor() as usize;
    let fraction = position - low as f64;
    let high = (low + 1).min(sorted.len() - 1);
    Ok(sorted[low] + fraction * (sorted[high] - sorted[low]))
}

/// pairs of numbers from two ranges of the same size; pairs where either
/// side is not a number are left out
fn number_pairs(ev: &Evaluator, args: &[Expr]) -> Outcome<Vec<(f64, f64)>> {
    arity(args, 2, 2)?;
    let (a, b) = (grid(ev, &args[0])?, grid(ev, &args[1])?);
    if a.items.len() != b.items.len() {
        return Err(ErrorValue::NA);
    }
    let mut pairs = Vec::new();
    for (x, y) in a.items.iter().zip(&b.items) {
        match (x, y) {
            (Value::Error(e), _) | (_, Value::Error(e)) => return Err(*e),
            (Value::Number(x), Value::Number(y)) => pairs.push((*x, *y)),
            _ => {}
        }
    }
    Ok(pairs)
}

fn correlation(ev: &Evaluator, args: &[Expr]) -> Outcome<f64> {
    let pairs = number_pairs(ev, args)?;
    let n = pairs.len() as f64;
    if pairs.len() < 2 {
        return Err(ErrorValue::Div0);
    }
    let mean_x = pairs.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = pairs.iter().map(|p| p.1).sum::<f64>() / n;
    let covariance: f64 = pairs.iter().map(|(x, y)| (x - mean_x) * (y - mean_y)).sum();
    let spread_x: f64 = pairs.iter().map(|(x, _)| (x - mean_x).powi(2)).sum();
    let spread_y: f64 = pairs.iter().map(|(_, y)| (y - mean_y).powi(2)).sum();
    if spread_x == 0.0 || spread_y == 0.0 {
        return Err(ErrorValue::Div0);
    }
    Ok(covariance / (spread_x * spread_y).sqrt())
}

fn kth(ev: &Evaluator, args: &[Expr], largest: bool) -> Outcome<f64> {
    arity(args, 2, 2)?;
    let list = sorted(&numbers(ev, &args[..1])?);
    let k = number(ev, &args[1])?.ceil();
    if k < 1.0 || k > list.len() as f64 {
        return Err(ErrorValue::Num);
    }
    let index = k as usize - 1;
    Ok(if largest {
        list[list.len() - 1 - index]
    } else {
        list[index]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statistics_of_a_small_list() {
        let n = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        assert_eq!(average(&n), Ok(5.0));
        assert_eq!(median(&n), Ok(4.5));
        assert_eq!(mode(&n), Ok(4.0));
        assert_eq!(variance(&n, false), Ok(4.0));
        assert!((variance(&n, true).unwrap() - 4.571_428_571_428_571).abs() < 1e-12);
        assert_eq!(percentile(&sorted(&n), 0.25, false), Ok(4.0));
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0], 0.5, true), Ok(2.5));
        assert_eq!(mode(&[1.0, 2.0]), Err(ErrorValue::NA));
        assert_eq!(average(&[]), Err(ErrorValue::Div0));
    }
}
