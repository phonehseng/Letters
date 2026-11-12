//! logical functions: IF and IFS, AND, OR, XOR, NOT, IFERROR, IFNA,
//! SWITCH, TRUE and FALSE. IF evaluates only the branch it takes, so
//! =IF(A1=0,0,1/A1) never divides by zero.

use std::cmp::Ordering;

use crate::formula::Expr;
use crate::model::{ErrorValue, Value};

use super::super::eval::Evaluator;
use super::super::value::{compare, to_bool, Array, CalcValue};
use super::args::*;

pub const NAMES: &[&str] = &[
    "IF", "IFS", "AND", "OR", "XOR", "NOT", "IFERROR", "IFNA", "SWITCH", "TRUE", "FALSE",
];

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    Some(match name {
        "IF" => if_function(ev, args),
        "IFS" => ifs(ev, args),
        "AND" => done(combine(ev, args, |values| values.iter().all(|b| *b))),
        "OR" => done(combine(ev, args, |values| values.iter().any(|b| *b))),
        "XOR" => done(combine(ev, args, |values| {
            values.iter().filter(|b| **b).count() % 2 == 1
        })),
        "NOT" => lift(ev, args, &|v| match to_bool(&v[0]) {
            Ok(b) => Value::Bool(!b),
            Err(e) => Value::Error(e),
        }),
        "IFERROR" => replace_errors(ev, args, |_| true),
        "IFNA" => replace_errors(ev, args, |e| e == ErrorValue::NA),
        "SWITCH" => switch(ev, args),
        "TRUE" => CalcValue::boolean(true),
        "FALSE" => CalcValue::boolean(false),
        _ => return None,
    })
}

/// a branch that was left out: IF(c,,1) gives 0 for the empty branch
fn branch(ev: &Evaluator, args: &[Expr], index: usize, missing: Value) -> CalcValue {
    match args.get(index) {
        None => CalcValue::Scalar(missing),
        Some(Expr::Missing) => CalcValue::Scalar(Value::Number(0.0)),
        Some(expr) => ev.eval(expr),
    }
}

fn if_function(ev: &Evaluator, args: &[Expr]) -> CalcValue {
    if let Err(e) = arity(args, 1, 3) {
        return CalcValue::error(e);
    }
    let condition = if ev.legacy {
        CalcValue::Scalar(ev.scalar(&args[0]))
    } else {
        ev.operand(ev.eval(&args[0]))
    };
    match condition {
        CalcValue::Array(conditions) => if_over_array(ev, args, &conditions),
        other => match to_bool(&ev.intersect(other)) {
            Ok(true) => branch(ev, args, 1, Value::Bool(true)),
            Ok(false) => branch(ev, args, 2, Value::Bool(false)),
            Err(e) => CalcValue::error(e),
        },
    }
}

/// IF(A1:A3>1, "big", "small") gives one answer per item
fn if_over_array(ev: &Evaluator, args: &[Expr], conditions: &Array) -> CalcValue {
    let as_array = |value: CalcValue| match ev.operand(value) {
        CalcValue::Array(array) => array,
        other => Array::new(1, 1, vec![ev.intersect(other)]),
    };
    let yes = as_array(branch(ev, args, 1, Value::Bool(true)));
    let no = as_array(branch(ev, args, 2, Value::Bool(false)));
    let rows = conditions.rows.max(yes.rows).max(no.rows);
    let cols = conditions.cols.max(yes.cols).max(no.cols);
    let mut items = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            items.push(match to_bool(&conditions.broadcast(r, c)) {
                Ok(true) => yes.broadcast(r, c),
                Ok(false) => no.broadcast(r, c),
                Err(e) => Value::Error(e),
            });
        }
    }
    CalcValue::Array(Array::new(rows, cols, items))
}

fn ifs(ev: &Evaluator, args: &[Expr]) -> CalcValue {
    if args.is_empty() || !args.len().is_multiple_of(2) {
        return CalcValue::error(ErrorValue::Value);
    }
    for pair in args.chunks(2) {
        match to_bool(&ev.scalar(&pair[0])) {
            Ok(true) => return ev.eval(&pair[1]),
            Ok(false) => {}
            Err(e) => return CalcValue::error(e),
        }
    }
    CalcValue::error(ErrorValue::NA)
}

/// the logical values AND, OR and XOR look at: numbers and logical
/// values in ranges (text and blanks skipped), typed arguments coerced
fn combine(ev: &Evaluator, args: &[Expr], decide: fn(&[bool]) -> bool) -> Outcome<Value> {
    let mut found = Vec::new();
    for arg in args {
        match ev.eval(arg) {
            CalcValue::Ref(areas) => {
                for area in areas {
                    let mut error = None;
                    ev.for_each_value(area, &mut |_, _, value| match value {
                        Value::Number(n) => found.push(*n != 0.0),
                        Value::Bool(b) => found.push(*b),
                        Value::Error(e) => error = error.or(Some(*e)),
                        _ => {}
                    });
                    if let Some(e) = error {
                        return Err(e);
                    }
                }
            }
            CalcValue::Array(array) => {
                for item in &array.items {
                    match item {
                        Value::Number(n) => found.push(*n != 0.0),
                        Value::Bool(b) => found.push(*b),
                        Value::Error(e) => return Err(*e),
                        _ => {}
                    }
                }
            }
            // typed text is passed over, as in a range: AND("abc",TRUE) is
            // TRUE in excel (libreoffice says #VALUE!)
            CalcValue::Scalar(Value::Text(_)) => {}
            CalcValue::Scalar(value) => found.push(to_bool(&value)?),
        }
    }
    if found.is_empty() {
        return Err(ErrorValue::Value);
    }
    Ok(Value::Bool(decide(&found)))
}

fn replace_errors(ev: &Evaluator, args: &[Expr], catches: fn(ErrorValue) -> bool) -> CalcValue {
    if let Err(e) = arity(args, 2, 2) {
        return CalcValue::error(e);
    }
    let value = if ev.legacy {
        CalcValue::Scalar(ev.scalar(&args[0]))
    } else {
        ev.operand(ev.eval(&args[0]))
    };
    let fallback = || match ev.operand(ev.eval(&args[1])) {
        CalcValue::Scalar(Value::Blank) => Value::Number(0.0),
        other => ev.intersect(other),
    };
    match value {
        CalcValue::Array(array) => {
            let replacement = fallback();
            let items = array
                .items
                .into_iter()
                .map(|item| match item {
                    Value::Error(e) if catches(e) => replacement.clone(),
                    other => other,
                })
                .collect();
            CalcValue::Array(Array::new(array.rows, array.cols, items))
        }
        CalcValue::Scalar(Value::Error(e)) if catches(e) => ev.eval(&args[1]),
        other => other,
    }
}

fn switch(ev: &Evaluator, args: &[Expr]) -> CalcValue {
    if args.len() < 3 {
        return CalcValue::error(ErrorValue::Value);
    }
    let subject = ev.scalar(&args[0]);
    if let Value::Error(e) = subject {
        return CalcValue::error(e);
    }
    let rest = &args[1..];
    for pair in rest.chunks(2) {
        if pair.len() == 1 {
            return ev.eval(&pair[0]);
        }
        let candidate = ev.scalar(&pair[0]);
        let same_kind = std::mem::discriminant(&candidate) == std::mem::discriminant(&subject);
        if same_kind && compare(&subject, &candidate) == Ordering::Equal {
            return ev.eval(&pair[1]);
        }
    }
    CalcValue::error(ErrorValue::NA)
}
