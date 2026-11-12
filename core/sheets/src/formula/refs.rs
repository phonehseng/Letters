//! changing the references inside formulas: moving a formula to another
//! cell (copy, fill), and keeping formulas pointing at the same cells
//! when rows or columns are inserted or deleted, or sheets renamed or
//! removed.

use crate::address::{MAX_COLS, MAX_ROWS};
use crate::model::ErrorValue;

use super::ast::*;

/// the formula as it reads after being copied `rows` down and `cols`
/// right. relative parts move; absolute ($) parts stay. a reference
/// pushed off the grid becomes #REF!, as in excel.
pub fn translate(expr: &Expr, rows: i64, cols: i64) -> Expr {
    expr.clone().transform(&mut |node| match node {
        Expr::Ref { reference } => match translate_area(&reference.area, rows, cols) {
            Some(area) => Expr::Ref {
                reference: Box::new(Reference {
                    sheet: reference.sheet,
                    area,
                }),
            },
            None => Expr::Error {
                value: ErrorValue::Ref,
            },
        },
        other => other,
    })
}

fn moved(value: u32, absolute: bool, by: i64, limit: u32) -> Option<u32> {
    if absolute {
        return Some(value);
    }
    let next = i64::from(value) + by;
    (0..i64::from(limit)).contains(&next).then_some(next as u32)
}

fn translate_pos(pos: CellPos, rows: i64, cols: i64) -> Option<CellPos> {
    Some(CellPos {
        row: moved(pos.row, pos.row_abs, rows, MAX_ROWS)?,
        col: moved(pos.col, pos.col_abs, cols, MAX_COLS)?,
        ..pos
    })
}

fn translate_area(area: &Area, rows: i64, cols: i64) -> Option<Area> {
    Some(match *area {
        Area::Cell { at } => Area::Cell {
            at: translate_pos(at, rows, cols)?,
        },
        Area::Range { start, end } => Area::Range {
            start: translate_pos(start, rows, cols)?,
            end: translate_pos(end, rows, cols)?,
        },
        Area::Columns {
            first,
            last,
            first_abs,
            last_abs,
        } => Area::Columns {
            first: moved(first, first_abs, cols, MAX_COLS)?,
            last: moved(last, last_abs, cols, MAX_COLS)?,
            first_abs,
            last_abs,
        },
        Area::Rows {
            first,
            last,
            first_abs,
            last_abs,
        } => Area::Rows {
            first: moved(first, first_abs, rows, MAX_ROWS)?,
            last: moved(last, last_abs, rows, MAX_ROWS)?,
            first_abs,
            last_abs,
        },
    })
}

/// rows or columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    Rows,
    Columns,
}

/// an insertion (count > 0) or deletion (count < 0) of whole rows or
/// columns at `at` on one sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shift {
    pub axis: Axis,
    pub at: u32,
    pub count: i64,
}

impl Shift {
    /// where a single row or column number ends up; none when deleted.
    pub fn position(&self, value: u32) -> Option<u32> {
        let limit = match self.axis {
            Axis::Rows => MAX_ROWS,
            Axis::Columns => MAX_COLS,
        };
        if self.count >= 0 {
            if value < self.at {
                return Some(value);
            }
            let next = i64::from(value) + self.count;
            (next < i64::from(limit)).then_some(next as u32)
        } else {
            let removed_end = i64::from(self.at) - self.count; // one past the last deleted
            let v = i64::from(value);
            if v < i64::from(self.at) {
                Some(value)
            } else if v < removed_end {
                None
            } else {
                Some((v + self.count) as u32)
            }
        }
    }

    /// where the span first..=last ends up: it grows when rows are
    /// inserted inside it and shrinks when some of it is deleted; none
    /// when all of it is deleted.
    pub fn span(&self, first: u32, last: u32) -> Option<(u32, u32)> {
        let (first, last) = (first.min(last), first.max(last));
        if self.count >= 0 {
            let new_first = self.position(first)?;
            let new_last = self.position(last).unwrap_or(match self.axis {
                Axis::Rows => MAX_ROWS - 1,
                Axis::Columns => MAX_COLS - 1,
            });
            return Some((new_first, new_last));
        }
        let removed_last = (i64::from(self.at) - self.count - 1) as u32;
        if first >= self.at && last <= removed_last {
            return None;
        }
        let new_first = self.position(first).unwrap_or(self.at);
        let new_last = match self.position(last) {
            Some(value) => value,
            None => self.at - 1,
        };
        Some((new_first, new_last))
    }
}

