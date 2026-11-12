//! reading function arguments the way excel does: single values with
//! coercion, lists of numbers for aggregates (text in ranges is skipped,
//! text typed directly is read as a number), whole ranges as grids, and
//! "lifting" a one-value function over arrays.

use crate::formula::Expr;
use crate::model::{ErrorValue, Value};

use super::super::eval::Evaluator;
use super::super::value::*;

pub type Outcome<T> = Result<T, ErrorValue>;

pub fn number(ev: &Evaluator, expr: &Expr) -> Outcome<f64> {
    to_number(&ev.scalar(expr), ev.system())
}

/// a number cut to a whole number toward zero, as excel's integer
/// arguments are
pub fn integer(ev: &Evaluator, expr: &Expr) -> Outcome<i64> {
    let n = number(ev, expr)?;
    if n.abs() > 9.0e15 {
        return Err(ErrorValue::Num);
    }
    Ok(n.trunc() as i64)
}

pub fn text(ev: &Evaluator, expr: &Expr) -> Outcome<String> {
    to_text(&ev.scalar(expr))
}

pub fn boolean(ev: &Evaluator, expr: &Expr) -> Outcome<bool> {
    to_bool(&ev.scalar(expr))
}

/// argument `index`, or `default` when it is left out
pub fn number_or(ev: &Evaluator, args: &[Expr], index: usize, default: f64) -> Outcome<f64> {
    match args.get(index) {
        None | Some(Expr::Missing) => Ok(default),
        Some(expr) => number(ev, expr),
    }
}

pub fn integer_or(ev: &Evaluator, args: &[Expr], index: usize, default: i64) -> Outcome<i64> {
    match args.get(index) {
        None | Some(Expr::Missing) => Ok(default),
        Some(expr) => integer(ev, expr),
    }
}

pub fn boolean_or(ev: &Evaluator, args: &[Expr], index: usize, default: bool) -> Outcome<bool> {
    match args.get(index) {
        None | Some(Expr::Missing) => Ok(default),
        Some(expr) => boolean(ev, expr),
    }
}

/// the numbers of SUM-like functions: numbers in ranges and arrays
/// (text, logical values and blanks there are skipped), and typed
/// arguments coerced (TRUE is 1, "3" is 3, "a" is #VALUE!). the first
/// error met is the result.
pub fn numbers(ev: &Evaluator, args: &[Expr]) -> Outcome<Vec<f64>> {
    let mut out = Vec::new();
    for arg in args {
        match ev.eval(arg) {
            CalcValue::Ref(areas) => {
                for area in areas {
                    numbers_in_area(ev, area, &mut out)?;
                }
            }
            CalcValue::Array(array) => {
                for item in &array.items {
                    match item {
                        Value::Number(n) => out.push(*n),
                        Value::Error(e) => return Err(*e),
                        _ => {}
                    }
                }
            }
            CalcValue::Scalar(Value::Blank) if matches!(arg, Expr::Missing) => {}
            CalcValue::Scalar(value) => out.push(to_number(&value, ev.system())?),
        }
    }
    Ok(out)
}

fn numbers_in_area(ev: &Evaluator, area: Area, out: &mut Vec<f64>) -> Outcome<()> {
    let mut error = None;
    ev.for_each_value(area, &mut |_, _, value| match value {
        Value::Number(n) => out.push(*n),
        Value::Error(e) if error.is_none() => error = Some(*e),
        _ => {}
    });
    match error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// every value of an argument as a grid: ranges and arrays as they are,
/// a single value as a 1 by 1 grid
pub fn grid(ev: &Evaluator, expr: &Expr) -> Outcome<Array> {
    match ev.eval(expr) {
        CalcValue::Ref(areas) if areas.len() == 1 => Ok(ev.area_array(areas[0])),
        CalcValue::Ref(_) => Err(ErrorValue::Value),
        CalcValue::Array(array) => Ok(array),
        CalcValue::Scalar(value) => Ok(Array::new(1, 1, vec![value])),
    }
}

/// an argument that has to be a reference (SUMIF's ranges, ROW, OFFSET)
pub fn area(ev: &Evaluator, expr: &Expr) -> Outcome<Area> {
    match ev.eval(expr) {
        CalcValue::Ref(areas) if areas.len() == 1 => Ok(areas[0]),
        CalcValue::Scalar(Value::Error(e)) => Err(e),
        _ => Err(ErrorValue::Value),
    }
}

/// applies a function of single values to its arguments, item by item
/// when any argument is an array or a range (excel 365 lifting). files
/// without dynamic arrays take the cell in line with the formula instead.
pub fn lift(ev: &Evaluator, args: &[Expr], f: &dyn Fn(&[Value]) -> Value) -> CalcValue {
    let mut values: Vec<CalcValue> = Vec::with_capacity(args.len());
    for arg in args {
        let value = ev.eval(arg);
        let value = if ev.legacy {
            CalcValue::Scalar(ev.intersect(value))
        } else {
            ev.operand(value)
        };
        values.push(value);
    }
    if values.iter().all(|v| matches!(v, CalcValue::Scalar(_))) {
        let scalars: Vec<Value> = values.into_iter().map(|v| ev.intersect(v)).collect();
        return CalcValue::Scalar(f(&scalars));
    }
    let arrays: Vec<Array> = values
        .into_iter()
        .map(|v| match v {
            CalcValue::Array(array) => array,
            other => Array::new(1, 1, vec![ev.intersect(other)]),
        })
        .collect();
    let rows = arrays.iter().map(|a| a.rows).max().unwrap_or(1);
    let cols = arrays.iter().map(|a| a.cols).max().unwrap_or(1);
    let mut items = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let row: Vec<Value> = arrays.iter().map(|a| a.broadcast(r, c)).collect();
            items.push(f(&row));
        }
    }
    CalcValue::Array(Array::new(rows, cols, items))
}

/// a one-number function lifted: errors and text that is not a number
/// give their error, results that are not finite give #NUM!
pub fn lift_number(ev: &Evaluator, args: &[Expr], f: &dyn Fn(&[f64]) -> Outcome<f64>) -> CalcValue {
    let system = ev.system();
    lift(ev, args, &|values| {
        let numbers: Outcome<Vec<f64>> = values.iter().map(|v| to_number(v, system)).collect();
        match numbers.and_then(|n| f(&n)) {
            Ok(n) => number_or_error(n),
            Err(e) => Value::Error(e),
        }
    })
}

/// the result of a function that gives one value or fails
pub fn done(result: Outcome<Value>) -> CalcValue {
    match result {
        Ok(value) => CalcValue::Scalar(value),
        Err(e) => CalcValue::error(e),
    }
}

pub fn done_number(result: Outcome<f64>) -> CalcValue {
    match result {
        Ok(n) => CalcValue::number(n),
        Err(e) => CalcValue::error(e),
    }
}

/// argument count checks: excel refuses such formulas as they are typed;
/// a file that has one anyway gets #VALUE!
pub fn arity(args: &[Expr], min: usize, max: usize) -> Outcome<()> {
    if args.len() < min || args.len() > max {
        Err(ErrorValue::Value)
    } else {
        Ok(())
    }
}
