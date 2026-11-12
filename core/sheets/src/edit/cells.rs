//! edits to cells in place: typing, putting cells back, clearing,
//! merging, styles and the format painter, freezing panes.

use crate::address::Range;
use crate::error::SheetError;
use crate::model::{Cell, Formula, FrozenPanes, Value, Workbook};

use super::{sheet_check, snapshot, Applied, Changed, ClearKind, Operation};

/// the cells of a range as they are now, for undo
pub fn saved(book: &Workbook, sheet: usize, range: Range) -> Operation {
    let grid = &book.sheets[sheet].cells;
    let mut cells: Vec<(u32, u32, Option<Cell>)> = Vec::new();
    for (row, col, cell) in grid.iter_range(range) {
        cells.push((row, col, Some(cell.clone())));
    }
    // cells that do not exist now must be removed again on undo
    if range.cell_count() <= 100_000 {
        for at in range.iter() {
            if grid.get(at.row, at.col).is_none() {
                cells.push((at.row, at.col, None));
            }
        }
    }
    Operation::PutCells { sheet, cells }
}

pub fn set_cell(
    book: &mut Workbook,
    sheet: usize,
    row: u32,
    col: u32,
    input: &str,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let typed = super::input::read_input(input, book.date_system)?;
    let inverse = saved(
        book,
        sheet,
        Range::cell(crate::address::CellRef::new(row, col)),
    );
    let style = book.sheets[sheet]
        .cells
        .get(row, col)
        .map(|c| c.style)
        .unwrap_or(0);
    let style = match &typed.number_format {
        // keep a format the person chose; give one only to a General cell
        Some(code) if book.styles.get(style).number_format == "General" => book
            .styles
            .derive(style, |s| s.number_format = code.clone()),
        _ => style,
    };
    let cell = book.sheets[sheet].cells.entry(row, col);
    cell.style = style;
    cell.not_recalculated = false;
    cell.extra = None;
    match typed.formula {
        Some(text) => {
            cell.formula = Some(Formula::new(text));
            cell.value = Value::Blank;
        }
        None => {
            cell.formula = None;
            cell.value = typed.value;
        }
    }
    if cell.is_empty() && cell.style == 0 {
        book.sheets[sheet].cells.remove(row, col);
    }
    Ok(Applied {
        changed: Changed {
            cells: vec![(sheet, row, col)],
            structure: false,
        },
        inverse: vec![inverse],
    })
}

pub fn put_cells(
    book: &mut Workbook,
    sheet: usize,
    cells: &[(u32, u32, Option<Cell>)],
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let mut undo = Vec::with_capacity(cells.len());
    let mut changed = Vec::with_capacity(cells.len());
    let grid = &mut book.sheets[sheet].cells;
    for (row, col, cell) in cells {
        let previous = match cell {
            Some(cell) => grid.set(*row, *col, cell.clone()),
            None => grid.remove(*row, *col),
        };
        undo.push((*row, *col, previous));
        changed.push((sheet, *row, *col));
    }
    Ok(Applied {
        changed: Changed {
            cells: changed,
            structure: false,
        },
        inverse: vec![Operation::PutCells { sheet, cells: undo }],
    })
}

pub fn clear(
    book: &mut Workbook,
    sheet: usize,
    range: Range,
    what: ClearKind,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let inverse = saved(book, sheet, range);
    let positions: Vec<(u32, u32)> = book.sheets[sheet]
        .cells
        .iter_range(range)
        .map(|(r, c, _)| (r, c))
        .collect();
    let grid = &mut book.sheets[sheet].cells;
    for (row, col) in positions {
        match what {
            ClearKind::All => {
                grid.remove(row, col);
            }
            ClearKind::Contents => {
                let keep_style = grid.get(row, col).map(|c| c.style).unwrap_or(0);
                if keep_style == 0 {
                    grid.remove(row, col);
                } else {
                    grid.set(
                        row,
                        col,
                        Cell {
                            style: keep_style,
                            ..Cell::default()
                        },
                    );
                }
            }
            ClearKind::Formats => {
                if let Some(cell) = grid.get_mut(row, col) {
                    cell.style = 0;
                }
            }
        }
    }
    Ok(Applied {
        changed: Changed::range(sheet, range),
        inverse: vec![inverse],
    })
}

/// merging keeps only the top-left value, as excel does (after warning)
pub fn merge(book: &mut Workbook, sheet: usize, range: Range) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let undo = snapshot(book, sheet);
    let target = &mut book.sheets[sheet];
    target
        .merged
        .retain(|existing| !existing.intersects(&range));
    let others: Vec<(u32, u32)> = target
        .cells
        .iter_range(range)
        .map(|(r, c, _)| (r, c))
        .filter(|(r, c)| (*r, *c) != (range.start.row, range.start.col))
        .collect();
    for (row, col) in others {
        if let Some(cell) = target.cells.get_mut(row, col) {
            cell.value = Value::Blank;
            cell.formula = None;
        }
    }
    if range.cell_count() > 1 {
        target.merged.push(range);
    }
    Ok(Applied {
        changed: Changed::range(sheet, range),
        inverse: vec![undo],
    })
}

pub fn unmerge(book: &mut Workbook, sheet: usize, range: Range) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let undo = snapshot(book, sheet);
    book.sheets[sheet]
        .merged
        .retain(|existing| !existing.intersects(&range));
    Ok(Applied {
        changed: Changed::range(sheet, range),
        inverse: vec![undo],
    })
}

pub fn set_style(
    book: &mut Workbook,
    sheet: usize,
    range: Range,
    style: u32,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let inverse = saved(book, sheet, range);
    for at in range.iter() {
        book.sheets[sheet].cells.entry(at.row, at.col).style = style;
    }
    Ok(Applied {
        changed: Changed::range(sheet, range),
        inverse: vec![inverse],
    })
}

/// the format painter: the source's styles repeated over the target
pub fn copy_formats(
    book: &mut Workbook,
    sheet: usize,
    from: Range,
    to: Range,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let inverse = saved(book, sheet, to);
    let styles: Vec<u32> = from
        .iter()
        .map(|at| {
            book.sheets[sheet]
                .cells
                .get(at.row, at.col)
                .map(|c| c.style)
                .unwrap_or(0)
        })
        .collect();
    let (rows, cols) = (from.rows(), from.cols());
    for at in to.iter() {
        let r = (at.row - to.start.row) % rows;
        let c = (at.col - to.start.col) % cols;
        let style = styles[(r * cols + c) as usize];
        let cells = &mut book.sheets[sheet].cells;
        if style == 0 && cells.get(at.row, at.col).is_none_or(|cell| cell.is_empty()) {
            cells.remove(at.row, at.col);
        } else {
            cells.entry(at.row, at.col).style = style;
        }
    }
    Ok(Applied {
        changed: Changed::range(sheet, to),
        inverse: vec![inverse],
    })
}

pub fn freeze(
    book: &mut Workbook,
    sheet: usize,
    rows: u32,
    cols: u32,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let previous = book.sheets[sheet].view.frozen.unwrap_or_default();
    let frozen = (rows > 0 || cols > 0).then_some(FrozenPanes { rows, cols });
    book.sheets[sheet].view.frozen = frozen;
    Ok(Applied {
        changed: Changed::default(),
        inverse: vec![Operation::Freeze {
            sheet,
            rows: previous.rows,
            cols: previous.cols,
        }],
    })
}
