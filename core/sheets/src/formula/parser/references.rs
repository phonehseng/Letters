//! cell, area, column and row references in a1 and r1c1 style, sheet
//! prefixes ("Sheet2!", "'My sheet'!", "Sheet1:Sheet3!"), and telling a
//! reference apart from a defined name or a function.

use crate::address::{column_index, CellRef, MAX_COLS, MAX_ROWS};

use super::super::ast::{Area, CellPos, Expr, SheetRef};
use super::{is_name_char, ParseError, Parser};

/// one r1c1 part: the row or column number and whether it is absolute
type R1c1Part = Option<(u32, bool)>;

impl Parser {
    /// whole rows "3:5" or "$3:$5"
    pub(super) fn try_rows(&mut self, sheet: Option<SheetRef>) -> Option<Expr> {
        let start = self.pos;
        let first = self.row_part();
        let joined = first.is_some() && self.eat(":");
        let last = if joined { self.row_part() } else { None };
        let (Some((first, first_abs)), Some((last, last_abs))) = (first, last) else {
            self.pos = start;
            return None;
        };
        let area = Area::Rows {
            first,
            last,
            first_abs,
            last_abs,
        };
        Some(Expr::reference(sheet, area))
    }

    fn row_part(&mut self) -> Option<(u32, bool)> {
        let absolute = self.eat("$");
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        let row: u32 = self.text_from(start).parse().ok()?;
        (1..=MAX_ROWS).contains(&row).then(|| (row - 1, absolute))
    }

    fn column_part(&mut self) -> Option<(u32, bool)> {
        let absolute = self.eat("$");
        let start = self.pos;
        // the grid's widest column name, "XFD", has three letters; reading
        // a fourth makes "LONG1" fail as a cell and become a name
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) && self.pos - start < 4 {
            self.pos += 1;
        }
        column_index(&self.text_from(start)).map(|col| (col, absolute))
    }

    fn cell_part(&mut self) -> Option<CellPos> {
        let (col, col_abs) = self.column_part()?;
        let (row, row_abs) = self.row_part()?;
        Some(CellPos {
            row,
            col,
            row_abs,
            col_abs,
        })
    }

    /// "'My sheet'!A1" or the 3-d "'Jan':'Mar'!A1"
    pub(super) fn quoted_sheet_reference(&mut self) -> Result<Expr, ParseError> {
        let name = self.quoted_name()?;
        let sheet = if self.eat(":") {
            let last = if self.peek() == Some('\'') {
                self.quoted_name()?
            } else {
                self.name_text()
            };
            SheetRef::Span { first: name, last }
        } else {
            sheet_from_text(name)
        };
        if !self.eat("!") {
            return Err(self.error("a sheet name must be followed by \"!\""));
        }
        self.after_sheet(sheet)
    }

    /// a name in single quotes; a doubled quote stands for one
    fn quoted_name(&mut self) -> Result<String, ParseError> {
        self.pos += 1;
        let mut name = String::new();
        loop {
            match self.peek() {
                None => return Err(self.error("a quoted sheet name is not closed")),
                Some('\'') if self.peek_at(1) == Some('\'') => {
                    name.push('\'');
                    self.pos += 2;
                }
                Some('\'') => {
                    self.pos += 1;
                    return Ok(name);
                }
                Some(c) => {
                    name.push(c);
                    self.pos += 1;
                }
            }
        }
    }

    /// what follows "Sheet1!": a cell, an area, columns, rows, or a
    /// sheet-level name
    pub(super) fn after_sheet(&mut self, sheet: SheetRef) -> Result<Expr, ParseError> {
        if self.peek() == Some('#') {
            // "Sheet1!#REF!" is what is left after a range was deleted
            return self.error_literal();
        }
        if let Some(reference) = self.try_reference(Some(sheet.clone())) {
            return Ok(reference);
        }
        let name = self.name_text();
        if name.is_empty() {
            return Err(self.error("a reference is missing after the sheet name"));
        }
        let prefix = super::super::print::sheet_text(&sheet);
        Ok(Expr::Name {
            name: format!("{prefix}!{name}"),
        })
    }

    /// a cell, area, column or row reference at the cursor
    pub(super) fn try_reference(&mut self, sheet: Option<SheetRef>) -> Option<Expr> {
        let start = self.pos;
        if let Some(at) = self.r1c1 {
            if let Some(expr) = self.try_r1c1(sheet.clone(), at) {
                return Some(expr);
            }
            self.pos = start;
        }
        if let Some(cell) = self.cell_part() {
            // "LOG10(" and "A1B" are a function and a name, not cells
            if self
                .peek()
                .is_some_and(|c| is_name_char(c) || c == '(' || c == '[')
            {
                self.pos = start;
                return None;
            }
            return Some(Expr::reference(sheet, Area::Cell { at: cell }));
        }
        self.pos = start;
        if let Some(columns) = self.try_columns(sheet.clone()) {
            return Some(columns);
        }
        self.pos = start;
        if self.peek().is_some_and(|c| c.is_ascii_digit() || c == '$') {
            if let Some(rows) = self.try_rows(sheet) {
                return Some(rows);
            }
        }
        self.pos = start;
        None
    }

    /// whole columns "A:C"
    fn try_columns(&mut self, sheet: Option<SheetRef>) -> Option<Expr> {
        let (first, first_abs) = self.column_part()?;
        if !self.eat(":") {
            return None;
        }
        let (last, last_abs) = self.column_part()?;
        if self.peek().is_some_and(|c| is_name_char(c) || c == '(') {
            return None;
        }
        let area = Area::Columns {
            first,
            last,
            first_abs,
            last_abs,
        };
        Some(Expr::reference(sheet, area))
    }

    /// "R1C1", "R[-1]C", "C[2]", "R3" and ranges of them
    fn try_r1c1(&mut self, sheet: Option<SheetRef>, at: CellRef) -> Option<Expr> {
        let (row, col) = self.r1c1_part(at)?;
        if self.peek().is_some_and(|c| is_name_char(c) || c == '(') {
            return None;
        }
        let save = self.pos;
        if self.eat(":") {
            if let Some((row2, col2)) = self.r1c1_part(at) {
                return r1c1_range((row, col), (row2, col2))
                    .map(|area| Expr::reference(sheet, area));
            }
            self.pos = save;
        }
        r1c1_single(row, col).map(|area| Expr::reference(sheet, area))
    }

    fn r1c1_part(&mut self, at: CellRef) -> Option<(R1c1Part, R1c1Part)> {
        let row = if self.peek().is_some_and(|c| c == 'R' || c == 'r') {
            self.pos += 1;
            Some(self.r1c1_number(at.row, MAX_ROWS)?)
        } else {
            None
        };
        let col = if self.peek().is_some_and(|c| c == 'C' || c == 'c') {
            self.pos += 1;
            Some(self.r1c1_number(at.col, MAX_COLS)?)
        } else {
            None
        };
        (row.is_some() || col.is_some()).then_some((row, col))
    }

    /// "5" is absolute, "[-1]" relative, nothing means the same row or column
    fn r1c1_number(&mut self, base: u32, limit: u32) -> Option<(u32, bool)> {
        if self.eat("[") {
            let start = self.pos;
            if matches!(self.peek(), Some('-') | Some('+')) {
                self.pos += 1;
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
            let offset: i64 = self.text_from(start).parse().ok()?;
            if !self.eat("]") {
                return None;
            }
            let value = i64::from(base) + offset;
            return (0..i64::from(limit))
                .contains(&value)
                .then_some((value as u32, false));
        }
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if start == self.pos {
            return Some((base, false));
        }
        let value: u32 = self.text_from(start).parse().ok()?;
        (1..=limit).contains(&value).then(|| (value - 1, true))
    }

    /// a reference, a sheet prefix, a function call, a table reference,
    /// TRUE or FALSE, or else a defined name
    pub(super) fn reference_or_name(&mut self) -> Result<Expr, ParseError> {
        let start = self.pos;
        if let Some(reference) = self.try_reference(None) {
            return Ok(reference);
        }
        self.pos = start;
        if self.peek() == Some('$') {
            return Err(self.error("a \"$\" must be part of a reference"));
        }
        let name = self.name_text();
        if self.eat("!") {
            return self.after_sheet(SheetRef::One { name });
        }
        if let Some(span) = self.try_sheet_span(&name) {
            return self.after_sheet(span);
        }
        let save = self.pos;
        self.skip_spaces();
        if self.eat("(") {
            return self.function(&name);
        }
        self.pos = save;
        if self.peek() == Some('[') {
            return self.structured(Some(name));
        }
        match name.to_ascii_uppercase().as_str() {
            "TRUE" => Ok(Expr::Bool { value: true }),
            "FALSE" => Ok(Expr::Bool { value: false }),
            _ => Ok(Expr::Name { name }),
        }
    }

    /// "Sheet1:Sheet3!" after the first name was read
    fn try_sheet_span(&mut self, first: &str) -> Option<SheetRef> {
        let save = self.pos;
        if self.eat(":") {
            let last = self.name_text();
            if !last.is_empty() && self.eat("!") {
                return Some(SheetRef::Span {
                    first: first.to_string(),
                    last,
                });
            }
        }
        self.pos = save;
        None
    }
}