fn shift_pos_pair(start: CellPos, end: CellPos, shift: &Shift) -> Option<(CellPos, CellPos)> {
    let (mut start, mut end) = (start, end);
    match shift.axis {
        Axis::Rows => {
            let (a, b) = shift.span(start.row.min(end.row), start.row.max(end.row))?;
            if start.row <= end.row {
                start.row = a;
                end.row = b;
            } else {
                start.row = b;
                end.row = a;
            }
        }
        Axis::Columns => {
            let (a, b) = shift.span(start.col.min(end.col), start.col.max(end.col))?;
            if start.col <= end.col {
                start.col = a;
                end.col = b;
            } else {
                start.col = b;
                end.col = a;
            }
        }
    }
    Some((start, end))
}

/// an area after rows or columns moved; none means it was deleted.
pub fn shift_area(area: &Area, shift: &Shift) -> Option<Area> {
    Some(match *area {
        Area::Cell { at } => {
            let mut at = at;
            match shift.axis {
                Axis::Rows => at.row = shift.position(at.row)?,
                Axis::Columns => at.col = shift.position(at.col)?,
            }
            Area::Cell { at }
        }
        Area::Range { start, end } => {
            let (start, end) = shift_pos_pair(start, end, shift)?;
            Area::Range { start, end }
        }
        Area::Columns {
            first,
            last,
            first_abs,
            last_abs,
        } => match shift.axis {
            Axis::Rows => *area,
            Axis::Columns => {
                let (first, last) = shift.span(first, last)?;
                Area::Columns {
                    first,
                    last,
                    first_abs,
                    last_abs,
                }
            }
        },
        Area::Rows {
            first,
            last,
            first_abs,
            last_abs,
        } => match shift.axis {
            Axis::Columns => *area,
            Axis::Rows => {
                let (first, last) = shift.span(first, last)?;
                Area::Rows {
                    first,
                    last,
                    first_abs,
                    last_abs,
                }
            }
        },
    })
}

/// true when a reference written in a formula on `formula_sheet` points
/// at `target_sheet`.
fn points_at(reference: &Reference, formula_sheet: &str, target_sheet: &str) -> bool {
    match &reference.sheet {
        None => formula_sheet.eq_ignore_ascii_case(target_sheet),
        Some(SheetRef::One { name }) => name.eq_ignore_ascii_case(target_sheet),
        // a 3-d span keeps its shape; its cells move on every sheet it covers
        Some(SheetRef::Span { .. }) => false,
        Some(SheetRef::External { .. }) => false,
    }
}

/// the formula after rows or columns of `target_sheet` were inserted or
/// deleted. returns none when nothing changed, so callers can keep the
/// original text.
pub fn shift_formula(
    expr: &Expr,
    formula_sheet: &str,
    target_sheet: &str,
    shift: &Shift,
) -> Option<Expr> {
    let mut changed = false;
    let result = expr.clone().transform(&mut |node| match node {
        Expr::Ref { reference } if points_at(&reference, formula_sheet, target_sheet) => {
            match shift_area(&reference.area, shift) {
                Some(area) if area == reference.area => Expr::Ref { reference },
                Some(area) => {
                    changed = true;
                    Expr::Ref {
                        reference: Box::new(Reference {
                            sheet: reference.sheet,
                            area,
                        }),
                    }
                }
                None => {
                    changed = true;
                    Expr::Error {
                        value: ErrorValue::Ref,
                    }
                }
            }
        }
        other => other,
    });
    changed.then_some(result)
}

