//! the values formulas pass around while calculating (a single value,
//! an array, or a reference to cells) and excel's rules for turning one
//! kind into another.

use std::cmp::Ordering;

use crate::address::Range;
use crate::model::{ErrorValue, Value};

/// a rectangle of cells on one sheet, as a reference evaluates to
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub sheet: usize,
    pub range: Range,
}

/// an array of values, row by row
#[derive(Clone, Debug, PartialEq)]
pub struct Array {
    pub rows: usize,
    pub cols: usize,
    pub items: Vec<Value>,
}

impl Array {
    pub fn new(rows: usize, cols: usize, items: Vec<Value>) -> Array {
        Array { rows, cols, items }
    }

    pub fn filled(rows: usize, cols: usize, value: Value) -> Array {
        Array::new(rows, cols, vec![value; rows * cols])
    }

    pub fn column(items: Vec<Value>) -> Array {
        Array::new(items.len(), 1, items)
    }

    pub fn get(&self, row: usize, col: usize) -> &Value {
        &self.items[row * self.cols + col]
    }

    /// the value at (row, col), stretching a single row or column the way
    /// excel broadcasts arrays of different sizes; #N/A outside
    pub fn broadcast(&self, row: usize, col: usize) -> Value {
        let r = if self.rows == 1 { 0 } else { row };
        let c = if self.cols == 1 { 0 } else { col };
        if r < self.rows && c < self.cols {
            self.get(r, c).clone()
        } else {
            Value::Error(ErrorValue::NA)
        }
    }

    pub fn transpose(&self) -> Array {
        let mut items = Vec::with_capacity(self.items.len());
        for c in 0..self.cols {
            for r in 0..self.rows {
                items.push(self.get(r, c).clone());
            }
        }
        Array::new(self.cols, self.rows, items)
    }

    pub fn rows_iter(&self) -> impl Iterator<Item = &[Value]> {
        self.items.chunks(self.cols.max(1))
    }
}

/// what evaluating an expression gives
#[derive(Clone, Debug, PartialEq)]
pub enum CalcValue {
    Scalar(Value),
    Array(Array),
    /// one or more areas (more than one only from the union operator)
    Ref(Vec<Area>),
}

impl CalcValue {
    pub fn error(error: ErrorValue) -> CalcValue {
        CalcValue::Scalar(Value::Error(error))
    }

    pub fn number(n: f64) -> CalcValue {
        CalcValue::Scalar(number_or_error(n))
    }

    pub fn text(text: impl Into<String>) -> CalcValue {
        CalcValue::Scalar(Value::Text(text.into()))
    }

    pub fn boolean(b: bool) -> CalcValue {
        CalcValue::Scalar(Value::Bool(b))
    }

    pub fn as_error(&self) -> Option<ErrorValue> {
        match self {
            CalcValue::Scalar(Value::Error(e)) => Some(*e),
            _ => None,
        }
    }
}

/// infinities and nan are not values in a spreadsheet: they are #NUM!
pub fn number_or_error(n: f64) -> Value {
    if n.is_finite() {
        // excel has no negative zero
        Value::Number(if n == 0.0 { 0.0 } else { n })
    } else {
        Value::Error(ErrorValue::Num)
    }
}

/// a value used as a number: text that reads as a number (or a date or
/// time) counts, as excel's arithmetic coerces it; other text is #VALUE!
pub fn to_number(value: &Value, system: crate::model::DateSystem) -> Result<f64, ErrorValue> {
    match value {
        Value::Number(n) => Ok(*n),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::Blank => Ok(0.0),
        Value::Text(text) => {
            super::text_numbers::parse_number(text, system).ok_or(ErrorValue::Value)
        }
        Value::Error(e) => Err(*e),
    }
}

