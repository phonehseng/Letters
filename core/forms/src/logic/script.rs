//! a tiny, safe language for form scripts. not a javascript engine.
//!
//! interactive pdf forms keep their calculations and checks as small
//! javascript programs. almost all real forms use a handful of shapes:
//! `event.value = this.getField("a").value * this.getField("b").value`,
//! a comparison, a ternary, Math.round. this module reads exactly those
//! shapes into a syntax tree and nothing else: no loops, no functions of
//! our own, no property access beyond the whitelisted forms below. anything
//! outside the subset is a parse error, and the caller leaves that script
//! alone and tells the person.
//!
//! the same tree is also written in "letters notation", the plain form a
//! person types in Letters: `price * quantity`, `"Line 1" + 5`,
//! `subtotal * 15%`, `value <= budget`.

pub use super::parse::{parse_notation, parse_script};
pub use super::print::{to_javascript, to_notation};

/// a value-producing part of a script.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Number(f64),
    Text(String),
    Bool(bool),
    /// the value of another field, by full name
    Field(String),
    /// the field the script belongs to (event.value)
    ThisValue,
    /// a variable declared with `var` earlier in the script
    Local(String),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Function, Vec<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Plus,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Equal,
    NotEqual,
    And,
    Or,
}

impl BinaryOp {
    /// how tightly the operator binds; higher binds first.
    pub(super) fn precedence(self) -> u8 {
        match self {
            BinaryOp::Or => 1,
            BinaryOp::And => 2,
            BinaryOp::Equal | BinaryOp::NotEqual => 3,
            BinaryOp::Less
            | BinaryOp::LessOrEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterOrEqual => 4,
            BinaryOp::Add | BinaryOp::Subtract => 5,
            BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Remainder => 6,
        }
    }

    pub(super) fn symbol(self, javascript: bool) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Subtract => "-",
            BinaryOp::Multiply => "*",
            BinaryOp::Divide => "/",
            BinaryOp::Remainder => "%",
            BinaryOp::Less => "<",
            BinaryOp::LessOrEqual => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterOrEqual => ">=",
            BinaryOp::Equal if javascript => "==",
            BinaryOp::Equal => "=",
            BinaryOp::NotEqual => "!=",
            BinaryOp::And if javascript => "&&",
            BinaryOp::And => "and",
            BinaryOp::Or if javascript => "||",
            BinaryOp::Or => "or",
        }
    }
}

/// the only functions a script may call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    Round,
    Floor,
    Ceil,
    Min,
    Max,
    Abs,
    /// Number(x)
    ToNumber,
    /// AFMakeNumber(x): like Number, but empty text and commas are handled
    MakeNumber,
}

impl Function {
    pub(super) fn javascript_name(self) -> &'static str {
        match self {
            Function::Round => "Math.round",
            Function::Floor => "Math.floor",
            Function::Ceil => "Math.ceil",
            Function::Min => "Math.min",
            Function::Max => "Math.max",
            Function::Abs => "Math.abs",
            Function::ToNumber => "Number",
            Function::MakeNumber => "AFMakeNumber",
        }
    }

    pub(super) fn notation_name(self) -> &'static str {
        match self {
            Function::Round => "round",
            Function::Floor => "floor",
            Function::Ceil => "ceil",
            Function::Min => "min",
            Function::Max => "max",
            Function::Abs => "abs",
            Function::ToNumber | Function::MakeNumber => "number",
        }
    }

    pub(super) fn from_notation(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "round" => Function::Round,
            "floor" => Function::Floor,
            "ceil" => Function::Ceil,
            "min" => Function::Min,
            "max" => Function::Max,
            "abs" => Function::Abs,
            "number" => Function::ToNumber,
            _ => return None,
        })
    }
}

/// one statement of a script.
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// event.value = ...
    SetValue(Expr),
    /// event.rc = ...: false refuses the answer
    SetAccepted(Expr),
    /// var name = ...; or name = ...;
    Assign(String, Expr),
    If(Expr, Vec<Stmt>, Vec<Stmt>),
    /// app.alert(...): the message shown when an answer is refused
    Alert(Expr),
}

/// why a script isn't in the subset Letters reads.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseError(pub String);

/// drops the AFMakeNumber(...) wrappers written around field references,
/// so a tree read back from a file equals the tree it was written from.
pub fn simplify(expr: &Expr) -> Expr {
    match expr {
        Expr::Call(Function::MakeNumber, arguments)
            if matches!(arguments.as_slice(), [Expr::Field(_)] | [Expr::ThisValue]) =>
        {
            arguments[0].clone()
        }
        Expr::Call(function, arguments) => {
            Expr::Call(*function, arguments.iter().map(simplify).collect())
        }
        Expr::Unary(op, inner) => Expr::Unary(*op, Box::new(simplify(inner))),
        Expr::Binary(op, left, right) => {
            Expr::Binary(*op, Box::new(simplify(left)), Box::new(simplify(right)))
        }
        Expr::Ternary(a, b, c) => Expr::Ternary(
            Box::new(simplify(a)),
            Box::new(simplify(b)),
            Box::new(simplify(c)),
        ),
        other => other.clone(),
    }
}

