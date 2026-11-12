//! information about values: the IS functions, NA, TYPE, N and ERROR.TYPE.
//! these never pass an error on: ISERROR(1/0) is TRUE, not #DIV/0!.

use crate::formula::Expr;
use crate::model::{ErrorValue, Value};

use super::super::eval::Evaluator;
use super::super::value::{to_number, CalcValue};
use super::args::*;

pub const NAMES: &[&str] = &[
    "ISNUMBER",
    "ISTEXT",
    "ISNONTEXT",
    "ISBLANK",
    "ISERROR",
    "ISERR",
    "ISNA",
    "ISLOGICAL",
    "ISEVEN",
    "ISODD",
    "ISREF",
    "ISFORMULA",
    "NA",
    "TYPE",
    "N",
    "ERROR.TYPE",
];

pub fn call(ev: &Evaluator, name: &str, args: &[Expr]) -> Option<CalcValue> {
    let test = |check: fn(&Value) -> bool| lift(ev, args, &move |v| Value::Bool(check(&v[0])));
    Some(match name {
        "ISNUMBER" => test(|v| matches!(v, Value::Number(_))),
        "ISTEXT" => test(|v| matches!(v, Value::Text(_))),
        "ISNONTEXT" => test(|v| !matches!(v, Value::Text(_))),
        "ISBLANK" => test(Value::is_blank),
        "ISERROR" => test(|v| matches!(v, Value::Error(_))),
        "ISERR" => test(|v| matches!(v, Value::Error(e) if *e != ErrorValue::NA)),
        "ISNA" => test(|v| matches!(v, Value::Error(ErrorValue::NA))),
        "ISLOGICAL" => test(|v| matches!(v, Value::Bool(_))),
        "ISEVEN" | "ISODD" => parity(ev, args, name == "ISEVEN"),
        "ISREF" => CalcValue::boolean(
            args.first()
                .is_some_and(|a| matches!(ev.eval(a), CalcValue::Ref(_))),
        ),
        "ISFORMULA" => done(is_formula(ev, args)),
        "NA" => CalcValue::error(ErrorValue::NA),
        "TYPE" => CalcValue::number(type_number(ev, args)),
        "N" => lift(ev, args, &|v| match &v[0] {
            Value::Number(n) => Value::Number(*n),
            Value::Bool(b) => Value::Number(if *b { 1.0 } else { 0.0 }),
            Value::Error(e) => Value::Error(*e),
            _ => Value::Number(0.0),
        }),
        "ERROR.TYPE" => lift(ev, args, &|v| match &v[0] {
            Value::Error(e) => Value::Number(f64::from(e.code())),
            _ => Value::Error(ErrorValue::NA),
        }),
        _ => return None,
    })
}

/// ISEVEN and ISODD look at the whole part; text that is not a number is #VALUE!
fn parity(ev: &Evaluator, args: &[Expr], even: bool) -> CalcValue {
    let system = ev.system();
    lift(ev, args, &move |v| match to_number(&v[0], system) {
        Ok(n) => Value::Bool((n.trunc() as i64 % 2 == 0) == even),
        Err(e) => Value::Error(e),
    })
}

fn is_formula(ev: &Evaluator, args: &[Expr]) -> Outcome<Value> {
    arity(args, 1, 1)?;
    let area = area(ev, &args[0])?;
    let start = area.range.start;
    let has = ev.ctx.book.sheets[area.sheet]
        .cells
        .get(start.row, start.col)
        .is_some_and(|c| c.formula.is_some());
    Ok(Value::Bool(has))
}

/// TYPE: 1 number, 2 text, 4 logical, 16 error, 64 array
fn type_number(ev: &Evaluator, args: &[Expr]) -> f64 {
    let Some(arg) = args.first() else { return 16.0 };
    match ev.operand(ev.eval(arg)) {
        CalcValue::Array(_) => 64.0,
        other => match ev.intersect(other) {
            Value::Number(_) | Value::Blank => 1.0,
            Value::Text(_) => 2.0,
            Value::Bool(_) => 4.0,
            Value::Error(_) => 16.0,
        },
    }
}
