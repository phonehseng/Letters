//! sorting a range by one or more columns, filtering rows, and removing
//! duplicate rows. sorting moves whole rows of the range; formulas in
//! them shift their relative references with the row, as excel's sort does.

use serde::{Deserialize, Serialize};

use crate::address::{CellRef, Range};
use crate::error::SheetError;
use crate::model::{AutoFilter, Cell, ColumnFilter, Value, Workbook};

use super::clipboard::translated;
use super::{sheet_check, snapshot, Applied, Changed};

/// one sort key: a column of the range (0 is its first) and the direction
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortKey {
    pub column: u32,
    pub descending: bool,
}

fn data_rows(range: Range, has_header: bool) -> Option<(u32, u32)> {
    let first = range.start.row + u32::from(has_header);
    (first <= range.end.row).then_some((first, range.end.row))
}

/// the cells of one row of the range, by column offset
fn row_cells(book: &Workbook, sheet: usize, row: u32, range: Range) -> Vec<Option<Cell>> {
    (range.start.col..=range.end.col)
        .map(|col| book.sheets[sheet].cells.get(row, col).cloned())
        .collect()
}

fn value_of(cells: &[Option<Cell>], column: u32) -> Value {
    cells
        .get(column as usize)
        .and_then(|c| c.as_ref())
        .map(|c| c.value.clone())
        .unwrap_or_default()
}

pub fn sort(
    book: &mut Workbook,
    sheet: usize,
    range: Range,
    keys: &[SortKey],
    has_header: bool,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let Some((first, last)) = data_rows(range, has_header) else {
        return Err(SheetError::Invalid("There are no rows to sort.".into()));
    };
    if keys.is_empty() || keys.iter().any(|key| key.column >= range.cols()) {
        return Err(SheetError::Invalid(
            "Choose a column inside the range to sort by.".into(),
        ));
    }
    let undo = snapshot(book, sheet);
    let mut rows: Vec<(u32, Vec<Option<Cell>>)> = (first..=last)
        .map(|row| (row, row_cells(book, sheet, row, range)))
        .collect();
    rows.sort_by(|(_, a), (_, b)| {
        for key in keys {
            let order = crate::calc::functions::sort_order(
                &value_of(a, key.column),
                &value_of(b, key.column),
                key.descending,
            );
            if order != std::cmp::Ordering::Equal {
                return order;
            }
        }
        std::cmp::Ordering::Equal
    });
    for (offset, (old_row, cells)) in rows.into_iter().enumerate() {
        let new_row = first + offset as u32;
        for (index, cell) in cells.into_iter().enumerate() {
            let col = range.start.col + index as u32;
            match cell {
                Some(mut cell) => {
                    if let Some(formula) = cell.formula.as_mut() {
                        formula.text =
                            translated(&formula.text, i64::from(new_row) - i64::from(old_row), 0);
                    }
                    book.sheets[sheet].cells.set(new_row, col, cell);
                }
                None => {
                    book.sheets[sheet].cells.remove(new_row, col);
                }
            }
        }
    }
    Ok(Applied {
        changed: Changed::range(
            sheet,
            Range::from_bounds(first, range.start.col, last, range.end.col),
        ),
        inverse: vec![undo],
    })
}

/// what a cell shows, for comparing with filter choices
fn shown(book: &Workbook, cell: Option<&Cell>) -> String {
    match cell {
        None => String::new(),
        Some(cell) => {
            let code = &book.styles.get(cell.style).number_format;
            crate::numfmt::format_value(
                &cell.value,
                code,
                &crate::numfmt::Locale::default(),
                book.date_system,
            )
            .text
        }
    }
}

/// shows only the rows whose value in `column` is one of `keep` (as
/// shown, ignoring case; "" keeps empty cells), hiding the others
pub fn filter(
    book: &mut Workbook,
    sheet: usize,
    range: Range,
    column: u32,
    keep: &[String],
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let undo = snapshot(book, sheet);
    let Some((first, last)) = data_rows(range, true) else {
        return Err(SheetError::Invalid("There are no rows to filter.".into()));
    };
    let wanted: Vec<String> = keep.iter().map(|k| k.to_lowercase()).collect();
    for row in first..=last {
        let text = shown(
            book,
            book.sheets[sheet].cells.get(row, range.start.col + column),
        )
        .to_lowercase();
        let hidden = !wanted.contains(&text);
        book.sheets[sheet].rows.entry(row).or_default().hidden = hidden;
    }
    book.sheets[sheet].rows.retain(|_, info| !info.is_default());
    let values: Vec<String> = keep.iter().filter(|k| !k.is_empty()).cloned().collect();
    let blank = keep.iter().any(|k| k.is_empty());
    book.sheets[sheet].auto_filter = Some(AutoFilter {
        range,
        columns: vec![(column, ColumnFilter::Values { values, blank })],
        sort: None,
    });
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![undo],
    })
}

pub fn clear_filter(book: &mut Workbook, sheet: usize) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let undo = snapshot(book, sheet);
    if let Some(filter) = book.sheets[sheet].auto_filter.take() {
        for row in filter.range.start.row..=filter.range.end.row {
            if let Some(info) = book.sheets[sheet].rows.get_mut(&row) {
                info.hidden = false;
            }
        }
        book.sheets[sheet].rows.retain(|_, info| !info.is_default());
    }
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![undo],
    })
}

/// removes later rows whose `columns` (offsets; all when empty) match an
/// earlier row, ignoring case as excel does, and closes the gaps
pub fn remove_duplicates(
    book: &mut Workbook,
    sheet: usize,
    range: Range,
    columns: &[u32],
    has_header: bool,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let Some((first, last)) = data_rows(range, has_header) else {
        return Err(SheetError::Invalid("There are no rows to check.".into()));
    };
    let undo = snapshot(book, sheet);
    let keys: Vec<u32> = if columns.is_empty() {
        (0..range.cols()).collect()
    } else {
        columns.to_vec()
    };
    let mut kept: Vec<Vec<Option<Cell>>> = Vec::new();
    for row in first..=last {
        let cells = row_cells(book, sheet, row, range);
        let duplicate = kept.iter().any(|earlier| {
            keys.iter().all(|key| {
                let (a, b) = (value_of(earlier, *key), value_of(&cells, *key));
                crate::calc::value::compare(&a, &b) == std::cmp::Ordering::Equal
                    && std::mem::discriminant(&a) == std::mem::discriminant(&b)
            })
        });
        if !duplicate {
            kept.push(cells);
        }
    }
    for row in first..=last {
        for col in range.start.col..=range.end.col {
            book.sheets[sheet].cells.remove(row, col);
        }
    }
    for (offset, cells) in kept.into_iter().enumerate() {
        for (index, cell) in cells.into_iter().enumerate() {
            if let Some(cell) = cell {
                let at = CellRef::new(first + offset as u32, range.start.col + index as u32);
                book.sheets[sheet].cells.set(at.row, at.col, cell);
            }
        }
    }
    Ok(Applied {
        changed: Changed::range(sheet, range),
        inverse: vec![undo],
    })
}
