//! column widths, row heights and hiding, with their undo.
//!
//! the engine's edit operations (core/sheets `edit::Operation`) don't
//! cover sizes, so this is the one kind of edit the shell makes itself.
//! a change sets an exact state for each column or row and returns the
//! states it replaced, so undo is applying that answer.

use letters_sheets::{Sheet, Workbook};
use serde::{Deserialize, Serialize};

/// one column's or row's size and whether it is hidden. `size` is in
/// excel's units: characters for columns, points for rows; none is the
/// sheet's default.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineState {
    pub index: u32,
    pub size: Option<f64>,
    pub hidden: bool,
}

/// which lines of a sheet a change is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    Rows,
    Columns,
}

/// sets each listed line to its state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutChange {
    pub sheet: usize,
    pub axis: Axis,
    pub lines: Vec<LineState>,
}

/// applies a change and returns the change that undoes it.
pub fn apply_layout(book: &mut Workbook, change: &LayoutChange) -> Result<LayoutChange, String> {
    let sheet = book
        .sheets
        .get_mut(change.sheet)
        .ok_or("That sheet doesn’t exist any more.")?;
    let mut previous = Vec::with_capacity(change.lines.len());
    for line in &change.lines {
        previous.push(read_line(sheet, change.axis, line.index));
        write_line(sheet, change.axis, line);
    }
    // put back in reverse, so a line listed twice ends as it started
    previous.reverse();
    Ok(LayoutChange {
        sheet: change.sheet,
        axis: change.axis,
        lines: previous,
    })
}

fn read_line(sheet: &Sheet, axis: Axis, index: u32) -> LineState {
    match axis {
        Axis::Columns => {
            let info = sheet
                .columns
                .iter()
                .find(|info| index >= info.first && index <= info.last);
            LineState {
                index,
                size: info.and_then(|info| info.width),
                hidden: info.is_some_and(|info| info.hidden),
            }
        }
        Axis::Rows => {
            let info = sheet.rows.get(&index);
            LineState {
                index,
                size: info.and_then(|info| info.height),
                hidden: info.is_some_and(|info| info.hidden),
            }
        }
    }
}

fn write_line(sheet: &mut Sheet, axis: Axis, line: &LineState) {
    match axis {
        Axis::Columns => sheet.edit_column(line.index, |info| {
            info.width = line.size;
            info.custom_width = line.size.is_some();
            info.hidden = line.hidden;
        }),
        Axis::Rows => {
            let info = sheet.rows.entry(line.index).or_default();
            info.height = line.size;
            info.custom_height = line.size.is_some();
            info.hidden = line.hidden;
            if info.is_default() {
                sheet.rows.remove(&line.index);
            }
        }
    }
}

/// the change that gives `indices` one size (keeping whether each is hidden).
pub fn resize(
    book: &Workbook,
    sheet: usize,
    axis: Axis,
    indices: &[u32],
    size: Option<f64>,
) -> Result<LayoutChange, String> {
    let target = book
        .sheets
        .get(sheet)
        .ok_or("That sheet doesn’t exist any more.")?;
    let lines = indices
        .iter()
        .map(|&index| LineState {
            size,
            ..read_line(target, axis, index)
        })
        .collect();
    Ok(LayoutChange { sheet, axis, lines })
}

/// the change that hides or shows `indices` (keeping their sizes).
pub fn set_hidden(
    book: &Workbook,
    sheet: usize,
    axis: Axis,
    indices: &[u32],
    hidden: bool,
) -> Result<LayoutChange, String> {
    let target = book
        .sheets
        .get(sheet)
        .ok_or("That sheet doesn’t exist any more.")?;
    let lines = indices
        .iter()
        .map(|&index| LineState {
            hidden,
            ..read_line(target, axis, index)
        })
        .collect();
    Ok(LayoutChange { sheet, axis, lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widening_a_column_inside_a_run_and_undoing_restores_the_run() {
        let mut book = Workbook::new();
        book.sheets[0]
            .columns
            .push(letters_sheets::model::ColumnInfo {
                first: 0,
                last: 4,
                width: Some(12.0),
                custom_width: true,
                ..Default::default()
            });
        let change = resize(&book, 0, Axis::Columns, &[2], Some(30.0)).unwrap();
        let undo = apply_layout(&mut book, &change).unwrap();
        assert_eq!(book.sheets[0].column_width(2), 30.0);
        assert_eq!(book.sheets[0].column_width(1), 12.0);
        apply_layout(&mut book, &undo).unwrap();
        assert_eq!(book.sheets[0].column_width(2), 12.0);
    }

    #[test]
    fn hiding_rows_keeps_their_height_and_showing_them_leaves_no_trace() {
        let mut book = Workbook::new();
        let change = set_hidden(&book, 0, Axis::Rows, &[3, 4], true).unwrap();
        let undo = apply_layout(&mut book, &change).unwrap();
        assert!(book.sheets[0].row_hidden(3) && book.sheets[0].row_hidden(4));
        apply_layout(&mut book, &undo).unwrap();
        assert!(book.sheets[0].rows.is_empty());
    }

    #[test]
    fn a_row_height_set_back_to_default_is_removed() {
        let mut book = Workbook::new();
        let tall = resize(&book, 0, Axis::Rows, &[0], Some(40.0)).unwrap();
        apply_layout(&mut book, &tall).unwrap();
        assert_eq!(book.sheets[0].row_height(0), 40.0);
        let back = resize(&book, 0, Axis::Rows, &[0], None).unwrap();
        apply_layout(&mut book, &back).unwrap();
        assert!(book.sheets[0].rows.is_empty());
    }
}