/// a value used as text
pub fn to_text(value: &Value) -> Result<String, ErrorValue> {
    match value {
        Value::Number(n) => Ok(general_text(*n)),
        Value::Bool(b) => Ok(if *b { "TRUE" } else { "FALSE" }.to_string()),
        Value::Blank => Ok(String::new()),
        Value::Text(text) => Ok(text.clone()),
        Value::Error(e) => Err(*e),
    }
}

/// a value used as true or false
pub fn to_bool(value: &Value) -> Result<bool, ErrorValue> {
    match value {
        Value::Number(n) => Ok(*n != 0.0),
        Value::Bool(b) => Ok(*b),
        Value::Blank => Ok(false),
        Value::Text(text) if text.eq_ignore_ascii_case("TRUE") => Ok(true),
        Value::Text(text) if text.eq_ignore_ascii_case("FALSE") => Ok(false),
        Value::Text(_) => Err(ErrorValue::Value),
        Value::Error(e) => Err(*e),
    }
}

/// the significant digits excel keeps when it shows or converts a number
pub const SIGNIFICANT_DIGITS: usize = 15;

/// a number rounded to 15 significant digits, the precision excel shows
pub fn round_significant(n: f64) -> f64 {
    if n == 0.0 || !n.is_finite() {
        return n;
    }
    format!("{:.*e}", SIGNIFICANT_DIGITS - 1, n)
        .parse()
        .unwrap_or(n)
}

/// a number as text the way excel converts it ("General" without a
/// column width): up to 15 significant digits, trailing zeros dropped,
/// scientific notation for very large or very small numbers
pub fn general_text(n: f64) -> String {
    if n == 0.0 {
        return "0".into();
    }
    let scientific = format!("{:.*e}", SIGNIFICANT_DIGITS - 1, n);
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect();
    let digits = digits.trim_end_matches('0');
    let negative = n < 0.0;
    // excel writes the fixed form when it takes at most 20 characters
    // (sign aside) and scientific notation otherwise: 1E+19 and 1E-15 are
    // written out, 1E+20 and 1E-19 are not (measured with the harness)
    let fixed = fixed_text(negative, digits, exponent);
    if fixed.trim_start_matches('-').len() > MAX_FIXED_LENGTH {
        return scientific_text(negative, digits, exponent);
    }
    fixed
}

/// the longest number excel writes without an exponent when it turns a
/// number into text
const MAX_FIXED_LENGTH: usize = 20;

fn scientific_text(negative: bool, digits: &str, exponent: i32) -> String {
    let sign = if negative { "-" } else { "" };
    let (first, rest) = digits.split_at(1.min(digits.len()));
    let mantissa = if rest.is_empty() {
        first.to_string()
    } else {
        format!("{first}.{rest}")
    };
    let exponent_sign = if exponent < 0 { "-" } else { "+" };
    format!("{sign}{mantissa}E{exponent_sign}{:02}", exponent.abs())
}

fn fixed_text(negative: bool, digits: &str, exponent: i32) -> String {
    let sign = if negative { "-" } else { "" };
    if exponent < 0 {
        let zeros = "0".repeat((-exponent - 1) as usize);
        return format!("{sign}0.{zeros}{digits}");
    }
    let whole_digits = exponent as usize + 1;
    if digits.len() <= whole_digits {
        let zeros = "0".repeat(whole_digits - digits.len());
        return format!("{sign}{digits}{zeros}");
    }
    let (whole, fraction) = digits.split_at(whole_digits);
    format!("{sign}{whole}.{fraction}")
}

/// excel's order between kinds of values: numbers, then text, then
/// logical values; errors are handled before comparing
fn kind_rank(value: &Value) -> u8 {
    match value {
        Value::Number(_) | Value::Blank => 0,
        Value::Text(_) => 1,
        Value::Bool(_) => 2,
        Value::Error(_) => 3,
    }
}

