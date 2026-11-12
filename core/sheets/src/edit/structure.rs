//! inserting and deleting rows and columns. cells move, and so does
//! everything that names cells: formulas on every sheet, defined names,
//! merged ranges, conditional formats, validations, links, notes,
//! tables, filters, print settings and drawn objects. references to
//! deleted cells become #REF!, as in excel.

use crate::address::{CellRef, Range, MAX_COLS, MAX_ROWS};
use crate::error::SheetError;
use crate::formula::refs::{shift_formula, Axis, Shift};
use crate::model::{Sheet, TableColumn, Workbook};

use super::rewrite::{rewrite_all, FormulaPlace};
use super::{sheet_check, snapshot, Applied, Changed, Operation};

pub fn insert(
    book: &mut Workbook,
    sheet: usize,
    axis: Axis,
    at: u32,
    count: u32,
) -> Result<Applied, SheetError> {
    shift(
        book,
        sheet,
        Shift {
            axis,
            at,
            count: i64::from(count),
        },
    )
}

pub fn delete(
    book: &mut Workbook,
    sheet: usize,
    axis: Axis,
    at: u32,
    count: u32,
) -> Result<Applied, SheetError> {
    shift(
        book,
        sheet,
        Shift {
            axis,
            at,
            count: -i64::from(count),
        },
    )
}

fn shift(book: &mut Workbook, sheet: usize, shift: Shift) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let limit = match shift.axis {
        Axis::Rows => MAX_ROWS,
        Axis::Columns => MAX_COLS,
    };
    if shift.count == 0 || shift.at >= limit {
        return Err(SheetError::Invalid(
            "There is nothing to insert or delete there.".into(),
        ));
    }
    let undo_sheet = snapshot(book, sheet);
    let target = book.sheets[sheet].name.clone();
    let changes = rewrite_all(book, &|expr, home, _| {
        shift_formula(expr, home, &target, &shift)
    });
    move_cells(&mut book.sheets[sheet], &shift);
    shift_sheet_features(&mut book.sheets[sheet], &shift);

    // the target sheet and the names come back whole; formulas on other
    // sheets are put back one by one
    let elsewhere: Vec<_> = changes
        .into_iter()
        .filter(|change| match change.place {
            FormulaPlace::Cell { sheet: s, .. }
            | FormulaPlace::Conditional { sheet: s, .. }
            | FormulaPlace::Validation { sheet: s, .. }
            | FormulaPlace::TableColumn { sheet: s, .. } => s != sheet,
            FormulaPlace::Name { .. } => false,
        })
        .collect();
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![
            undo_sheet,
            Operation::RestoreFormulas { changes: elsewhere },
        ],
    })
}

fn move_cells(sheet: &mut Sheet, shift: &Shift) {
    match shift.axis {
        Axis::Rows => {
            sheet
                .cells
                .remap(|row, col| shift.position(row).map(|row| (row, col)));
            let rows = std::mem::take(&mut sheet.rows);
            sheet.rows = rows
                .into_iter()
                .filter_map(|(row, info)| Some((shift.position(row)?, info)))
                .collect();
        }
        Axis::Columns => {
            sheet
                .cells
                .remap(|row, col| shift.position(col).map(|col| (row, col)));
            let columns = std::mem::take(&mut sheet.columns);
            sheet.columns = columns
                .into_iter()
                .filter_map(|mut info| {
                    let (first, last) = shift.span(info.first, info.last)?;
                    info.first = first;
                    info.last = last;
                    Some(info)
                })
                .collect();
        }
    }
}

/// a range after the shift; none when all of it was deleted
pub fn shift_range(range: Range, shift: &Shift) -> Option<Range> {
    let whole_line = match shift.axis {
        Axis::Rows => range.is_whole_columns(),
        Axis::Columns => range.is_whole_rows(),
    };
    if whole_line {
        return Some(range);
    }
    match shift.axis {
        Axis::Rows => {
            let (first, last) = shift.span(range.start.row, range.end.row)?;
            Some(Range::from_bounds(
                first,
                range.start.col,
                last,
                range.end.col,
            ))
        }
        Axis::Columns => {
            let (first, last) = shift.span(range.start.col, range.end.col)?;
            Some(Range::from_bounds(
                range.start.row,
                first,
                range.end.row,
                last,
            ))
        }
    }
}