/// whether letters notation can say this expression: it has no text,
/// true/false or variables, which the notation has no way to write.
pub fn fits_notation(expr: &Expr) -> bool {
    match expr {
        Expr::Number(_) | Expr::Field(_) | Expr::ThisValue => true,
        Expr::Text(_) | Expr::Bool(_) | Expr::Local(_) => false,
        Expr::Unary(_, inner) => fits_notation(inner),
        Expr::Binary(_, left, right) => fits_notation(left) && fits_notation(right),
        Expr::Ternary(a, b, c) => fits_notation(a) && fits_notation(b) && fits_notation(c),
        Expr::Call(_, arguments) => arguments.iter().all(fits_notation),
    }
}

/// letters notation is about numbers, so every field it reads is read as
/// a number, the way the javascript it is written as does
/// (AFMakeNumber(...)): an empty field counts as 0.
pub fn as_numbers(expr: &Expr) -> Expr {
    match expr {
        Expr::Field(_) | Expr::ThisValue => Expr::Call(Function::MakeNumber, vec![expr.clone()]),
        Expr::Call(function, arguments) => {
            Expr::Call(*function, arguments.iter().map(as_numbers).collect())
        }
        Expr::Unary(op, inner) => Expr::Unary(*op, Box::new(as_numbers(inner))),
        Expr::Binary(op, left, right) => {
            Expr::Binary(*op, Box::new(as_numbers(left)), Box::new(as_numbers(right)))
        }
        Expr::Ternary(a, b, c) => Expr::Ternary(
            Box::new(as_numbers(a)),
            Box::new(as_numbers(b)),
            Box::new(as_numbers(c)),
        ),
        other => other.clone(),
    }
}

/// every field an expression reads, in first-seen order.
pub fn fields_in(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Field(name) => {
            if !out.contains(name) {
                out.push(name.clone());
            }
        }
        Expr::Unary(_, inner) => fields_in(inner, out),
        Expr::Binary(_, left, right) => {
            fields_in(left, out);
            fields_in(right, out);
        }
        Expr::Ternary(a, b, c) => {
            fields_in(a, out);
            fields_in(b, out);
            fields_in(c, out);
        }
        Expr::Call(_, arguments) => arguments
            .iter()
            .for_each(|argument| fields_in(argument, out)),
        _ => {}
    }
}

/// every field a script reads.
pub fn fields_in_statements(statements: &[Stmt], out: &mut Vec<String>) {
    for statement in statements {
        match statement {
            Stmt::SetValue(expr)
            | Stmt::SetAccepted(expr)
            | Stmt::Assign(_, expr)
            | Stmt::Alert(expr) => fields_in(expr, out),
            Stmt::If(condition, then, otherwise) => {
                fields_in(condition, out);
                fields_in_statements(then, out);
                fields_in_statements(otherwise, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_usual_calculation_script() {
        let script = r#"event.value = this.getField("price").value * this.getField("qty").value;"#;
        let parsed = parse_script(script).unwrap();
        assert_eq!(
            parsed,
            vec![Stmt::SetValue(Expr::Binary(
                BinaryOp::Multiply,
                Box::new(Expr::Field("price".into())),
                Box::new(Expr::Field("qty".into()))
            ))]
        );
    }

    #[test]
    fn notation_round_trips_through_javascript() {
        for source in [
            "price * quantity",
            "(a + b) * 15%",
            "\"Line 1\" + total.sub - 2",
            "a - (b - c)",
            "round(a / 3)",
            "value <= budget and value > 0",
            "max(a, b, 10)",
        ] {
            let expr = parse_notation(source).unwrap();
            let javascript = format!("event.value = {};", to_javascript(&expr));
            let back = match &parse_script(&javascript).unwrap()[0] {
                Stmt::SetValue(expr) => simplify(expr),
                other => panic!("{other:?}"),
            };
            assert_eq!(to_notation(&back), to_notation(&expr), "{source}");
        }
        assert_eq!(
            to_notation(&parse_notation("(a+b)*15%").unwrap()),
            "(a + b) * 0.15"
        );
    }

    #[test]
    fn refuses_what_is_outside_the_subset() {
        for script in [
            "for (var i = 0; i < 3; i++) {}",
            "function f() {}",
            "this.getField(\"a\").display = display.hidden;",
            "while (true) {}",
            "app.launchURL(\"http://x\")",
            "event.value = this.getField(\"a\").value.toFixed(2);",
            "event.target.textColor = color.red;",
        ] {
            assert!(parse_script(script).is_err(), "{script}");
        }
    }

    #[test]
    fn reads_ifs_alerts_and_variables() {
        let script = r#"
            // a typical hand-written check
            var total = AFMakeNumber(getField("a").value) + AFMakeNumber(getField("b").value);
            if (event.value > total) {
                app.alert("Too much.");
                event.rc = false;
            } else event.rc = true;
        "#;
        let parsed = parse_script(script).unwrap();
        assert_eq!(parsed.len(), 2);
        let mut fields = Vec::new();
        fields_in_statements(&parsed, &mut fields);
        assert_eq!(fields, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn deep_nesting_is_refused_not_crashing() {
        let deep = format!("event.value = {}1{};", "(".repeat(500), ")".repeat(500));
        assert!(parse_script(&deep).is_err());
        let long = "a + ".repeat(5000);
        assert!(parse_notation(&long).is_err());
    }
}