/// "[1]Sheet1" in a quoted name is a sheet in another workbook
fn sheet_from_text(name: String) -> SheetRef {
    if let Some(rest) = name.strip_prefix('[') {
        if let Some((book, sheet)) = rest.split_once(']') {
            return SheetRef::External {
                workbook: book.to_string(),
                name: sheet.to_string(),
            };
        }
    }
    SheetRef::One { name }
}

fn r1c1_corner(row: R1c1Part, col: R1c1Part) -> Option<CellPos> {
    let (row, row_abs) = row?;
    let (col, col_abs) = col?;
    Some(CellPos {
        row,
        col,
        row_abs,
        col_abs,
    })
}

fn r1c1_single(row: R1c1Part, col: R1c1Part) -> Option<Area> {
    match (row, col) {
        (Some(_), Some(_)) => r1c1_corner(row, col).map(|at| Area::Cell { at }),
        (Some((r, abs)), None) => Some(Area::Rows {
            first: r,
            last: r,
            first_abs: abs,
            last_abs: abs,
        }),
        (None, Some((c, abs))) => Some(Area::Columns {
            first: c,
            last: c,
            first_abs: abs,
            last_abs: abs,
        }),
        (None, None) => None,
    }
}

fn r1c1_range(first: (R1c1Part, R1c1Part), second: (R1c1Part, R1c1Part)) -> Option<Area> {
    if let (Some(start), Some(end)) = (
        r1c1_corner(first.0, first.1),
        r1c1_corner(second.0, second.1),
    ) {
        return Some(Area::Range { start, end });
    }
    match (first, second) {
        ((Some(a), None), (Some(b), None)) => Some(Area::Rows {
            first: a.0,
            last: b.0,
            first_abs: a.1,
            last_abs: b.1,
        }),
        ((None, Some(a)), (None, Some(b))) => Some(Area::Columns {
            first: a.0,
            last: b.0,
            first_abs: a.1,
            last_abs: b.1,
        }),
        _ => None,
    }
}
