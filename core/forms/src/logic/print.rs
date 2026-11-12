//! writing a syntax tree out again: as javascript other pdf apps run, or
//! in letters notation the way a person types it. brackets are added only
//! where the order of operations needs them.

use std::fmt::Write as _;

use super::script::{Expr, Function, UnaryOp};

fn precedence_of(expr: &Expr) -> u8 {
    match expr {
        Expr::Ternary(..) => 0,
        Expr::Binary(op, ..) => op.precedence(),
        Expr::Unary(..) => 7,
        _ => 8,
    }
}

/// a number written the shortest way that reads back the same.
pub fn number_text(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// a field name that can be written bare in letters notation.
fn is_plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .is_some_and(|first| first.is_alphabetic() || first == '_')
                && chars.all(|c| c.is_alphanumeric() || c == '_')
        })
        && Function::from_notation(name).is_none()
        && !["value", "and", "or", "not"].contains(&name.to_ascii_lowercase().as_str())
}

fn print(expr: &Expr, javascript: bool, out: &mut String) {
    let child = |inner: &Expr, minimum: u8, out: &mut String| {
        if precedence_of(inner) < minimum {
            out.push('(');
            print(inner, javascript, out);
            out.push(')');
        } else {
            print(inner, javascript, out);
        }
    };
    match expr {
        Expr::Number(value) if *value < 0.0 => {
            let _ = write!(out, "({})", number_text(*value));
        }
        Expr::Number(value) => out.push_str(&number_text(*value)),
        Expr::Text(text) => out.push_str(&quoted(text)),
        Expr::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        Expr::Field(name) if javascript => {
            let _ = write!(out, "AFMakeNumber(this.getField({}).value)", quoted(name));
        }
        Expr::Field(name) if is_plain_name(name) => out.push_str(name),
        Expr::Field(name) => out.push_str(&quoted(name)),
        Expr::ThisValue if javascript => out.push_str("AFMakeNumber(event.value)"),
        Expr::ThisValue => out.push_str("value"),
        Expr::Local(name) => out.push_str(name),
        Expr::Unary(op, inner) => {
            out.push_str(match (op, javascript) {
                (UnaryOp::Negate, _) => "-",
                (UnaryOp::Plus, _) => "+",
                (UnaryOp::Not, true) => "!",
                (UnaryOp::Not, false) => "not ",
            });
            child(inner, 7, out);
        }
        Expr::Binary(op, left, right) => {
            let precedence = op.precedence();
            child(left, precedence, out);
            let _ = write!(out, " {} ", op.symbol(javascript));
            // left associative: an equal-precedence right side needs brackets
            child(right, precedence + 1, out);
        }
        Expr::Ternary(condition, then, otherwise) => {
            child(condition, 1, out);
            out.push_str(" ? ");
            child(then, 1, out);
            out.push_str(" : ");
            child(otherwise, 0, out);
        }
        Expr::Call(function, arguments) => {
            out.push_str(if javascript {
                function.javascript_name()
            } else {
                function.notation_name()
            });
            out.push('(');
            for (index, argument) in arguments.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                print(argument, javascript, out);
            }
            out.push(')');
        }
    }
}

/// an expression written as javascript that other pdf apps run the same way.
pub fn to_javascript(expr: &Expr) -> String {
    let mut out = String::new();
    print(expr, true, &mut out);
    out
}

/// an expression written in letters notation, the way a person types it.
pub fn to_notation(expr: &Expr) -> String {
    let mut out = String::new();
    print(expr, false, &mut out);
    out
}
