//! counting, summing and averaging the cells that meet conditions:
//! COUNTIF(S), SUMIF(S), AVERAGEIF(S), MAXIFS and MINIFS.

use std::borrow::Cow;

use crate::address::Range;
use crate::formula::Expr;
use crate::model::{ErrorValue, Value};

use super::super::criteria::Criterion;
use super::super::eval::Evaluator;
use super::super::value::{Area, Array, CalcValue};
use super::args::*;

pub const NAMES: &[&str] = &[
    "COUNTIF",
    "COUNTIFS",
    "SUMIF",
    "SUMIFS",
    "AVERAGEIF",
    "AVERAGEIFS",
    "MAXIFS",
    "MINIFS",
];

#[derive(Clone, Copy)]
enum Aggregate {
    Count,
    Sum,
    Average,
    Max,
    Min,
}

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    Some(match name {
        "COUNTIF" => single(ev, args, Aggregate::Count),
        "SUMIF" => single(ev, args, Aggregate::Sum),
        "AVERAGEIF" => single(ev, args, Aggregate::Average),
        "COUNTIFS" => multiple(ev, args, None, Aggregate::Count),
        "SUMIFS" => multiple(
            ev,
            args.get(1..).unwrap_or(&[]),
            args.first(),
            Aggregate::Sum,
        ),
        "AVERAGEIFS" => multiple(
            ev,
            args.get(1..).unwrap_or(&[]),
            args.first(),
            Aggregate::Average,
        ),
        "MAXIFS" => multiple(
            ev,
            args.get(1..).unwrap_or(&[]),
            args.first(),
            Aggregate::Max,
        ),
        "MINIFS" => multiple(
            ev,
            args.get(1..).unwrap_or(&[]),
            args.first(),
            Aggregate::Min,
        ),
        _ => return None,
    })
}

/// COUNTIF(range, condition), SUMIF(range, condition, [sum range])
fn single(ev: &Evaluator, args: &[Expr], aggregate: Aggregate) -> CalcValue {
    if let Err(e) = arity(args, 2, 3) {
        return CalcValue::error(e);
    }
    let pairs = [(&args[0], &args[1])];
    let target = args.get(2).or(if matches!(aggregate, Aggregate::Count) {
        None
    } else {
        Some(&args[0])
    });
    evaluate(ev, &pairs, target, aggregate, true)
}

/// COUNTIFS(range1, condition1, ...), SUMIFS(sum range, range1, condition1, ...)
fn multiple(
    ev: &Evaluator,
    pairs: &[Expr],
    target: Option<&Expr>,
    aggregate: Aggregate,
) -> CalcValue {
    if pairs.is_empty() || !pairs.len().is_multiple_of(2) {
        return CalcValue::error(ErrorValue::Value);
    }
    let pairs: Vec<(&Expr, &Expr)> = pairs.chunks(2).map(|pair| (&pair[0], &pair[1])).collect();
    evaluate(ev, &pairs, target, aggregate, false)
}

/// the condition argument may itself be a range (excel 365 then gives
/// one result per condition)
fn evaluate(
    ev: &Evaluator,
    pairs: &[(&Expr, &Expr)],
    target: Option<&Expr>,
    aggregate: Aggregate,
    resize: bool,
) -> CalcValue {
    let conditions: Vec<CalcValue> = pairs.iter().map(|(_, c)| ev.operand(ev.eval(c))).collect();
    let array_shape = conditions.iter().find_map(|c| match c {
        CalcValue::Array(a) if !ev.legacy => Some((a.rows, a.cols)),
        _ => None,
    });
    let Some((rows, cols)) = array_shape else {
        let values: Vec<Value> = conditions.into_iter().map(|c| ev.intersect(c)).collect();
        return done(run(ev, pairs, &values, target, aggregate, resize));
    };
    let mut items = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let values: Vec<Value> = conditions
                .iter()
                .map(|condition| match condition {
                    CalcValue::Array(a) => a.broadcast(r, c),
                    other => ev.intersect(other.clone()),
                })
                .collect();
            items.push(
                run(ev, pairs, &values, target, aggregate, resize).unwrap_or_else(Value::Error),
            );
        }
    }
    CalcValue::Array(Array::new(rows, cols, items))
}

