//! turning a range into a table (filter buttons, banded rows, structured
//! references) and a table back into a plain range, where structured
//! references in formulas become ordinary references, as excel does.

use crate::address::Range;
use crate::error::SheetError;
use crate::formula::{Area, CellPos, Expr, StructuredRef, TableItem};
use crate::model::{AutoFilter, Table, TableColumn, Value, Workbook};

use super::rewrite::rewrite_all;
use super::{sheet_check, snapshot, Applied, Changed};

/// the style excel gives a new table
const DEFAULT_TABLE_STYLE: &str = "TableStyleMedium2";

/// header names from the first row: empty ones become "Column1" and
/// repeated ones get a number, so every column has its own name
fn header_names(book: &Workbook, sheet: usize, range: Range, has_header: bool) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (index, col) in (range.start.col..=range.end.col).enumerate() {
        let text = if has_header {
            let value = book.sheets[sheet].cells.value(range.start.row, col);
            crate::calc::value::to_text(value).unwrap_or_default()
        } else {
            String::new()
        };
        let base = if text.trim().is_empty() {
            format!("Column{}", index + 1)
        } else {
            text.trim().to_string()
        };
        let mut name = base.clone();
        let mut n = 2;
        while names.iter().any(|known| known.eq_ignore_ascii_case(&name)) {
            name = format!("{base}{n}");
            n += 1;
        }
        names.push(name);
    }
    names
}

pub fn to_table(
    book: &mut Workbook,
    sheet: usize,
    range: Range,
    has_header: bool,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let target = &book.sheets[sheet];
    if target.tables.iter().any(|t| t.range.intersects(&range)) {
        return Err(SheetError::Invalid(
            "A table can’t overlap another table.".into(),
        ));
    }
    if target.merged.iter().any(|m| m.intersects(&range)) {
        return Err(SheetError::Invalid(
            "Unmerge the cells first: a table can’t contain merged cells.".into(),
        ));
    }
    let undo = snapshot(book, sheet);
    let names = header_names(book, sheet, range, has_header);
    if has_header {
        for (index, name) in names.iter().enumerate() {
            book.set_value(
                sheet,
                range.start.row,
                range.start.col + index as u32,
                Value::text(name.clone()),
            );
        }
    }
    let id = book
        .sheets
        .iter()
        .flat_map(|s| s.tables.iter().map(|t| t.id))
        .max()
        .unwrap_or(0)
        + 1;
    let name = (1..)
        .map(|n| format!("Table{n}"))
        .find(|n| book.find_table(n).is_none() && book.find_name(n, None).is_none())
        .unwrap_or_default();
    let table = Table {
        id,
        name,
        range,
        header_row: has_header,
        totals_row: false,
        columns: names
            .into_iter()
            .map(|name| TableColumn {
                name,
                ..TableColumn::default()
            })
            .collect(),
        style: Some(DEFAULT_TABLE_STYLE.into()),
        banded_rows: true,
        banded_columns: false,
        first_column: false,
        last_column: false,
        filter: has_header.then_some(AutoFilter {
            range,
            columns: Vec::new(),
            sort: None,
        }),
    };
    book.sheets[sheet].tables.push(table);
    Ok(Applied {
        changed: Changed::range(sheet, range),
        inverse: vec![undo],
    })
}

pub fn to_range(book: &mut Workbook, sheet: usize, name: &str) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let Some(index) = book.sheets[sheet]
        .tables
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(name))
    else {
        return Err(SheetError::Invalid(format!(
            "There is no table called “{name}” on this sheet."
        )));
    };
    let undo = snapshot(book, sheet);
    let table = book.sheets[sheet].tables[index].clone();
    let sheet_name = book.sheets[sheet].name.clone();
    rewrite_all(book, &|expr, home, at| {
        plain_references(expr, &table, &sheet_name, home, at)
    });
    book.sheets[sheet].tables.remove(index);
    Ok(Applied {
        changed: Changed::range(sheet, table.range),
        inverse: vec![undo],
    })
}

/// structured references to `table` as ordinary ones
fn plain_references(
    expr: &Expr,
    table: &Table,
    table_sheet: &str,
    home: &str,
    at: Option<crate::address::CellRef>,
) -> Option<Expr> {
    let mut changed = false;
    let result = expr.clone().transform(&mut |node| match node {
        Expr::Structured { reference } if names_table(&reference, table, table_sheet, home) => {
            match area_of(&reference, table, at) {
                Some(area) => {
                    changed = true;
                    let sheet = (!home.eq_ignore_ascii_case(table_sheet)).then(|| {
                        crate::formula::SheetRef::One {
                            name: table_sheet.to_string(),
                        }
                    });
                    Expr::reference(sheet, area)
                }
                None => Expr::Structured { reference },
            }
        }
        other => other,
    });
    changed.then_some(result)
}

fn names_table(reference: &StructuredRef, table: &Table, table_sheet: &str, home: &str) -> bool {
    match &reference.table {
        Some(name) => name.eq_ignore_ascii_case(&table.name),
        None => home.eq_ignore_ascii_case(table_sheet),
    }
}

/// the cells a structured reference means: whole columns absolute ($C$2:$C$9),
/// this-row references relative in the row ($C2), as excel converts them
fn area_of(
    reference: &StructuredRef,
    table: &Table,
    at: Option<crate::address::CellRef>,
) -> Option<Area> {
    let start = table.range.start.col;
    let first = match &reference.first_column {
        Some(name) => start + table.column_index(name)?,
        None => start,
    };
    let last = match (&reference.last_column, &reference.first_column) {
        (Some(name), _) => start + table.column_index(name)?,
        (None, Some(_)) => first,
        (None, None) => table.range.end.col,
    };
    let data = table.data_range()?;
    let (rows, this_row) = if reference.items.contains(&TableItem::All) {
        ((table.range.start.row, table.range.end.row), false)
    } else if reference.items.contains(&TableItem::Headers) && table.header_row {
        ((table.range.start.row, table.range.start.row), false)
    } else if reference.items.contains(&TableItem::Totals) && table.totals_row {
        ((table.range.end.row, table.range.end.row), false)
    } else {
        (
            (data.start.row, data.end.row),
            reference.items.contains(&TableItem::ThisRow),
        )
    };
    let corner = |row: u32, col: u32| CellPos {
        row,
        col,
        row_abs: !this_row,
        col_abs: true,
    };
    if this_row {
        // the formula's own row, written relative so it fills down
        let row = at.map(|cell| cell.row).unwrap_or(data.start.row);
        let start = corner(row, first);
        return Some(if first == last {
            Area::Cell { at: start }
        } else {
            Area::Range {
                start,
                end: corner(row, last),
            }
        });
    }
    let (a, b) = (corner(rows.0, first), corner(rows.1, last));
    Some(if a == b {
        Area::Cell { at: a }
    } else {
        Area::Range { start: a, end: b }
    })
}
