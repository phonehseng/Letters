//! the smallest pieces of a formula: what a primary operand starts with,
//! numbers, quoted text, error values, array constants and function calls.

use crate::model::ErrorValue;

use super::super::ast::{Expr, SheetRef, UnaryOp};
use super::{is_name_start, ParseError, Parser, Syntax};

impl Parser {
    /// one operand: a value, a reference, a name or a function call
    pub(super) fn primary(&mut self) -> Result<Expr, ParseError> {
        self.skip_spaces();
        let Some(c) = self.peek() else {
            return Err(self.error("the formula ends too early"));
        };
        match c {
            '(' => self.parenthesised(),
            '{' => self.array(),
            '"' => Ok(Expr::Text {
                value: self.string()?,
            }),
            '#' => self.error_literal(),
            '@' => self.implicit_intersection(),
            '[' if self.syntax == Syntax::OpenFormula => self.open_formula_reference(),
            '[' if self.at_external_book() => self.external_reference(),
            '[' => self.structured(None),
            '\'' => self.quoted_sheet_reference(),
            c if c.is_ascii_digit() || c == '.' => self.number_or_rows(),
            c if c == '$' || is_name_start(c) => self.reference_or_name(),
            other => Err(self.error(&format!("unexpected \"{other}\""))),
        }
    }

    /// "@A1:A5": one value from a range, as excel 365 writes it
    fn implicit_intersection(&mut self) -> Result<Expr, ParseError> {
        self.pos += 1;
        let inner = self.primary()?;
        Ok(Expr::Unary {
            op: UnaryOp::At,
            expr: Box::new(inner),
        })
    }

