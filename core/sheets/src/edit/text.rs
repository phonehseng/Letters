//! find and replace across a sheet or the whole workbook, and splitting
//! a column of text into several columns.

use serde::{Deserialize, Serialize};

use crate::address::Range;
use crate::error::SheetError;
use crate::model::{Cell, Value, Workbook};

use super::cells::saved;
use super::{sheet_check, Applied, Changed};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindOptions {
    #[serde(default)]
    pub match_case: bool,
    /// the whole cell must match, not just part of it
    #[serde(default)]
    pub whole_cell: bool,
    /// look in formulas (excel's "look in: formulas") rather than values
    #[serde(default)]
    pub in_formulas: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub sheet: usize,
    pub row: u32,
    pub col: u32,
    /// the text that matched
    pub text: String,
}

/// what a search looks at in a cell: "=" and the formula, or the value as text
fn searchable(cell: &Cell, options: &FindOptions) -> Option<String> {
    if options.in_formulas {
        if let Some(formula) = &cell.formula {
            return Some(format!("={}", formula.text));
        }
    }
    match &cell.value {
        Value::Blank => None,
        value => crate::calc::value::to_text(value)
            .ok()
            .or_else(|| Some(format!("{value:?}"))),
    }
}

/// excel's find understands * ? and ~ as a condition does
fn matches(haystack: &str, needle: &str, options: &FindOptions) -> bool {
    let pattern = if options.whole_cell {
        needle.to_string()
    } else {
        format!("*{needle}*")
    };
    crate::calc::criteria::wildcard_match_with_case(&pattern, haystack, !options.match_case)
}

/// every cell matching `needle`, row by row; `sheet` none searches all sheets
pub fn find(
    book: &Workbook,
    sheet: Option<usize>,
    needle: &str,
    options: &FindOptions,
) -> Vec<Found> {
    let mut found = Vec::new();
    for (index, target) in book.sheets.iter().enumerate() {
        if sheet.is_some_and(|s| s != index) {
            continue;
        }
        for (row, col, cell) in target.cells.iter() {
            if let Some(text) = searchable(cell, options) {
                if matches(&text, needle, options) {
                    found.push(Found {
                        sheet: index,
                        row,
                        col,
                        text,
                    });
                }
            }
        }
    }
    found
}

/// replaces `needle` with `with` in every matching cell; formulas are
/// changed only when searching formulas, and must still read afterwards
pub fn replace(
    book: &mut Workbook,
    sheet: Option<usize>,
    needle: &str,
    with: &str,
    options: &FindOptions,
) -> Result<Applied, SheetError> {
    if needle.is_empty() {
        return Err(SheetError::Invalid("Type what to find first.".into()));
    }
    let hits = find(book, sheet, needle, options);
    let mut inverse = Vec::new();
    let mut changed = Vec::new();
    for hit in hits {
        let at = crate::address::CellRef::new(hit.row, hit.col);
        let new_text = replaced(&hit.text, needle, with, options);
        inverse.push(saved(book, hit.sheet, Range::cell(at)));
        let input = if new_text.starts_with('=') || options.in_formulas {
            new_text
        } else {
            format!("'{new_text}")
        };
        let input = if input.starts_with('\'')
            && crate::calc::text_numbers::parse_number(&input[1..], book.date_system).is_some()
        {
            input[1..].to_string()
        } else {
            input
        };
        super::cells::set_cell(book, hit.sheet, hit.row, hit.col, &input)?;
        changed.push((hit.sheet, hit.row, hit.col));
    }
    inverse.reverse();
    Ok(Applied {
        changed: Changed {
            cells: changed,
            structure: false,
        },
        inverse,
    })
}

/// the text with every occurrence replaced (the whole text for whole-cell matches)
fn replaced(text: &str, needle: &str, with: &str, options: &FindOptions) -> String {
    if options.whole_cell {
        return with.to_string();
    }
    if options.match_case {
        return text.replace(needle, with);
    }
    let lower = text.to_lowercase();
    let lower_needle = needle.to_lowercase();
    let mut out = String::new();
    let mut position = 0;
    while let Some(found) = lower[position..].find(&lower_needle) {
        let start = position + found;
        out.push_str(&text[position..start]);
        out.push_str(with);
        position = start + needle.len();
    }
    out.push_str(&text[position..]);
    out
}

/// splits each cell of a one-column range at `delimiter` into the
/// columns to its right; pieces that read as numbers or dates become them
pub fn text_to_columns(
    book: &mut Workbook,
    sheet: usize,
    range: Range,
    delimiter: char,
) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    if range.cols() != 1 {
        return Err(SheetError::Invalid(
            "Choose one column of text to split.".into(),
        ));
    }
    let col = range.start.col;
    let pieces: Vec<(u32, Vec<String>)> = (range.start.row..=range.end.row)
        .filter_map(|row| match book.sheets[sheet].cells.value(row, col) {
            Value::Text(text) => Some((
                row,
                text.split(delimiter)
                    .map(|p| p.trim().to_string())
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    let widest = pieces.iter().map(|(_, p)| p.len()).max().unwrap_or(1) as u32;
    let area = Range::from_bounds(range.start.row, col, range.end.row, col + widest - 1);
    let inverse = saved(book, sheet, area);
    for (row, parts) in pieces {
        for (offset, part) in parts.iter().enumerate() {
            super::cells::set_cell(book, sheet, row, col + offset as u32, part)?;
        }
    }
    Ok(Applied {
        changed: Changed::range(sheet, area),
        inverse: vec![inverse],
    })
}
