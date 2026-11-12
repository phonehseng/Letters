//! moving, copying and filling cells. copies shift relative references
//! by how far they travel; a move leaves its own formulas alone and makes
//! every formula that pointed at the moved cells follow them, as excel's
//! cut and paste does.

use crate::address::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::error::SheetError;
use crate::formula::{self, refs};
use crate::model::{Cell, Workbook};

use super::cells::saved;
use super::rewrite::rewrite_all;
use super::series::{extend, SeriesItem};
use super::{sheet_check, snapshot, Applied, Changed, Operation, PasteKind};

fn destination(from: Range, to: CellRef) -> Result<Range, SheetError> {
    let end_row = u64::from(to.row) + u64::from(from.rows()) - 1;
    let end_col = u64::from(to.col) + u64::from(from.cols()) - 1;
    if end_row >= u64::from(MAX_ROWS) || end_col >= u64::from(MAX_COLS) {
        return Err(SheetError::Invalid(
            "That would go past the edge of the sheet.".into(),
        ));
    }
    Ok(Range::from_bounds(
        to.row,
        to.col,
        end_row as u32,
        end_col as u32,
    ))
}

/// a formula moved `rows` down and `cols` right, as text
pub fn translated(text: &str, rows: i64, cols: i64) -> String {
    match formula::parse(text) {
        Ok(expr) => formula::to_string(&refs::translate(&expr, rows, cols)),
        Err(_) => text.to_string(),
    }
}

/// the source cells with their offset from the range's corner
fn source_cells(book: &Workbook, sheet: usize, from: Range) -> Vec<(u32, u32, Cell)> {
    book.sheets[sheet]
        .cells
        .iter_range(from)
        .map(|(r, c, cell)| (r - from.start.row, c - from.start.col, cell.clone()))
        .collect()
}

pub fn copy_cells(
    book: &mut Workbook,
    sheet: usize,
    from: Range,
    to_sheet: usize,
    to: CellRef,
    paste: PasteKind,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    sheet_check(book, to_sheet)?;
    let target = destination(from, to)?;
    let inverse = saved(book, to_sheet, target);
    let cells = source_cells(book, sheet, from);
    let rows = i64::from(to.row) - i64::from(from.start.row);
    let cols = i64::from(to.col) - i64::from(from.start.col);
    clear_area(book, to_sheet, target, paste);
    for (dr, dc, cell) in cells {
        let (row, col) = (to.row + dr, to.col + dc);
        let existing = book.sheets[to_sheet]
            .cells
            .get(row, col)
            .cloned()
            .unwrap_or_default();
        let pasted = pasted_cell(cell, existing, paste, rows, cols);
        book.sheets[to_sheet].cells.set(row, col, pasted);
    }
    Ok(Applied {
        changed: Changed::range(to_sheet, target),
        inverse: vec![inverse],
    })
}

/// pasting over cells first empties what the paste replaces
fn clear_area(book: &mut Workbook, sheet: usize, area: Range, paste: PasteKind) {
    let positions: Vec<(u32, u32)> = book.sheets[sheet]
        .cells
        .iter_range(area)
        .map(|(r, c, _)| (r, c))
        .collect();
    for (row, col) in positions {
        let grid = &mut book.sheets[sheet].cells;
        match paste {
            PasteKind::All => {
                grid.remove(row, col);
            }
            PasteKind::Formats => {
                if let Some(cell) = grid.get_mut(row, col) {
                    cell.style = 0;
                }
            }
            PasteKind::Values | PasteKind::Formulas => {
                if let Some(cell) = grid.get_mut(row, col) {
                    cell.value = crate::model::Value::Blank;
                    cell.formula = None;
                }
            }
        }
    }
}

fn pasted_cell(mut cell: Cell, existing: Cell, paste: PasteKind, rows: i64, cols: i64) -> Cell {
    if let Some(formula) = cell.formula.as_mut() {
        formula.text = translated(&formula.text, rows, cols);
        // a pasted formula is a fresh one: shared groups and spills stay behind
        formula.kind = crate::model::FormulaKind::Dynamic { spill: None };
    }
    match paste {
        PasteKind::All => cell,
        PasteKind::Values => Cell {
            value: cell.value,
            style: existing.style,
            ..Cell::default()
        },
        PasteKind::Formulas => Cell {
            style: existing.style,
            ..cell
        },
        PasteKind::Formats => Cell {
            style: cell.style,
            ..existing
        },
    }
}

