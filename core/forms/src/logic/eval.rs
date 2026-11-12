//! running a script from `script.rs` with strict limits.
//!
//! values behave the way form scripts expect them to: a field holding
//! "12" reads as the number 12, an empty field as empty text that counts
//! as 0 in arithmetic, and + joins text when either side is text. every
//! step is counted, and a script that takes too many is stopped.

use super::script::{BinaryOp, Expr, Function, Stmt, UnaryOp};

/// the most steps one script may take. a calculation over a few dozen
/// fields takes well under a hundred.
pub const MAX_STEPS: usize = 20_000;

/// a value while a script runs.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
    Bool(bool),
}

impl Value {
    /// reads a field's stored text the way form scripts see it: text that
    /// looks like a number is a number.
    pub fn from_field_text(text: &str) -> Value {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Value::Text(String::new());
        }
        match parse_number(trimmed) {
            Some(number) => Value::Number(number),
            None => Value::Text(text.to_owned()),
        }
    }

    pub fn to_number(&self) -> f64 {
        match self {
            Value::Number(value) => *value,
            Value::Bool(true) => 1.0,
            Value::Bool(false) => 0.0,
            Value::Text(text) if text.trim().is_empty() => 0.0,
            Value::Text(text) => parse_number(text.trim()).unwrap_or(f64::NAN),
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Number(value) => *value != 0.0 && !value.is_nan(),
            Value::Bool(value) => *value,
            Value::Text(text) => !text.is_empty(),
        }
    }

    /// the text stored in the field when a script sets this value.
    pub fn to_field_text(&self) -> String {
        match self {
            Value::Number(value) if value.is_nan() => "NaN".to_owned(),
            Value::Number(value) if value.is_infinite() => if *value > 0.0 {
                "Infinity"
            } else {
                "-Infinity"
            }
            .to_owned(),
            Value::Number(value) => number_to_text(*value),
            Value::Bool(value) => value.to_string(),
            Value::Text(text) => text.clone(),
        }
    }
}

/// a plain number: optional sign, digits, one decimal point, optional exponent.
pub fn parse_number(text: &str) -> Option<f64> {
    let valid = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))
        && text.chars().any(|c| c.is_ascii_digit());
    if !valid {
        return None;
    }
    text.parse().ok().filter(|value: &f64| value.is_finite())
}

/// a number as text without the float noise sums leave behind
/// (0.1 + 0.2 shows as 0.3, as form apps show it).
pub fn number_to_text(value: f64) -> String {
    if value == 0.0 {
        return "0".to_owned();
    }
    let rounded = format!("{:.10}", value);
    let trimmed = rounded.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// what a script can read: other fields' values.
pub trait FieldReader {
    fn value_of(&self, field: &str) -> Option<String>;
}

/// why a script stopped without finishing.
#[derive(Clone, Debug, PartialEq)]
pub enum RunError {
    /// it reads a field the form doesn't have
    UnknownField(String),
    TooManySteps,
}

/// what a script left behind.
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    pub value: Value,
    /// false when the script refused the answer (event.rc = false)
    pub accepted: bool,
    /// the messages it showed, in order
    pub alerts: Vec<String>,
}

struct Machine<'a> {
    fields: &'a dyn FieldReader,
    this_value: Value,
    accepted: bool,
    alerts: Vec<String>,
    locals: Vec<(String, Value)>,
    steps: usize,
}

/// runs a script. `this_value` is event.value before it starts.
pub fn run(
    statements: &[Stmt],
    this_value: Value,
    fields: &dyn FieldReader,
) -> Result<Outcome, RunError> {
    let mut machine = Machine {
        fields,
        this_value,
        accepted: true,
        alerts: Vec::new(),
        locals: Vec::new(),
        steps: 0,
    };
    machine.block(statements)?;
    Ok(Outcome {
        value: machine.this_value,
        accepted: machine.accepted,
        alerts: machine.alerts,
    })
}

/// evaluates one expression on its own.
pub fn evaluate(
    expr: &Expr,
    this_value: Value,
    fields: &dyn FieldReader,
) -> Result<Value, RunError> {
    let mut machine = Machine {
        fields,
        this_value,
        accepted: true,
        alerts: Vec::new(),
        locals: Vec::new(),
        steps: 0,
    };
    machine.expr(expr)
}