/// every reference to `old` renamed to `new` (sheet rename).
pub fn rename_sheet(expr: &Expr, old: &str, new: &str) -> Option<Expr> {
    let mut changed = false;
    let rename = |name: &mut String, changed: &mut bool| {
        if name.eq_ignore_ascii_case(old) {
            *name = new.to_string();
            *changed = true;
        }
    };
    let result = expr.clone().transform(&mut |node| match node {
        Expr::Ref { mut reference } => {
            match &mut reference.sheet {
                Some(SheetRef::One { name }) => rename(name, &mut changed),
                Some(SheetRef::Span { first, last }) => {
                    rename(first, &mut changed);
                    rename(last, &mut changed);
                }
                _ => {}
            }
            Expr::Ref { reference }
        }
        other => other,
    });
    changed.then_some(result)
}

/// references to a deleted sheet become #REF!. formulas on that sheet
/// itself are gone with it, so plain references are left alone.
pub fn remove_sheet(expr: &Expr, removed: &str) -> Option<Expr> {
    let mut changed = false;
    let result = expr.clone().transform(&mut |node| match node {
        Expr::Ref { reference } => match &reference.sheet {
            Some(SheetRef::One { name }) if name.eq_ignore_ascii_case(removed) => {
                changed = true;
                Expr::Error {
                    value: ErrorValue::Ref,
                }
            }
            _ => Expr::Ref { reference },
        },
        other => other,
    });
    changed.then_some(result)
}

/// references into a moved block follow it (cut and paste): any
/// reference entirely inside `from` on `target_sheet` moves by the offset.
pub fn follow_move(
    expr: &Expr,
    formula_sheet: &str,
    target_sheet: &str,
    from: crate::address::Range,
    rows: i64,
    cols: i64,
) -> Option<Expr> {
    let mut changed = false;
    let result = expr.clone().transform(&mut |node| match node {
        Expr::Ref { reference } if points_at(&reference, formula_sheet, target_sheet) => {
            let range = reference.area.to_range();
            if from.contains_range(&range)
                && !matches!(reference.area, Area::Columns { .. } | Area::Rows { .. })
            {
                let mut area = reference.area;
                let shift = |pos: &mut CellPos| {
                    pos.row = (i64::from(pos.row) + rows).clamp(0, i64::from(MAX_ROWS) - 1) as u32;
                    pos.col = (i64::from(pos.col) + cols).clamp(0, i64::from(MAX_COLS) - 1) as u32;
                };
                match &mut area {
                    Area::Cell { at } => shift(at),
                    Area::Range { start, end } => {
                        shift(start);
                        shift(end);
                    }
                    _ => {}
                }
                changed = true;
                Expr::Ref {
                    reference: Box::new(Reference {
                        sheet: reference.sheet,
                        area,
                    }),
                }
            } else {
                Expr::Ref { reference }
            }
        }
        other => other,
    });
    changed.then_some(result)
}

