//! table references ("Sales[Price]", "Sales[#Totals]", "[@Price]") turned
//! into the cells they mean, using the table definitions on the sheets.

use crate::address::Range;
use crate::formula::{StructuredRef, TableItem};
use crate::model::Table;

use super::eval::Evaluator;
use super::value::Area;

pub fn resolve(ev: &Evaluator, reference: &StructuredRef) -> Option<Area> {
    let (sheet, table) = match &reference.table {
        Some(name) => ev.ctx.book.find_table(name)?,
        // "[@Price]" with no table name means the table the formula is in
        None => (ev.sheet, ev.ctx.book.table_at(ev.sheet, ev.row, ev.col)?),
    };
    let rows = rows_for(table, &reference.items, ev.row)?;
    let (first_col, last_col) = columns_for(table, reference)?;
    Some(Area {
        sheet,
        range: Range::from_bounds(rows.0, first_col, rows.1, last_col),
    })
}

/// the first and last row the items select
fn rows_for(table: &Table, items: &[TableItem], formula_row: u32) -> Option<(u32, u32)> {
    let range = table.range;
    let header = table.header_row.then_some(range.start.row);
    let totals = table.totals_row.then_some(range.end.row);
    let data = table
        .data_range()
        .map(|data| (data.start.row, data.end.row));
    if items.contains(&TableItem::ThisRow) {
        let (first, last) = data?;
        return (first..=last)
            .contains(&formula_row)
            .then_some((formula_row, formula_row));
    }
    let has = |item| items.contains(&item);
    if items.is_empty()
        || (has(TableItem::Data) && !has(TableItem::Headers) && !has(TableItem::Totals))
    {
        return data;
    }
    if has(TableItem::All) {
        return Some((range.start.row, range.end.row));
    }
    let mut first = None;
    let mut last = None;
    if has(TableItem::Headers) {
        first = header;
        last = header;
    }
    if has(TableItem::Data) {
        first = first.or(data.map(|d| d.0));
        last = data.map(|d| d.1).or(last);
    }
    if has(TableItem::Totals) {
        first = first.or(totals);
        last = totals.or(last);
    }
    Some((first?, last?))
}

fn columns_for(table: &Table, reference: &StructuredRef) -> Option<(u32, u32)> {
    let start = table.range.start.col;
    match (&reference.first_column, &reference.last_column) {
        (None, _) => Some((start, table.range.end.col)),
        (Some(first), last) => {
            let a = start + table.column_index(first)?;
            let b = match last {
                Some(last) => start + table.column_index(last)?,
                None => a,
            };
            Some((a.min(b), a.max(b)))
        }
    }
}