/// a range's values, borrowed from the workbook where possible
struct Values<'a> {
    items: Vec<Cow<'a, Value>>,
    rows: usize,
    cols: usize,
}

/// a range's values; SUMIF's sum range takes the shape of the condition
/// range from its own top-left cell, as excel does
fn values_of<'a>(
    ev: &Evaluator<'_, 'a>,
    expr: &Expr,
    shape: Option<Range>,
) -> Outcome<(Values<'a>, Option<Area>)> {
    match ev.eval(expr) {
        CalcValue::Ref(areas) if areas.len() == 1 => {
            let mut area = ev.trimmed(areas[0]);
            if let Some(shape) = shape {
                area.range = Range::from_bounds(
                    area.range.start.row,
                    area.range.start.col,
                    area.range.start.row + shape.rows() - 1,
                    area.range.start.col + shape.cols() - 1,
                );
            }
            let (items, rows, cols) = ev.area_values(area);
            Ok((Values { items, rows, cols }, Some(area)))
        }
        CalcValue::Array(array) => {
            let items = array.items.into_iter().map(Cow::Owned).collect();
            Ok((
                Values {
                    items,
                    rows: array.rows,
                    cols: array.cols,
                },
                None,
            ))
        }
        CalcValue::Scalar(Value::Error(e)) => Err(e),
        _ => Err(ErrorValue::Value),
    }
}

fn run(
    ev: &Evaluator,
    pairs: &[(&Expr, &Expr)],
    conditions: &[Value],
    target: Option<&Expr>,
    aggregate: Aggregate,
    resize: bool,
) -> Outcome<Value> {
    let system = ev.system();
    let (first, first_area) = values_of(ev, pairs[0].0, None)?;
    let mut ranges = vec![first];
    for (range, _) in &pairs[1..] {
        let (values, _) = values_of(ev, range, first_area.map(|a| a.range))?;
        ranges.push(values);
    }
    let size = (ranges[0].rows, ranges[0].cols);
    if ranges.iter().any(|r| (r.rows, r.cols) != size) {
        return Err(ErrorValue::Value);
    }
    let target_values = match target {
        Some(expr) => {
            let shape = if resize {
                first_area.map(|a| a.range)
            } else {
                None
            };
            let (values, _) = values_of(ev, expr, shape.or(first_area.map(|a| a.range)))?;
            if (values.rows, values.cols) != size {
                return Err(ErrorValue::Value);
            }
            Some(values)
        }
        None => None,
    };
    let criteria: Vec<Criterion> = conditions
        .iter()
        .map(|value| Criterion::new(value, system))
        .collect();
    let mut matched: Vec<f64> = Vec::new();
    let mut count = 0usize;
    for index in 0..ranges[0].items.len() {
        let all = criteria
            .iter()
            .zip(&ranges)
            .all(|(criterion, range)| criterion.matches(range.items[index].as_ref(), system));
        if !all {
            continue;
        }
        count += 1;
        if let Some(values) = &target_values {
            match values.items[index].as_ref() {
                Value::Number(n) => matched.push(*n),
                Value::Error(e) if !matches!(aggregate, Aggregate::Count) => return Err(*e),
                _ => {}
            }
        }
    }
    Ok(finish(aggregate, count, &matched))
}

fn finish(aggregate: Aggregate, count: usize, matched: &[f64]) -> Value {
    match aggregate {
        Aggregate::Count => Value::Number(count as f64),
        Aggregate::Sum => Value::Number(matched.iter().sum()),
        Aggregate::Average if matched.is_empty() => Value::Error(ErrorValue::Div0),
        Aggregate::Average => Value::Number(matched.iter().sum::<f64>() / matched.len() as f64),
        Aggregate::Max => Value::Number(extreme(matched, f64::max)),
        Aggregate::Min => Value::Number(extreme(matched, f64::min)),
    }
}

/// MAXIFS and MINIFS of nothing are 0, as in excel
fn extreme(values: &[f64], pick: fn(f64, f64) -> f64) -> f64 {
    values.iter().copied().reduce(pick).unwrap_or(0.0)
}