/// rewrites the text of a formula with `change`, returning the new text
/// only when something changed (unparsable formulas are left alone).
pub fn rewrite_text(text: &str, change: impl Fn(&Expr) -> Option<Expr>) -> Option<String> {
    let expr = super::parse(text).ok()?;
    change(&expr).map(|new| super::print::to_string(&new))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::{parse, print::to_string};

    fn shifted(text: &str, shift: Shift) -> String {
        let expr = parse(text).unwrap();
        shift_formula(&expr, "S", "S", &shift)
            .map(|e| to_string(&e))
            .unwrap_or_else(|| text.to_string())
    }

    #[test]
    fn copying_moves_relative_parts_only() {
        let expr = parse("A1+$B$2+C$3+$D4+SUM(A1:A3)+A:A").unwrap();
        assert_eq!(
            to_string(&translate(&expr, 2, 1)),
            "B3+$B$2+D$3+$D6+SUM(B3:B5)+B:B"
        );
        assert_eq!(to_string(&translate(&parse("A1").unwrap(), -1, 0)), "#REF!");
    }

    #[test]
    fn inserting_rows_shifts_and_grows() {
        let insert = Shift {
            axis: Axis::Rows,
            at: 4,
            count: 2,
        };
        assert_eq!(shifted("A3+A5+$A$10", insert), "A3+A7+$A$12");
        assert_eq!(shifted("SUM(A1:A10)", insert), "SUM(A1:A12)");
        assert_eq!(shifted("SUM(A5:A10)", insert), "SUM(A7:A12)");
        assert_eq!(shifted("A:A+B2", insert), "A:A+B2");
        assert_eq!(shifted("Other!A10", insert), "Other!A10");
    }

    #[test]
    fn deleting_rows_shrinks_or_breaks() {
        let delete = Shift {
            axis: Axis::Rows,
            at: 4,
            count: -2,
        }; // rows 5 and 6
        assert_eq!(shifted("A5", delete), "#REF!");
        assert_eq!(shifted("A7+A4", delete), "A5+A4");
        assert_eq!(shifted("SUM(A1:A10)", delete), "SUM(A1:A8)");
        assert_eq!(shifted("SUM(A5:A6)", delete), "SUM(#REF!)");
        assert_eq!(shifted("SUM(A6:A9)", delete), "SUM(A5:A7)");
        assert_eq!(shifted("SUM(A2:A5)", delete), "SUM(A2:A4)");
    }

    #[test]
    fn columns_and_sheets() {
        let insert = Shift {
            axis: Axis::Columns,
            at: 1,
            count: 1,
        };
        assert_eq!(
            shifted("A1+B1+SUM(A1:C1)+B:C", insert),
            "A1+C1+SUM(A1:D1)+C:D"
        );
        let renamed =
            rename_sheet(&parse("Old!A1+'old'!B2+A3").unwrap(), "Old", "New year").unwrap();
        assert_eq!(to_string(&renamed), "'New year'!A1+'New year'!B2+A3");
        let removed = remove_sheet(&parse("Gone!A1+A2").unwrap(), "Gone").unwrap();
        assert_eq!(to_string(&removed), "#REF!+A2");
    }

    /// a small deterministic random source, so the property test needs
    /// no extra library and always checks the same cases
    struct Random(u64);
    impl Random {
        fn next(&mut self, below: u32) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % u64::from(below)) as u32
        }
    }

    #[test]
    fn insert_then_delete_restores_every_reference() {
        let mut random = Random(0x9E37_79B9_7F4A_7C15);
        for _ in 0..2000 {
            let r1 = random.next(50);
            let r2 = r1 + random.next(20);
            let c1 = random.next(20);
            let text = format!(
                "{}+SUM({}:{})",
                crate::address::CellRef::new(r1, c1).to_a1(),
                crate::address::CellRef::new(r1, c1).to_a1(),
                crate::address::CellRef::new(r2, c1 + 1).to_a1()
            );
            let at = random.next(80);
            let count = i64::from(random.next(5) + 1);
            let axis = if random.next(2) == 0 {
                Axis::Rows
            } else {
                Axis::Columns
            };
            let grown = shifted(&text, Shift { axis, at, count });
            let back = shifted(
                &grown,
                Shift {
                    axis,
                    at,
                    count: -count,
                },
            );
            assert_eq!(
                back, text,
                "insert {count} at {at} {axis:?} then delete: {grown}"
            );
        }
    }

    #[test]
    fn deleting_keeps_every_surviving_cell_in_place() {
        // a cell reference either points at the same cell after deletion
        // (shifted), or becomes #REF! exactly when its cell was deleted
        let mut random = Random(42);
        for _ in 0..2000 {
            let row = random.next(100);
            let at = random.next(100);
            let count = i64::from(random.next(10) + 1);
            let shift = Shift {
                axis: Axis::Rows,
                at,
                count: -count,
            };
            let text = crate::address::CellRef::new(row, 0).to_a1();
            let result = shifted(&text, shift);
            let deleted = row >= at && i64::from(row) < i64::from(at) + count;
            if deleted {
                assert_eq!(result, "#REF!");
            } else {
                let expected = if row < at {
                    row
                } else {
                    (i64::from(row) - count) as u32
                };
                assert_eq!(result, crate::address::CellRef::new(expected, 0).to_a1());
            }
        }
    }
}