impl Machine<'_> {
    fn step(&mut self) -> Result<(), RunError> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            Err(RunError::TooManySteps)
        } else {
            Ok(())
        }
    }

    fn block(&mut self, statements: &[Stmt]) -> Result<(), RunError> {
        for statement in statements {
            self.step()?;
            match statement {
                Stmt::SetValue(expr) => self.this_value = self.expr(expr)?,
                Stmt::SetAccepted(expr) => self.accepted = self.expr(expr)?.truthy(),
                Stmt::Assign(name, expr) => {
                    let value = self.expr(expr)?;
                    match self.locals.iter_mut().find(|(own, _)| own == name) {
                        Some(slot) => slot.1 = value,
                        None => self.locals.push((name.clone(), value)),
                    }
                }
                Stmt::If(condition, then, otherwise) => {
                    if self.expr(condition)?.truthy() {
                        self.block(then)?;
                    } else {
                        self.block(otherwise)?;
                    }
                }
                Stmt::Alert(expr) => {
                    let message = self.expr(expr)?.to_field_text();
                    self.alerts.push(message);
                }
            }
        }
        Ok(())
    }

    fn expr(&mut self, expr: &Expr) -> Result<Value, RunError> {
        self.step()?;
        Ok(match expr {
            Expr::Number(value) => Value::Number(*value),
            Expr::Text(text) => Value::Text(text.clone()),
            Expr::Bool(value) => Value::Bool(*value),
            Expr::ThisValue => self.this_value.clone(),
            Expr::Local(name) => self
                .locals
                .iter()
                .find(|(own, _)| own == name)
                .map(|(_, value)| value.clone())
                .unwrap_or(Value::Text(String::new())),
            Expr::Field(name) => {
                let text = self
                    .fields
                    .value_of(name)
                    .ok_or_else(|| RunError::UnknownField(name.clone()))?;
                Value::from_field_text(&text)
            }
            Expr::Unary(op, inner) => {
                let inner = self.expr(inner)?;
                match op {
                    UnaryOp::Negate => Value::Number(-inner.to_number()),
                    UnaryOp::Plus => Value::Number(inner.to_number()),
                    UnaryOp::Not => Value::Bool(!inner.truthy()),
                }
            }
            Expr::Binary(op, left, right) => self.binary(*op, left, right)?,
            Expr::Ternary(condition, then, otherwise) => {
                if self.expr(condition)?.truthy() {
                    self.expr(then)?
                } else {
                    self.expr(otherwise)?
                }
            }
            Expr::Call(function, arguments) => {
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    values.push(self.expr(argument)?);
                }
                call(*function, &values)
            }
        })
    }

    fn binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> Result<Value, RunError> {
        // and / or stop early, as in javascript, and give back a side
        if op == BinaryOp::And || op == BinaryOp::Or {
            let first = self.expr(left)?;
            let decided = if op == BinaryOp::And {
                !first.truthy()
            } else {
                first.truthy()
            };
            return if decided { Ok(first) } else { self.expr(right) };
        }
        let a = self.expr(left)?;
        let b = self.expr(right)?;
        Ok(match op {
            BinaryOp::Add => match (&a, &b) {
                (Value::Text(x), _) => Value::Text(format!("{x}{}", b.to_field_text())),
                (_, Value::Text(y)) => Value::Text(format!("{}{y}", a.to_field_text())),
                _ => Value::Number(a.to_number() + b.to_number()),
            },
            BinaryOp::Subtract => Value::Number(a.to_number() - b.to_number()),
            BinaryOp::Multiply => Value::Number(a.to_number() * b.to_number()),
            BinaryOp::Divide => Value::Number(a.to_number() / b.to_number()),
            BinaryOp::Remainder => Value::Number(a.to_number() % b.to_number()),
            BinaryOp::Equal => Value::Bool(loosely_equal(&a, &b)),
            BinaryOp::NotEqual => Value::Bool(!loosely_equal(&a, &b)),
            BinaryOp::Less
            | BinaryOp::LessOrEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterOrEqual => Value::Bool(compare(op, &a, &b)),
            BinaryOp::And | BinaryOp::Or => unreachable!("handled above"),
        })
    }
}

fn loosely_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Text(x), Value::Text(y)) => x == y,
        _ => a.to_number() == b.to_number(),
    }
}

fn compare(op: BinaryOp, a: &Value, b: &Value) -> bool {
    let ordering = match (a, b) {
        (Value::Text(x), Value::Text(y)) => Some(x.cmp(y)),
        _ => a.to_number().partial_cmp(&b.to_number()),
    };
    let Some(ordering) = ordering else {
        return false;
    };
    match op {
        BinaryOp::Less => ordering.is_lt(),
        BinaryOp::LessOrEqual => ordering.is_le(),
        BinaryOp::Greater => ordering.is_gt(),
        _ => ordering.is_ge(),
    }
}