/// compares two values as the comparison operators do. a blank cell
/// counts as 0, "" or FALSE depending on what it is compared with. text
/// ignores case. numbers equal to 15 significant digits are equal
/// (excel: =0.1+0.2=0.3 is TRUE).
pub fn compare(a: &Value, b: &Value) -> Ordering {
    let blank_like = |blank: &Value, other: &Value| match (blank, other) {
        (Value::Blank, Value::Text(_)) => Value::Text(String::new()),
        (Value::Blank, Value::Bool(_)) => Value::Bool(false),
        (Value::Blank, _) => Value::Number(0.0),
        (value, _) => value.clone(),
    };
    let a = blank_like(a, b);
    let b = blank_like(b, &a);
    match (&a, &b) {
        (Value::Number(x), Value::Number(y)) => compare_numbers(*x, *y),
        (Value::Text(x), Value::Text(y)) => compare_text(x, y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        _ => kind_rank(&a).cmp(&kind_rank(&b)),
    }
}

pub fn compare_numbers(x: f64, y: f64) -> Ordering {
    if x == y || round_significant(x) == round_significant(y) {
        Ordering::Equal
    } else {
        x.partial_cmp(&y).unwrap_or(Ordering::Equal)
    }
}

/// case-insensitive order, as excel compares text
pub fn compare_text(x: &str, y: &str) -> Ordering {
    // plain ascii text compares without copying (the usual case in a
    // large lookup or sort); anything else is lowercased first
    if x.is_ascii() && y.is_ascii() {
        let lower = |b: &u8| b.to_ascii_lowercase();
        return x
            .bytes()
            .map(|b| lower(&b))
            .cmp(y.bytes().map(|b| lower(&b)));
    }
    x.to_lowercase().cmp(&y.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DateSystem;

    #[test]
    fn numbers_convert_to_text_like_excel() {
        assert_eq!(general_text(0.1 + 0.2), "0.3");
        assert_eq!(general_text(1.0 / 3.0), "0.333333333333333");
        assert_eq!(general_text(123_456_789_012_345.0), "123456789012345");
        assert_eq!(general_text(1e15), "1000000000000000");
        assert_eq!(
            general_text(123_456_789_012_345_678.0),
            "123456789012346000"
        );
        assert_eq!(general_text(1e20), "1E+20");
        assert_eq!(general_text(1e-15), "0.000000000000001");
        assert_eq!(general_text(1e-19), "1E-19");
        assert_eq!(general_text(-2.5), "-2.5");
        assert_eq!(general_text(1e-10), "0.0000000001");
        assert_eq!(general_text(0.0001), "0.0001");
        assert_eq!(general_text(100.0), "100");
    }

    #[test]
    fn comparisons_follow_excels_rules() {
        assert_eq!(
            compare(&Value::Number(0.1 + 0.2), &Value::Number(0.3)),
            Ordering::Equal
        );
        assert_eq!(
            compare(&Value::text("abc"), &Value::text("ABC")),
            Ordering::Equal
        );
        assert_eq!(
            compare(&Value::Number(1e9), &Value::text("a")),
            Ordering::Less
        );
        assert_eq!(
            compare(&Value::text("z"), &Value::Bool(false)),
            Ordering::Less
        );
        assert_eq!(compare(&Value::Blank, &Value::Number(0.0)), Ordering::Equal);
        assert_eq!(compare(&Value::Blank, &Value::text("")), Ordering::Equal);
        assert_eq!(compare(&Value::Blank, &Value::Bool(false)), Ordering::Equal);
    }

    #[test]
    fn coercion_to_numbers() {
        let system = DateSystem::Excel1900;
        assert_eq!(to_number(&Value::text(" 12 "), system), Ok(12.0));
        assert_eq!(to_number(&Value::Bool(true), system), Ok(1.0));
        assert_eq!(
            to_number(&Value::text("abc"), system),
            Err(ErrorValue::Value)
        );
        assert_eq!(to_bool(&Value::text("true")), Ok(true));
        assert_eq!(
            number_or_error(f64::INFINITY),
            Value::Error(ErrorValue::Num)
        );
    }
}