pub fn move_cells(
    book: &mut Workbook,
    sheet: usize,
    from: Range,
    to_sheet: usize,
    to: CellRef,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    sheet_check(book, to_sheet)?;
    let target = destination(from, to)?;
    let mut inverse = vec![snapshot(book, sheet)];
    if to_sheet != sheet {
        inverse.push(snapshot(book, to_sheet));
    }
    let cells = source_cells(book, sheet, from);
    let rows = i64::from(to.row) - i64::from(from.start.row);
    let cols = i64::from(to.col) - i64::from(from.start.col);
    if to_sheet == sheet {
        let name = book.sheets[sheet].name.clone();
        let changes = rewrite_all(book, &|expr, home, _| {
            refs::follow_move(expr, home, &name, from, rows, cols)
        });
        let elsewhere = changes
            .into_iter()
            .filter(|c| !matches!(c.place, super::rewrite::FormulaPlace::Cell { sheet: s, .. } if s == sheet))
            .collect();
        inverse.push(Operation::RestoreFormulas { changes: elsewhere });
    }
    // the moved cells' formulas were rewritten in place above; take the
    // rewritten versions along
    let moved: Vec<(u32, u32, Cell)> = cells
        .into_iter()
        .map(|(dr, dc, cell)| {
            let current = book.sheets[sheet]
                .cells
                .get(from.start.row + dr, from.start.col + dc)
                .cloned();
            (dr, dc, current.unwrap_or(cell))
        })
        .collect();
    for at in from.iter() {
        book.sheets[sheet].cells.remove(at.row, at.col);
    }
    clear_area(book, to_sheet, target, PasteKind::All);
    for (dr, dc, cell) in moved {
        book.sheets[to_sheet]
            .cells
            .set(to.row + dr, to.col + dc, cell);
    }
    let mut changed = Changed::range(sheet, from);
    changed.cells.extend(Changed::range(to_sheet, target).cells);
    Ok(Applied { changed, inverse })
}

/// the fill handle: `target` is the source plus the cells to fill
pub fn fill(
    book: &mut Workbook,
    sheet: usize,
    source: Range,
    target: Range,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    if !target.contains_range(&source) || target == source {
        return Err(SheetError::Invalid(
            "Drag past the selected cells to fill.".into(),
        ));
    }
    let inverse = saved(book, sheet, target);
    let vertical = target.cols() == source.cols();
    if vertical {
        for col in source.start.col..=source.end.col {
            let line: Vec<CellRef> = (source.start.row..=source.end.row)
                .map(|r| CellRef::new(r, col))
                .collect();
            let down: Vec<CellRef> = (source.end.row + 1..=target.end.row)
                .map(|r| CellRef::new(r, col))
                .collect();
            let up: Vec<CellRef> = (target.start.row..source.start.row)
                .rev()
                .map(|r| CellRef::new(r, col))
                .collect();
            fill_line(book, sheet, &line, &down, false);
            fill_line(book, sheet, &line, &up, true);
        }
    } else {
        for row in source.start.row..=source.end.row {
            let line: Vec<CellRef> = (source.start.col..=source.end.col)
                .map(|c| CellRef::new(row, c))
                .collect();
            let right: Vec<CellRef> = (source.end.col + 1..=target.end.col)
                .map(|c| CellRef::new(row, c))
                .collect();
            let left: Vec<CellRef> = (target.start.col..source.start.col)
                .rev()
                .map(|c| CellRef::new(row, c))
                .collect();
            fill_line(book, sheet, &line, &right, false);
            fill_line(book, sheet, &line, &left, true);
        }
    }
    Ok(Applied {
        changed: Changed::range(sheet, target),
        inverse: vec![inverse],
    })
}

/// fills `targets` from the `line` of source cells; `backwards` fills
/// up or left, continuing the series the other way
fn fill_line(
    book: &mut Workbook,
    sheet: usize,
    line: &[CellRef],
    targets: &[CellRef],
    backwards: bool,
) {
    if targets.is_empty() {
        return;
    }
    let cells: Vec<Cell> = line
        .iter()
        .map(|at| {
            book.sheets[sheet]
                .cells
                .cell_at(*at)
                .cloned()
                .unwrap_or_default()
        })
        .collect();
    let ordered: Vec<&Cell> = if backwards {
        cells.iter().rev().collect()
    } else {
        cells.iter().collect()
    };
    let has_formula = cells.iter().any(|c| c.formula.is_some());
    let items: Vec<SeriesItem> = ordered
        .iter()
        .map(|cell| SeriesItem {
            value: cell.value.clone(),
            is_date: is_date_style(book, cell.style),
        })
        .collect();
    let series = if has_formula {
        Vec::new()
    } else {
        extend(&items, targets.len(), book.date_system)
    };
    for (index, at) in targets.iter().enumerate() {
        let source_index = index % ordered.len();
        let source = ordered[source_index];
        let source_at = if backwards {
            line[line.len() - 1 - source_index]
        } else {
            line[source_index]
        };
        let mut cell = source.clone();
        if let Some(formula) = cell.formula.as_mut() {
            let rows = i64::from(at.row) - i64::from(source_at.row);
            let cols = i64::from(at.col) - i64::from(source_at.col);
            formula.text = translated(&formula.text, rows, cols);
            formula.kind = crate::model::FormulaKind::Dynamic { spill: None };
        } else {
            cell.value = series[index].clone();
        }
        book.sheets[sheet].cells.set(at.row, at.col, cell);
    }
}

fn is_date_style(book: &Workbook, style: u32) -> bool {
    let code = &book.styles.get(style).number_format;
    crate::numfmt::parse::parse(code)
        .first()
        .is_some_and(|section| section.is_date())
}