fn call(function: Function, values: &[Value]) -> Value {
    let first = || values.first().map(Value::to_number).unwrap_or(f64::NAN);
    match function {
        // javascript rounds halves up, towards positive infinity
        Function::Round => Value::Number((first() + 0.5).floor()),
        Function::Floor => Value::Number(first().floor()),
        Function::Ceil => Value::Number(first().ceil()),
        Function::Abs => Value::Number(first().abs()),
        Function::Min => Value::Number(
            values
                .iter()
                .map(Value::to_number)
                .fold(f64::INFINITY, f64::min),
        ),
        Function::Max => Value::Number(
            values
                .iter()
                .map(Value::to_number)
                .fold(f64::NEG_INFINITY, f64::max),
        ),
        Function::ToNumber => Value::Number(first()),
        // AFMakeNumber reads "1,5" as 1.5 and gives null (0 in sums) for text
        Function::MakeNumber => {
            let value = values.first().cloned().unwrap_or(Value::Number(0.0));
            match value {
                Value::Text(text) => {
                    let cleaned = text.trim().replace(',', ".");
                    Value::Number(parse_number(&cleaned).unwrap_or(0.0))
                }
                other => Value::Number(other.to_number()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::script::{parse_notation, parse_script};
    use super::*;
    use std::collections::HashMap;

    struct Fields(HashMap<&'static str, &'static str>);

    impl FieldReader for Fields {
        fn value_of(&self, field: &str) -> Option<String> {
            self.0.get(field).map(|value| (*value).to_owned())
        }
    }

    fn fields() -> Fields {
        Fields(HashMap::from([
            ("a", "12"),
            ("b", "2.5"),
            ("name", "Ada"),
            ("empty", ""),
        ]))
    }

    fn value(source: &str) -> Value {
        let expr = super::super::script::as_numbers(&parse_notation(source).unwrap());
        evaluate(&expr, Value::Text(String::new()), &fields()).unwrap()
    }

    #[test]
    fn arithmetic_follows_the_usual_rules() {
        assert_eq!(value("a * b"), Value::Number(30.0));
        assert_eq!(value("a + b * 2"), Value::Number(17.0));
        assert_eq!(value("(a + b) * 2"), Value::Number(29.0));
        assert_eq!(value("a * 15%"), Value::Number(1.7999999999999998));
        assert_eq!(value("empty + 1"), Value::Number(1.0));
        assert_eq!(value("round(b)"), Value::Number(3.0));
        assert_eq!(value("max(a, b, 40)"), Value::Number(40.0));
        assert_eq!(value("a > b"), Value::Bool(true));
    }

    #[test]
    fn javascript_text_joins_and_ternaries() {
        let script = parse_script(
            r#"event.value = this.getField("name").value + " owes " + (this.getField("a").value > 10 ? "a lot" : "little");"#,
        )
        .unwrap();
        let outcome = run(&script, Value::Text(String::new()), &fields()).unwrap();
        assert_eq!(outcome.value, Value::Text("Ada owes a lot".into()));
    }

    #[test]
    fn refusals_carry_their_message() {
        let script = parse_script(
            r#"if (event.value > this.getField("a").value) { app.alert("At most " + this.getField("a").value + "."); event.rc = false; }"#,
        )
        .unwrap();
        let outcome = run(&script, Value::Number(20.0), &fields()).unwrap();
        assert!(!outcome.accepted);
        assert_eq!(outcome.alerts, vec!["At most 12.".to_owned()]);
        let fine = run(&script, Value::Number(3.0), &fields()).unwrap();
        assert!(fine.accepted);
    }

    #[test]
    fn missing_fields_and_long_runs_stop_the_script() {
        let script = parse_script(r#"event.value = this.getField("nope").value;"#).unwrap();
        assert_eq!(
            run(&script, Value::Number(0.0), &fields()),
            Err(RunError::UnknownField("nope".into()))
        );
        let long = format!("event.value = {};", vec!["1"; 250].join(" + "));
        let script = parse_script(&long).unwrap();
        let outcome = run(&script, Value::Number(0.0), &fields()).unwrap();
        assert_eq!(outcome.value, Value::Number(250.0));
    }

    #[test]
    fn float_noise_is_hidden() {
        assert_eq!(number_to_text(0.1 + 0.2), "0.3");
        assert_eq!(number_to_text(-0.0), "0");
        assert_eq!(number_to_text(1234.5), "1234.5");
        assert_eq!(number_to_text(3.0), "3");
    }
}
