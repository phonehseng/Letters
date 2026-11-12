//! editing a workbook as pure operations. `apply` changes the workbook
//! and returns which cells changed (for the grid to redraw and the
//! engine to recalculate) and the operations that undo it, so undo is
//! applying the inverse and redo is applying the operation again.

mod cells;
mod clipboard;
pub mod input;
mod rewrite;
pub mod series;
mod sheets;
mod sort;
mod structure;
mod tables;
mod text;

use serde::{Deserialize, Serialize};

use crate::address::{CellRef, Range};
use crate::calc::CellKey;
use crate::error::SheetError;
use crate::formula::refs::Axis;
use crate::model::{Cell, DefinedName, Sheet, Workbook};

pub use rewrite::FormulaChange;
pub use sort::SortKey;
pub use text::{find, FindOptions, Found};

/// what to clear
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClearKind {
    All,
    Contents,
    Formats,
}

/// how to paste a copy
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PasteKind {
    All,
    Values,
    Formats,
    Formulas,
}

/// one edit. the json form is `{"op": "insertRows", "sheet": 0, ...}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Operation {
    /// what a person typed into a cell: "=SUM(A1:A3)", "12%", "3/1/2024", "'007"
    SetCell {
        sheet: usize,
        row: u32,
        col: u32,
        input: String,
    },
    /// puts cells back exactly (undo uses it); none removes a cell
    PutCells {
        sheet: usize,
        cells: Vec<(u32, u32, Option<Cell>)>,
    },
    Clear {
        sheet: usize,
        range: Range,
        what: ClearKind,
    },
    InsertRows {
        sheet: usize,
        at: u32,
        count: u32,
    },
    DeleteRows {
        sheet: usize,
        at: u32,
        count: u32,
    },
    InsertColumns {
        sheet: usize,
        at: u32,
        count: u32,
    },
    DeleteColumns {
        sheet: usize,
        at: u32,
        count: u32,
    },
    /// cut and paste: formulas pointing into the moved cells follow them
    Move {
        sheet: usize,
        from: Range,
        to_sheet: usize,
        to: CellRef,
    },
    /// copy and paste: relative references shift, the source stays
    Copy {
        sheet: usize,
        from: Range,
        to_sheet: usize,
        to: CellRef,
        paste: PasteKind,
    },
    /// drag the fill handle: series where the source makes one, copies otherwise
    Fill {
        sheet: usize,
        source: Range,
        target: Range,
    },
    Sort {
        sheet: usize,
        range: Range,
        keys: Vec<SortKey>,
        has_header: bool,
    },
    /// hides the rows of a range whose column fails the filter
    Filter {
        sheet: usize,
        range: Range,
        column: u32,
        keep: Vec<String>,
    },
    /// shows every row a filter hid and removes the filter
    ClearFilter {
        sheet: usize,
    },
    Replace {
        sheet: Option<usize>,
        find: String,
        with: String,
        options: FindOptions,
    },
    Merge {
        sheet: usize,
        range: Range,
    },
    Unmerge {
        sheet: usize,
        range: Range,
    },
    /// format painter: the styles of `from`, repeated over `to`
    CopyFormats {
        sheet: usize,
        from: Range,
        to: Range,
    },
    SetStyle {
        sheet: usize,
        range: Range,
        style: u32,
    },
    ToTable {
        sheet: usize,
        range: Range,
        has_header: bool,
    },
    TableToRange {
        sheet: usize,
        table: String,
    },
    AddSheet {
        index: usize,
        name: String,
    },
    RenameSheet {
        sheet: usize,
        name: String,
    },
    MoveSheet {
        from: usize,
        to: usize,
    },
    DuplicateSheet {
        sheet: usize,
    },
    DeleteSheet {
        sheet: usize,
    },
    Freeze {
        sheet: usize,
        rows: u32,
        cols: u32,
    },
    TextToColumns {
        sheet: usize,
        range: Range,
        delimiter: char,
    },
    RemoveDuplicates {
        sheet: usize,
        range: Range,
        columns: Vec<u32>,
        has_header: bool,
    },
    /// puts a whole sheet (and the workbook's names) back as they were;
    /// the undo of edits that touch a sheet's structure
    RestoreSheet {
        index: usize,
        sheet: Box<Sheet>,
        names: Vec<DefinedName>,
    },
    /// removes a sheet with no other change (undo of adding one)
    RemoveSheet {
        index: usize,
    },
    /// puts a removed sheet back at its place (undo of deleting it)
    ReinsertSheet {
        index: usize,
        sheet: Box<Sheet>,
        names: Vec<DefinedName>,
    },
    /// puts the workbook's names back as they were
    RestoreNames {
        names: Vec<DefinedName>,
    },
    /// puts formulas back as they were (undo of reference rewriting)
    RestoreFormulas {
        changes: Vec<FormulaChange>,
    },
}

/// what an operation changed
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changed {
    /// cells whose content or style changed
    pub cells: Vec<CellKey>,
    /// rows, columns or sheets moved: the grid redraws everything and the
    /// calculation engine is rebuilt
    pub structure: bool,
}