    /// quoted text; a doubled quote stands for one quote
    pub(super) fn string(&mut self) -> Result<String, ParseError> {
        self.pos += 1;
        let mut text = String::new();
        loop {
            match self.peek() {
                None => return Err(self.error("a text in quotes is not closed")),
                Some('"') if self.peek_at(1) == Some('"') => {
                    text.push('"');
                    self.pos += 2;
                }
                Some('"') => {
                    self.pos += 1;
                    return Ok(text);
                }
                Some(c) => {
                    text.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    pub(super) fn error_literal(&mut self) -> Result<Expr, ParseError> {
        for error in ErrorValue::ALL {
            if self.eat_ignore_case(error.as_str()) {
                return Ok(Expr::Error { value: error });
            }
        }
        // libreoffice's spelling of #N/A
        if self.eat_ignore_case("#N/A!") {
            return Ok(Expr::Error {
                value: ErrorValue::NA,
            });
        }
        Err(self.error("unknown error value"))
    }

    /// a number, or whole rows like "3:5"
    pub(super) fn number_or_rows(&mut self) -> Result<Expr, ParseError> {
        let start = self.pos;
        if let Some(rows) = self.try_rows(None) {
            return Ok(rows);
        }
        self.pos = start;
        while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
            self.pos += 1;
        }
        self.exponent();
        let text = self.text_from(start);
        match text.parse::<f64>() {
            Ok(_) => Ok(Expr::number(keep_fifteen_digits(&text))),
            Err(_) => Err(self.error(&format!("\"{text}\" is not a number"))),
        }
    }

    /// "E+5" after a number; left alone when no digits follow
    fn exponent(&mut self) {
        if !self.peek().is_some_and(|c| c == 'e' || c == 'E') {
            return;
        }
        let save = self.pos;
        self.pos += 1;
        if self.peek().is_some_and(|c| c == '+' || c == '-') {
            self.pos += 1;
        }
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos = save;
            return;
        }
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
    }

    /// "{1,2;3,4}" in excel, "{1;2|3;4}" in openformula
    fn array(&mut self) -> Result<Expr, ParseError> {
        self.pos += 1;
        let (column_separator, row_separator) = match self.syntax {
            Syntax::Excel => (',', ';'),
            Syntax::OpenFormula => (';', '|'),
        };
        let mut rows: Vec<Vec<Expr>> = vec![Vec::new()];
        loop {
            self.skip_spaces();
            let item = self.array_item()?;
            if let Some(row) = rows.last_mut() {
                row.push(item);
            }
            self.skip_spaces();
            match self.peek() {
                Some(c) if c == column_separator => self.pos += 1,
                Some(c) if c == row_separator => {
                    self.pos += 1;
                    rows.push(Vec::new());
                }
                Some('}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.error("an array constant is not closed")),
            }
        }
        let width = rows[0].len();
        if rows.iter().any(|row| row.len() != width) {
            return Err(self.error("the rows of an array constant must be the same length"));
        }
        Ok(Expr::Array { rows })
    }

    /// array constants hold only values, never references or formulas
    fn array_item(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            Some('"') => Ok(Expr::Text {
                value: self.string()?,
            }),
            Some('#') => self.error_literal(),
            Some(sign @ ('-' | '+')) => {
                self.pos += 1;
                match self.number_or_rows()? {
                    Expr::Number { value } if sign == '-' => Ok(Expr::number(-value)),
                    number @ Expr::Number { .. } => Ok(number),
                    _ => Err(self.error("an array constant can only hold values")),
                }
            }
            Some(c) if c.is_ascii_digit() || c == '.' => self.number_or_rows(),
            _ => match self.name_text().to_ascii_uppercase().as_str() {
                "TRUE" => Ok(Expr::Bool { value: true }),
                "FALSE" => Ok(Expr::Bool { value: false }),
                _ => Err(self.error("an array constant can only hold values")),
            },
        }
    }

    /// the arguments of a function, the cursor just after "("
    pub(super) fn function(&mut self, written: &str) -> Result<Expr, ParseError> {
        let name = canonical_function_name(written);
        self.nest()?;
        let args = self.arguments(&name)?;
        self.depth -= 1;
        // openformula writes the constants as functions: TRUE()
        let is_constant = name == "TRUE" || name == "FALSE";
        if args.is_empty() && is_constant && self.syntax == Syntax::OpenFormula {
            return Ok(Expr::Bool {
                value: name == "TRUE",
            });
        }
        Ok(Expr::Function { name, args })
    }

    fn arguments(&mut self, name: &str) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        self.skip_spaces();
        if self.eat(")") {
            return Ok(args);
        }
        let separator = self.arg_separator();
        loop {
            self.skip_spaces();
            let left_out = self.peek() == Some(separator) || self.peek() == Some(')');
            if left_out {
                args.push(Expr::Missing);
            } else {
                args.push(self.comparison()?);
            }
            self.skip_spaces();
            match self.peek() {
                Some(c) if c == separator => self.pos += 1,
                Some(')') => {
                    self.pos += 1;
                    return Ok(args);
                }
                _ => {
                    return Err(
                        self.error(&format!("a \")\" is missing after the arguments of {name}"))
                    )
                }
            }
        }
    }

    /// "[1]Sheet1!A1": a reference into another workbook
    fn at_external_book(&self) -> bool {
        let rest = &self.chars[self.pos + 1..];
        let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
        digits > 0
            && rest.get(digits) == Some(&']')
            && rest.get(digits + 1).is_some_and(|c| is_name_start(*c))
    }

    fn external_reference(&mut self) -> Result<Expr, ParseError> {
        self.pos += 1;
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        let workbook = self.text_from(start);
        self.pos += 1;
        let name = self.name_text();
        if !self.eat("!") {
            return Err(self.error("a sheet name must be followed by \"!\""));
        }
        self.after_sheet(SheetRef::External { workbook, name })
    }
}

/// excel keeps only the first 15 significant digits of a number typed in
/// a formula, cutting the rest: 123456789012345678 becomes
/// 123456789012345000 (measured with the harness)
pub fn keep_fifteen_digits(text: &str) -> f64 {
    let (mantissa, exponent) = match text.find(['e', 'E']) {
        Some(index) => (&text[..index], &text[index..]),
        None => (text, ""),
    };
    let mut kept = String::new();
    let mut significant = 0;
    for c in mantissa.chars() {
        let counts = c.is_ascii_digit() && (significant > 0 || c != '0');
        if counts {
            significant += 1;
        }
        let keep = !c.is_ascii_digit() || significant <= 15 || !counts && significant == 0;
        kept.push(if keep { c } else { '0' });
    }
    format!("{kept}{exponent}").parse().unwrap_or(0.0)
}

/// upper case, without the prefixes excel puts on newer functions in
/// files ("_xlfn.XLOOKUP", "_xlfn._xlws.SORT") or ods's
/// "COM.MICROSOFT.CONCAT".
pub fn canonical_function_name(written: &str) -> String {
    let mut name = written.to_ascii_uppercase();
    for prefix in [
        "_XLFN._XLWS.",
        "_XLFN.",
        "_XLWS.",
        "COM.MICROSOFT.",
        "_XLUDF.",
    ] {
        if let Some(rest) = name.strip_prefix(prefix) {
            name = rest.to_string();
        }
    }
    name
}
