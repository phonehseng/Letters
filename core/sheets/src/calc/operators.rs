//! the arithmetic, text and comparison operators on two single values,
//! with excel's coercions: text that reads as a number takes part in
//! arithmetic, an error on the left wins over one on the right.

use std::cmp::Ordering;

use crate::formula::BinaryOp;
use crate::model::{DateSystem, ErrorValue, Value};

use super::value::{compare, number_or_error, to_number, to_text};

pub fn apply(op: BinaryOp, a: &Value, b: &Value, system: DateSystem) -> Value {
    if let Value::Error(e) = a {
        return Value::Error(*e);
    }
    // arithmetic and joining read the left side first, so "abc"+1/0 is
    // #VALUE! (the text fails) before the right side's #DIV/0! is seen
    // (checked against excel; libreoffice gives #DIV/0!)
    let reads_left_first = !matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    );
    if reads_left_first {
        let left_fails = match op {
            BinaryOp::Concat => to_text(a).err(),
            _ => to_number(a, system).err(),
        };
        if let Some(e) = left_fails {
            return Value::Error(e);
        }
    }
    if let Value::Error(e) = b {
        return Value::Error(*e);
    }
    match op {
        BinaryOp::Concat => match (to_text(a), to_text(b)) {
            (Ok(x), Ok(y)) => Value::Text(x + &y),
            (Err(e), _) | (_, Err(e)) => Value::Error(e),
        },
        BinaryOp::Equal => Value::Bool(compare(a, b) == Ordering::Equal),
        BinaryOp::NotEqual => Value::Bool(compare(a, b) != Ordering::Equal),
        BinaryOp::Less => Value::Bool(compare(a, b) == Ordering::Less),
        BinaryOp::LessEqual => Value::Bool(compare(a, b) != Ordering::Greater),
        BinaryOp::Greater => Value::Bool(compare(a, b) == Ordering::Greater),
        BinaryOp::GreaterEqual => Value::Bool(compare(a, b) != Ordering::Less),
        _ => arithmetic(op, a, b, system),
    }
}

fn arithmetic(op: BinaryOp, a: &Value, b: &Value, system: DateSystem) -> Value {
    let (x, y) = match (to_number(a, system), to_number(b, system)) {
        (Ok(x), Ok(y)) => (x, y),
        (Err(e), _) | (_, Err(e)) => return Value::Error(e),
    };
    match op {
        BinaryOp::Add => number_or_error(x + y),
        BinaryOp::Subtract => number_or_error(x - y),
        BinaryOp::Multiply => number_or_error(x * y),
        BinaryOp::Divide if y == 0.0 => Value::Error(ErrorValue::Div0),
        BinaryOp::Divide => number_or_error(x / y),
        BinaryOp::Power => power(x, y),
        _ => Value::Error(ErrorValue::Value),
    }
}

/// excel: 0^0 and 0^-1 are #NUM! and #DIV/0!; a negative number to a
/// fractional power is #NUM!
pub fn power(x: f64, y: f64) -> Value {
    if x == 0.0 && y == 0.0 {
        return Value::Error(ErrorValue::Num);
    }
    if x == 0.0 && y < 0.0 {
        return Value::Error(ErrorValue::Div0);
    }
    number_or_error(x.powf(y))
}

/// excel sets the result of a formula's last addition or subtraction to
/// zero when it is only rounding noise next to its operands, so
/// =0.5-0.4-0.1 shows 0 while =1*(0.5-0.4-0.1) shows -2.78E-17 (documented
/// by microsoft for excel 97 and later)
pub fn snap_to_zero(result: f64, a: f64, b: f64) -> f64 {
    let scale = a.abs().max(b.abs());
    // 2^-49 relative: a few units in the last place of the operands
    if result != 0.0 && result.abs() < scale * 1.776_356_839_400_250_5e-15 {
        0.0
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(op: BinaryOp, a: Value, b: Value) -> Value {
        apply(op, &a, &b, DateSystem::Excel1900)
    }

    #[test]
    fn coercion_and_errors() {
        assert_eq!(
            run(BinaryOp::Add, Value::text("2"), Value::Bool(true)),
            Value::Number(3.0)
        );
        assert_eq!(
            run(BinaryOp::Add, Value::text("a"), Value::Number(1.0)),
            Value::Error(ErrorValue::Value)
        );
        assert_eq!(
            run(BinaryOp::Divide, Value::Number(1.0), Value::Blank),
            Value::Error(ErrorValue::Div0)
        );
        assert_eq!(
            run(
                BinaryOp::Add,
                Value::Error(ErrorValue::NA),
                Value::Error(ErrorValue::Div0)
            ),
            Value::Error(ErrorValue::NA)
        );
        assert_eq!(
            run(BinaryOp::Concat, Value::Number(1.5), Value::Bool(false)),
            Value::text("1.5FALSE")
        );
        assert_eq!(
            run(BinaryOp::Power, Value::Number(0.0), Value::Number(0.0)),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            run(
                BinaryOp::Power,
                Value::Number(-8.0),
                Value::Number(1.0 / 3.0)
            ),
            Value::Error(ErrorValue::Num)
        );
        assert_eq!(
            run(BinaryOp::Equal, Value::text("A"), Value::text("a")),
            Value::Bool(true)
        );
        assert_eq!(snap_to_zero(0.5 - 0.4 - 0.1, 0.5 - 0.4, 0.1), 0.0);
    }
}