impl Changed {
    fn range(sheet: usize, range: Range) -> Changed {
        Changed {
            cells: range
                .iter()
                .take(1_000_000)
                .map(|c| (sheet, c.row, c.col))
                .collect(),
            structure: false,
        }
    }

    fn structure() -> Changed {
        Changed {
            cells: Vec::new(),
            structure: true,
        }
    }
}

/// the result of an edit
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub changed: Changed,
    /// applied in order, these undo the edit
    pub inverse: Vec<Operation>,
}

fn sheet_check(book: &Workbook, sheet: usize) -> Result<(), SheetError> {
    if sheet < book.sheets.len() {
        Ok(())
    } else {
        Err(SheetError::Invalid(
            "That sheet doesn’t exist any more.".into(),
        ))
    }
}

/// applies one operation.
pub fn apply(book: &mut Workbook, operation: &Operation) -> Result<Applied, SheetError> {
    use Operation::*;
    match operation {
        SetCell {
            sheet,
            row,
            col,
            input,
        } => cells::set_cell(book, *sheet, *row, *col, input),
        PutCells { sheet, cells } => cells::put_cells(book, *sheet, cells),
        Clear { sheet, range, what } => cells::clear(book, *sheet, *range, *what),
        InsertRows { sheet, at, count } => structure::insert(book, *sheet, Axis::Rows, *at, *count),
        DeleteRows { sheet, at, count } => structure::delete(book, *sheet, Axis::Rows, *at, *count),
        InsertColumns { sheet, at, count } => {
            structure::insert(book, *sheet, Axis::Columns, *at, *count)
        }
        DeleteColumns { sheet, at, count } => {
            structure::delete(book, *sheet, Axis::Columns, *at, *count)
        }
        Move {
            sheet,
            from,
            to_sheet,
            to,
        } => clipboard::move_cells(book, *sheet, *from, *to_sheet, *to),
        Copy {
            sheet,
            from,
            to_sheet,
            to,
            paste,
        } => clipboard::copy_cells(book, *sheet, *from, *to_sheet, *to, *paste),
        Fill {
            sheet,
            source,
            target,
        } => clipboard::fill(book, *sheet, *source, *target),
        Sort {
            sheet,
            range,
            keys,
            has_header,
        } => sort::sort(book, *sheet, *range, keys, *has_header),
        Filter {
            sheet,
            range,
            column,
            keep,
        } => sort::filter(book, *sheet, *range, *column, keep),
        ClearFilter { sheet } => sort::clear_filter(book, *sheet),
        Replace {
            sheet,
            find,
            with,
            options,
        } => text::replace(book, *sheet, find, with, options),
        Merge { sheet, range } => cells::merge(book, *sheet, *range),
        Unmerge { sheet, range } => cells::unmerge(book, *sheet, *range),
        CopyFormats { sheet, from, to } => cells::copy_formats(book, *sheet, *from, *to),
        SetStyle {
            sheet,
            range,
            style,
        } => cells::set_style(book, *sheet, *range, *style),
        ToTable {
            sheet,
            range,
            has_header,
        } => tables::to_table(book, *sheet, *range, *has_header),
        TableToRange { sheet, table } => tables::to_range(book, *sheet, table),
        AddSheet { index, name } => sheets::add(book, *index, name),
        RenameSheet { sheet, name } => sheets::rename(book, *sheet, name),
        MoveSheet { from, to } => sheets::move_sheet(book, *from, *to),
        DuplicateSheet { sheet } => sheets::duplicate(book, *sheet),
        DeleteSheet { sheet } => sheets::delete(book, *sheet),
        Freeze { sheet, rows, cols } => cells::freeze(book, *sheet, *rows, *cols),
        TextToColumns {
            sheet,
            range,
            delimiter,
        } => text::text_to_columns(book, *sheet, *range, *delimiter),
        RemoveDuplicates {
            sheet,
            range,
            columns,
            has_header,
        } => sort::remove_duplicates(book, *sheet, *range, columns, *has_header),
        RestoreSheet {
            index,
            sheet,
            names,
        } => sheets::restore(book, *index, sheet, names),
        RemoveSheet { index } => sheets::remove_plain(book, *index),
        ReinsertSheet {
            index,
            sheet,
            names,
        } => sheets::reinsert(book, *index, sheet, names),
        RestoreNames { names } => Ok(Applied {
            changed: Changed::structure(),
            inverse: vec![RestoreNames {
                names: std::mem::replace(&mut book.defined_names, names.clone()),
            }],
        }),
        RestoreFormulas { changes } => Ok(rewrite::restore(book, changes)),
    }
}

/// undo for edits that rebuild one sheet: the sheet as it was
fn snapshot(book: &Workbook, sheet: usize) -> Operation {
    Operation::RestoreSheet {
        index: sheet,
        sheet: Box::new(book.sheets[sheet].clone()),
        names: book.defined_names.clone(),
    }
}
