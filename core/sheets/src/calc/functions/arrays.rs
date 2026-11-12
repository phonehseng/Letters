//! excel 365's dynamic array functions that make whole lists: FILTER,
//! SORT, UNIQUE and SEQUENCE. their results spill into the cells below
//! and to the right.

use std::cmp::Ordering;

use crate::formula::Expr;
use crate::model::{ErrorValue, Value};

use super::super::eval::Evaluator;
use super::super::value::{compare, to_bool, Array, CalcValue};
use super::args::*;

pub const NAMES: &[&str] = &["FILTER", "SORT", "UNIQUE", "SEQUENCE"];

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    let result = match name {
        "FILTER" => filter(ev, args),
        "SORT" => sort(ev, args),
        "UNIQUE" => unique(ev, args),
        "SEQUENCE" => sequence(ev, args).map(CalcValue::Array),
        _ => return None,
    };
    Some(result.unwrap_or_else(CalcValue::error))
}

fn rows_of(array: &Array) -> Vec<Vec<Value>> {
    array.rows_iter().map(|row| row.to_vec()).collect()
}

fn from_rows(rows: Vec<Vec<Value>>) -> Array {
    let cols = rows.first().map(Vec::len).unwrap_or(0);
    let count = rows.len();
    Array::new(count, cols, rows.into_iter().flatten().collect())
}

/// FILTER(array, include, [if_empty]): keeps the rows (or columns) whose
/// include value is true
fn filter(ev: &Evaluator, args: &[Expr]) -> Outcome<CalcValue> {
    arity(args, 2, 3)?;
    let data = grid(ev, &args[0])?;
    let include = grid(ev, &args[1])?;
    let by_rows = include.cols == 1 && include.rows == data.rows;
    let by_cols = include.rows == 1 && include.cols == data.cols && !by_rows;
    if !by_rows && !by_cols {
        return Err(ErrorValue::Value);
    }
    let keep: Vec<bool> = include.items.iter().map(to_bool).collect::<Outcome<_>>()?;
    let source = if by_rows { data } else { data.transpose() };
    let kept: Vec<Vec<Value>> = rows_of(&source)
        .into_iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(row, _)| row)
        .collect();
    if kept.is_empty() {
        return match args.get(2) {
            Some(expr) => Ok(ev.eval(expr)),
            // excel's #CALC! for an empty result
            None => Err(ErrorValue::Calc),
        };
    }
    let result = from_rows(kept);
    Ok(CalcValue::Array(if by_rows {
        result
    } else {
        result.transpose()
    }))
}

/// SORT(array, [sort_index], [sort_order], [by_col]): a stable sort, as
/// excel's is, in excel's order: numbers, then text, then logical values
fn sort(ev: &Evaluator, args: &[Expr]) -> Outcome<CalcValue> {
    arity(args, 1, 4)?;
    let data = grid(ev, &args[0])?;
    let index = integer_or(ev, args, 1, 1)?;
    let order = integer_or(ev, args, 2, 1)?;
    let by_col = boolean_or(ev, args, 3, false)?;
    if order != 1 && order != -1 {
        return Err(ErrorValue::Value);
    }
    let source = if by_col { data.transpose() } else { data };
    if index < 1 || index as usize > source.cols {
        return Err(ErrorValue::Value);
    }
    let key = index as usize - 1;
    let mut rows = rows_of(&source);
    rows.sort_by(|a, b| sort_order(&a[key], &b[key], order == -1));
    let sorted = from_rows(rows);
    Ok(CalcValue::Array(if by_col {
        sorted.transpose()
    } else {
        sorted
    }))
}

/// excel sorts blanks last in both directions and errors after logical values
pub fn sort_order(a: &Value, b: &Value, descending: bool) -> Ordering {
    match (a.is_blank(), b.is_blank()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    let order = match (a, b) {
        (Value::Error(_), Value::Error(_)) => Ordering::Equal,
        (Value::Error(_), _) => Ordering::Greater,
        (_, Value::Error(_)) => Ordering::Less,
        _ => compare(a, b),
    };
    if descending {
        order.reverse()
    } else {
        order
    }
}

/// UNIQUE(array, [by_col], [exactly_once])
fn unique(ev: &Evaluator, args: &[Expr]) -> Outcome<CalcValue> {
    arity(args, 1, 3)?;
    let data = grid(ev, &args[0])?;
    let by_col = boolean_or(ev, args, 1, false)?;
    let exactly_once = boolean_or(ev, args, 2, false)?;
    let source = if by_col { data.transpose() } else { data };
    let rows = rows_of(&source);
    let same = |a: &Vec<Value>, b: &Vec<Value>| {
        a.iter().zip(b).all(|(x, y)| {
            compare(x, y) == Ordering::Equal
                && std::mem::discriminant(x) == std::mem::discriminant(y)
        })
    };
    let mut kept: Vec<Vec<Value>> = Vec::new();
    for row in &rows {
        let times = rows.iter().filter(|other| same(row, other)).count();
        let already = kept.iter().any(|k| same(k, row));
        if !already && (!exactly_once || times == 1) {
            kept.push(row.clone());
        }
    }
    if kept.is_empty() {
        return Err(ErrorValue::Calc);
    }
    let result = from_rows(kept);
    Ok(CalcValue::Array(if by_col {
        result.transpose()
    } else {
        result
    }))
}

/// SEQUENCE(rows, [columns], [start], [step])
fn sequence(ev: &Evaluator, args: &[Expr]) -> Outcome<Array> {
    arity(args, 1, 4)?;
    let rows = integer(ev, &args[0])?;
    let cols = integer_or(ev, args, 1, 1)?;
    let start = number_or(ev, args, 2, 1.0)?;
    let step = number_or(ev, args, 3, 1.0)?;
    if rows < 1 || cols < 1 {
        return Err(ErrorValue::Calc);
    }
    // excel's limit is the grid itself
    if rows * cols > i64::from(crate::address::MAX_ROWS) * 16 {
        return Err(ErrorValue::Num);
    }
    let count = (rows * cols) as usize;
    let items = (0..count)
        .map(|i| Value::Number(start + step * i as f64))
        .collect();
    Ok(Array::new(rows as usize, cols as usize, items))
}
