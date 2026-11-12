//! whether a rule matches one cell. each rule is first prepared once for
//! the request (formulas parsed, range statistics worked out), then
//! asked about each cell on screen.

use std::collections::HashMap;

use crate::address::CellRef;
use crate::calc::context::Context;
use crate::calc::eval::Evaluator;
use crate::calc::value::{compare, to_bool, to_text};
use crate::formula::{refs::translate, Expr};
use crate::model::Value;

use super::statistics::ValueKey;

/// excel's comparisons for "cellIs" rules
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Comparison {
    Between,
    NotBetween,
    Equal,
    NotEqual,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

impl Comparison {
    pub fn parse(operator: &str) -> Option<Comparison> {
        Some(match operator {
            "between" => Comparison::Between,
            "notBetween" => Comparison::NotBetween,
            "equal" => Comparison::Equal,
            "notEqual" => Comparison::NotEqual,
            "greaterThan" => Comparison::GreaterThan,
            "lessThan" => Comparison::LessThan,
            "greaterThanOrEqual" => Comparison::GreaterThanOrEqual,
            "lessThanOrEqual" => Comparison::LessThanOrEqual,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextTest {
    Contains,
    NotContains,
    BeginsWith,
    EndsWith,
}

/// a value a rule compares with: worked out once when it is a constant,
/// or for each cell when it refers to cells (relative references move
/// with the cell, as if the formula were filled across the range)
#[derive(Clone, Debug)]
pub enum Operand {
    Fixed(Value),
    Moving(Expr),
}

/// the question a rule asks of a cell
#[derive(Clone, Debug)]
pub enum Test {
    Compare(Comparison, Vec<Operand>),
    /// true when the formula's result is TRUE or a number other than 0
    Formula(Expr),
    /// `needle` is lowercased, since excel's text rules ignore case
    Text(TextTest, String),
    Blanks(bool),
    Errors(bool),
    /// top or bottom n: numbers at or past the n-th one
    AtLeast(f64),
    AtMost(f64),
    Above {
        limit: f64,
        equal: bool,
    },
    Below {
        limit: f64,
        equal: bool,
    },
    /// duplicate (`unique` false) or unique values in the range
    Counted {
        counts: HashMap<ValueKey, u32>,
        unique: bool,
    },
    /// any number: colour scales and data bars draw every number
    Number,
    /// a rule that never matches (it could not be read, or nothing in the
    /// range qualifies)
    Never,
}

/// where a rule is being asked about: the cell, and the range's top-left
/// cell its formulas were written for
pub struct Place<'c, 'a> {
    pub ctx: &'c Context<'a>,
    pub sheet: usize,
    pub at: CellRef,
    pub origin: CellRef,
}

impl Place<'_, '_> {
    /// a formula's value as if it sat in this cell
    pub fn evaluate(&self, expr: &Expr) -> Value {
        let rows = i64::from(self.at.row) - i64::from(self.origin.row);
        let cols = i64::from(self.at.col) - i64::from(self.origin.col);
        let moved = translate(expr, rows, cols);
        let evaluator = Evaluator {
            ctx: self.ctx,
            sheet: self.sheet,
            row: self.at.row,
            col: self.at.col,
            // a range in a value's place means the cell in line with this
            // one, as in formulas written before dynamic arrays
            legacy: true,
            depth: 0,
        };
        evaluator.scalar(&moved)
    }

    fn operand(&self, operand: &Operand) -> Value {
        match operand {
            Operand::Fixed(value) => value.clone(),
            Operand::Moving(expr) => self.evaluate(expr),
        }
    }
}

/// true when a formula refers to nothing, so its value is the same in
/// every cell ("50", "\"Pass\"", "-3")
pub fn is_constant(expr: &Expr) -> bool {
    let mut constant = true;
    expr.walk(&mut |node| {
        if matches!(
            node,
            Expr::Ref { .. } | Expr::Name { .. } | Expr::Structured { .. } | Expr::Function { .. }
        ) {
            constant = false;
        }
    });
    constant
}

impl Test {
    pub fn matches(&self, value: &Value, place: &Place) -> bool {
        match self {
            Test::Compare(comparison, operands) => compares(*comparison, value, operands, place),
            Test::Formula(expr) => to_bool(&place.evaluate(expr)).unwrap_or(false),
            Test::Text(test, needle) => text_matches(*test, value, needle),
            Test::Blanks(wanted) => is_blank(value) == *wanted,
            Test::Errors(wanted) => matches!(value, Value::Error(_)) == *wanted,
            Test::AtLeast(limit) => number(value).is_some_and(|n| n >= *limit),
            Test::AtMost(limit) => number(value).is_some_and(|n| n <= *limit),
            Test::Above { limit, equal } => {
                number(value).is_some_and(|n| n > *limit || (*equal && n == *limit))
            }
            Test::Below { limit, equal } => {
                number(value).is_some_and(|n| n < *limit || (*equal && n == *limit))
            }
            Test::Counted { counts, unique } => ValueKey::of(value)
                .and_then(|key| counts.get(&key))
                .is_some_and(|count| (*count == 1) == *unique),
            Test::Number => number(value).is_some(),
            Test::Never => false,
        }
    }
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => Some(*n),
        _ => None,
    }
}

/// excel's "blanks" rule also counts text of spaces only (LEN(TRIM(A1))=0)
fn is_blank(value: &Value) -> bool {
    match value {
        Value::Blank => true,
        Value::Text(text) => text.trim().is_empty(),
        _ => false,
    }
}

/// a cell compared the way excel's comparison operators compare: a blank
/// counts as 0 (or "" beside text), text ignores case, and an error on
/// either side never matches
fn compares(comparison: Comparison, value: &Value, operands: &[Operand], place: &Place) -> bool {
    use std::cmp::Ordering::*;
    if matches!(value, Value::Error(_)) {
        return false;
    }
    let mut values = operands.iter().map(|operand| place.operand(operand));
    let Some(first) = values.next() else {
        return false;
    };
    if matches!(first, Value::Error(_)) {
        return false;
    }
    let order = compare(value, &first);
    match comparison {
        Comparison::Equal => order == Equal,
        Comparison::NotEqual => order != Equal,
        Comparison::GreaterThan => order == Greater,
        Comparison::LessThan => order == Less,
        Comparison::GreaterThanOrEqual => order != Less,
        Comparison::LessThanOrEqual => order != Greater,
        Comparison::Between | Comparison::NotBetween => {
            let Some(second) = values.next() else {
                return false;
            };
            if matches!(second, Value::Error(_)) {
                return false;
            }
            // "between 10 and 5" means between 5 and 10, as in excel
            let (low, high) = if compare(&first, &second) == Greater {
                (second, first)
            } else {
                (first, second)
            };
            let inside = compare(value, &low) != Less && compare(value, &high) != Greater;
            inside == (comparison == Comparison::Between)
        }
    }
}

/// excel builds these rules on SEARCH, so a number is matched by its
/// plain digits ("1.5") and an error cell only by "does not contain"
fn text_matches(test: TextTest, value: &Value, needle: &str) -> bool {
    let Ok(text) = to_text(value) else {
        return test == TextTest::NotContains;
    };
    let text = text.to_lowercase();
    match test {
        TextTest::Contains => text.contains(needle),
        TextTest::NotContains => !text.contains(needle),
        TextTest::BeginsWith => text.starts_with(needle),
        TextTest::EndsWith => text.ends_with(needle),
    }
}
