//! openformula references as ods files store them: "[.A1]",
//! "[$Sheet2.A1:.B3]", "['My sheet'.A1]", "[.A:.A]", and 3-d
//! "[$Jan.A1:$Mar.A1]" (opendocument 1.2, part 2, section 5.8).

use crate::model::ErrorValue;

use super::super::ast::{Area, Expr, SheetRef};
use super::operators::join_range;
use super::{ParseError, Parser, Syntax};

impl Parser {
    pub(super) fn open_formula_reference(&mut self) -> Result<Expr, ParseError> {
        let start = self.pos;
        let mut quoted = false;
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == '\'' {
                quoted = !quoted;
            } else if c == ']' && !quoted {
                break;
            }
        }
        let inner: String = self.chars[start + 1..self.pos.saturating_sub(1)]
            .iter()
            .collect();
        let failed = ParseError {
            message: "this reference can't be read".into(),
            position: start,
        };
        if inner.contains("#REF!") {
            return Ok(Expr::Error {
                value: ErrorValue::Ref,
            });
        }
        read_inside(&inner).ok_or(failed)
    }
}

/// a plain a1 reference text read with the excel reader
fn a1_reference(text: &str, sheet: Option<SheetRef>) -> Option<(Expr, bool)> {
    let mut reader = Parser::new(text, Syntax::Excel, None);
    let expr = reader.try_reference(sheet)?;
    let read_all = reader.pos == reader.chars.len();
    Some((expr, read_all))
}

fn read_inside(inner: &str) -> Option<Expr> {
    let (first, second) = split_range(inner);
    let (first_sheet, first_cell) = split_part(&first)?;
    let sheet = first_sheet.map(|name| SheetRef::One { name });
    let Some(second) = second else {
        return a1_reference(&first_cell, sheet).map(|(expr, _)| expr);
    };
    let (second_sheet, second_cell) = split_part(&second)?;
    // whole columns or rows ("A:A") read as one excel reference
    if let Some((expr, true)) = a1_reference(&format!("{first_cell}:{second_cell}"), sheet.clone())
    {
        if matches!(&expr, Expr::Ref { reference } if matches!(reference.area, Area::Columns { .. } | Area::Rows { .. }))
        {
            return Some(expr);
        }
    }
    let (a, _) = a1_reference(&first_cell, sheet.clone())?;
    let (b, _) = a1_reference(&second_cell, None)?;
    match (&sheet, second_sheet) {
        (Some(SheetRef::One { name: x }), Some(y)) if !x.eq_ignore_ascii_case(&y) => {
            three_d(x, &y, &a, &b)
        }
        _ => Some(join_range(a, b)),
    }
}

fn three_d(first: &str, last: &str, a: &Expr, b: &Expr) -> Option<Expr> {
    let (Expr::Ref { reference: ra }, Expr::Ref { reference: rb }) = (a, b) else {
        return None;
    };
    let (Area::Cell { at: start }, Area::Cell { at: end }) = (ra.area, rb.area) else {
        return None;
    };
    let sheet = SheetRef::Span {
        first: first.to_string(),
        last: last.to_string(),
    };
    Some(Expr::reference(Some(sheet), Area::Range { start, end }))
}

/// splits "$Sheet1.A1:.B2" at the ":" outside quotes
fn split_range(inner: &str) -> (String, Option<String>) {
    let mut quoted = false;
    for (index, c) in inner.char_indices() {
        if c == '\'' {
            quoted = !quoted;
        } else if c == ':' && !quoted {
            return (
                inner[..index].to_string(),
                Some(inner[index + 1..].to_string()),
            );
        }
    }
    (inner.to_string(), None)
}

/// "$'My sheet'.A1" -> (Some("My sheet"), "A1"); ".A1" -> (None, "A1")
fn split_part(part: &str) -> Option<(Option<String>, String)> {
    let part = part.trim();
    let part = part.strip_prefix('$').unwrap_or(part);
    if let Some(cell) = part.strip_prefix('.') {
        return Some((None, cell.to_string()));
    }
    if let Some(rest) = part.strip_prefix('\'') {
        let (name, remainder) = quoted_prefix(rest);
        return Some((Some(name), remainder.strip_prefix('.')?.to_string()));
    }
    let (sheet, cell) = part.rsplit_once('.')?;
    Some((Some(sheet.to_string()), cell.to_string()))
}

/// the quoted name at the start of `rest` (opening quote already gone)
/// and what follows its closing quote
fn quoted_prefix(rest: &str) -> (String, String) {
    let mut name = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\'' {
            name.push(c);
        } else if chars.peek() == Some(&'\'') {
            chars.next();
            name.push('\'');
        } else {
            break;
        }
    }
    (name, chars.collect())
}