fn shift_cell(at: CellRef, shift: &Shift) -> Option<CellRef> {
    match shift.axis {
        Axis::Rows => Some(CellRef::new(shift.position(at.row)?, at.col)),
        Axis::Columns => Some(CellRef::new(at.row, shift.position(at.col)?)),
    }
}

fn shift_ranges(ranges: &mut Vec<Range>, shift: &Shift) {
    *ranges = ranges
        .iter()
        .filter_map(|range| shift_range(*range, shift))
        .collect();
}

fn shift_sheet_features(sheet: &mut Sheet, shift: &Shift) {
    shift_ranges(&mut sheet.merged, shift);
    for format in &mut sheet.conditional_formats {
        shift_ranges(&mut format.ranges, shift);
    }
    sheet
        .conditional_formats
        .retain(|format| !format.ranges.is_empty());
    for rule in &mut sheet.validations {
        shift_ranges(&mut rule.ranges, shift);
    }
    sheet.validations.retain(|rule| !rule.ranges.is_empty());
    sheet
        .hyperlinks
        .retain_mut(|link| match shift_range(link.range, shift) {
            Some(range) => {
                link.range = range;
                true
            }
            None => false,
        });
    sheet.comments.retain_mut(
        |note| match shift_cell(CellRef::new(note.row, note.col), shift) {
            Some(at) => {
                note.row = at.row;
                note.col = at.col;
                true
            }
            None => false,
        },
    );
    shift_tables(sheet, shift);
    if let Some(filter) = sheet.auto_filter.as_mut() {
        match shift_range(filter.range, shift) {
            Some(range) => filter.range = range,
            None => sheet.auto_filter = None,
        }
    }
    shift_print(sheet, shift);
    for object in &mut sheet.drawings {
        shift_anchor(object, shift);
    }
}

/// tables grow or shrink; columns inserted inside a table become new
/// table columns named as excel names them ("Column1")
fn shift_tables(sheet: &mut Sheet, shift: &Shift) {
    sheet.tables.retain_mut(|table| {
        let old = table.range;
        let Some(range) = shift_range(old, shift) else {
            return false;
        };
        table.range = range;
        if shift.axis == Axis::Columns {
            let old_width = old.cols() as i64;
            let new_width = range.cols() as i64;
            let relative_at = i64::from(shift.at) - i64::from(old.start.col);
            if new_width > old_width && (0..=old_width).contains(&relative_at) {
                for n in 0..(new_width - old_width) {
                    let name = (1..)
                        .map(|k| format!("Column{k}"))
                        .find(|name| table.column_index(name).is_none())
                        .unwrap_or_default();
                    table.columns.insert(
                        (relative_at + n) as usize,
                        TableColumn {
                            name,
                            ..TableColumn::default()
                        },
                    );
                }
            } else if new_width < old_width {
                let first = relative_at.max(0) as usize;
                let removed = (old_width - new_width) as usize;
                let end = (first + removed).min(table.columns.len());
                table.columns.drain(first.min(end)..end);
            }
        }
        true
    });
}

fn shift_print(sheet: &mut Sheet, shift: &Shift) {
    shift_ranges(&mut sheet.print.area, shift);
    let breaks = match shift.axis {
        Axis::Rows => &mut sheet.print.row_breaks,
        Axis::Columns => &mut sheet.print.col_breaks,
    };
    *breaks = breaks.iter().filter_map(|b| shift.position(*b)).collect();
    let titles = match shift.axis {
        Axis::Rows => &mut sheet.print.title_rows,
        Axis::Columns => &mut sheet.print.title_cols,
    };
    *titles = titles.and_then(|(first, last)| shift.span(first, last));
}

fn shift_anchor(object: &mut crate::model::DrawingObject, shift: &Shift) {
    use crate::model::DrawingObject;
    let anchor = match object {
        DrawingObject::Chart { anchor, .. }
        | DrawingObject::Image { anchor, .. }
        | DrawingObject::Shape { anchor, .. } => anchor,
    };
    let moved = |value: u32| shift.position(value).unwrap_or(shift.at);
    match shift.axis {
        Axis::Rows => {
            anchor.from_row = moved(anchor.from_row);
            anchor.to_row = moved(anchor.to_row);
        }
        Axis::Columns => {
            anchor.from_col = moved(anchor.from_col);
            anchor.to_col = moved(anchor.to_col);
        }
    }
}
