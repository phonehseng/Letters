//! tips offered at the moment they help. every tip is one row of `TIPS`
//! with the rule that finds it next to it; each detector looks only at
//! the selection and the cells around it, and fires only when it is sure
//! (a tip that fires wrongly is worse than none). a person can dismiss a
//! tip forever by its id.

use serde::Serialize;

use crate::address::{CellRef, Range};
use crate::calc::text_numbers::parse_number;
use crate::edit::{Operation, SortKey};
use crate::formula::{self, Expr};
use crate::model::{Cell, Value, Workbook};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// stable: the tip's name and where it applies ("add-total:0:A10")
    pub id: String,
    pub title: String,
    pub why: String,
    /// what applying it would put in the sheet, in a few words
    pub preview: String,
    /// applied in order
    pub operations: Vec<Operation>,
}

/// what the person just did, newest last
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecentAction {
    Typed { row: u32, col: u32 },
    Other,
}

/// where the detectors look
pub struct Place<'a> {
    pub book: &'a Workbook,
    pub sheet: usize,
    pub selection: Range,
    pub recent: &'a [RecentAction],
}

mod data;
mod formulas;

use data::*;
use formulas::*;

type Detector = fn(&Place) -> Vec<Suggestion>;

/// every tip: its name, and the rule that finds it
pub const TIPS: &[(&str, &str, Detector)] = &[
    ("add-total", "an empty cell right below two or more numbers in a column", add_total),
    ("fill-down", "a formula typed next to a column of data that goes further down", fill_down),
    ("flash-fill", "text typed beside a column of text, where one simple rule turns the column into what was typed", flash_fill),
    ("freeze-header", "a header row of text above 20 or more rows of data, with nothing frozen", freeze_header),
    ("make-table", "a block with a header row and three or more rows, not yet a table", make_table),
    ("sort", "the selection is in the header of a block of data", sort_column),
    ("remove-duplicates", "a block where some rows repeat exactly", remove_duplicates),
    ("text-to-numbers", "two or more cells in the selection holding numbers as text", text_to_numbers),
    ("text-to-dates", "two or more cells in the selection holding dates as text", text_to_dates),
    ("exact-lookup", "a VLOOKUP or HLOOKUP with approximate match on an unsorted first column", exact_lookup),
    ("name-value", "the same unusual number written into three or more formulas", name_value),
    ("bounded-ranges", "a volatile formula reading whole columns on a sheet of 10,000 or more rows", bounded_ranges),
    ("use-ifs", "three or more IFs nested inside each other", use_ifs),
    ("summarise", "a block with a column of repeating categories and a column of numbers", summarise),
];

/// the tips for the selection, without the ones dismissed
pub fn suggest(
    book: &Workbook,
    sheet: usize,
    selection: Range,
    recent: &[RecentAction],
    dismissed: &[String],
) -> Vec<Suggestion> {
    if sheet >= book.sheets.len() {
        return Vec::new();
    }
    let place = Place {
        book,
        sheet,
        selection,
        recent,
    };
    TIPS.iter()
        .flat_map(|(_, _, detect)| detect(&place))
        .filter(|s| !dismissed.contains(&s.id))
        .collect()
}

fn value<'a>(place: &Place<'a>, row: u32, col: u32) -> &'a Value {
    place.book.sheets[place.sheet].cells.value(row, col)
}

fn cell<'a>(place: &Place<'a>, row: u32, col: u32) -> Option<&'a Cell> {
    place.book.sheets[place.sheet].cells.get(row, col)
}

fn id(name: &str, place: &Place, at: CellRef) -> String {
    format!("{name}:{}:{}", place.sheet, at.to_a1())
}

fn add_total(place: &Place) -> Vec<Suggestion> {
    let at = place.selection.start;
    if place.selection.cell_count() != 1
        || cell(place, at.row, at.col).is_some_and(|c| !c.is_empty())
        || at.row < 2
    {
        return Vec::new();
    }
    let mut top = at.row;
    while top > 0
        && matches!(value(place, top - 1, at.col), Value::Number(_))
        && cell(place, top - 1, at.col).is_some_and(|c| c.formula.is_none())
    {
        top -= 1;
    }
    if at.row - top < 2 {
        return Vec::new();
    }
    let range = Range::from_bounds(top, at.col, at.row - 1, at.col).to_a1();
    vec![Suggestion {
        id: id("add-total", place, at),
        title: "Add a total".into(),
        why: "The cells above are numbers; Alt+= puts their sum here.".into(),
        preview: format!("=SUM({range})"),
        operations: vec![Operation::SetCell {
            sheet: place.sheet,
            row: at.row,
            col: at.col,
            input: format!("=SUM({range})"),
        }],
    }]
}

fn fill_down(place: &Place) -> Vec<Suggestion> {
    let Some(RecentAction::Typed { row, col }) = place.recent.last() else {
        return Vec::new();
    };
    if cell(place, *row, *col)
        .and_then(|c| c.formula.as_ref())
        .is_none()
        || !value(place, row + 1, *col).is_blank()
    {
        return Vec::new();
    }
    let neighbour = if *col > 0 && !value(place, row + 1, col - 1).is_blank() {
        Some(col - 1)
    } else {
        Some(col + 1).filter(|c| !value(place, row + 1, *c).is_blank())
    };
    let Some(neighbour) = neighbour else {
        return Vec::new();
    };
    let mut last = row + 1;
    while !value(place, last + 1, neighbour).is_blank() {
        last += 1;
    }
    let target = Range::from_bounds(*row, *col, last, *col);
    vec![Suggestion {
        id: id("fill-down", place, CellRef::new(*row, *col)),
        title: "Fill the formula down".into(),
        why: format!(
            "The data next to it goes down to row {}; Ctrl+D copies the formula to every row.",
            last + 1
        ),
        preview: format!("{} rows", last - row),
        operations: vec![Operation::Fill {
            sheet: place.sheet,
            source: Range::cell(CellRef::new(*row, *col)),
            target,
        }],
    }]
}

fn text_of(place: &Place, row: u32, col: u32) -> Option<String> {
    match value(place, row, col) {
        Value::Text(t) => Some(t.clone()),
        _ => None,
    }
}

fn flash_fill(place: &Place) -> Vec<Suggestion> {
    let Some(RecentAction::Typed { row, col }) = place.recent.last() else {
        return Vec::new();
    };
    if *col == 0 || text_of(place, *row, *col).is_none() {
        return Vec::new();
    }
    let source = col - 1;
    let mut first = *row;
    while first > 0
        && text_of(place, first - 1, *col).is_some()
        && text_of(place, first - 1, source).is_some()
    {
        first -= 1;
    }
    let examples: Vec<(String, String)> = (first..=*row)
        .filter_map(|r| Some((text_of(place, r, source)?, text_of(place, r, *col)?)))
        .collect();
    let mut last = *row;
    while text_of(place, last + 1, source).is_some() && value(place, last + 1, *col).is_blank() {
        last += 1;
    }
    let rest: Vec<String> = (row + 1..=last)
        .filter_map(|r| text_of(place, r, source))
        .collect();
    if examples.len() < 2 || rest.is_empty() {
        return Vec::new();
    }
    let Some(values) = super::flash_fill::complete_by_example(&examples, &rest) else {
        return Vec::new();
    };
    let cells = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            (
                row + 1 + i as u32,
                *col,
                Some(Cell::value(Value::text(v.clone()))),
            )
        })
        .collect();
    vec![Suggestion {
        id: id("flash-fill", place, CellRef::new(*row, *col)),
        title: "Fill the rest by example".into(),
        why: "The examples follow one pattern from the column beside them (Ctrl+E in Excel)."
            .into(),
        preview: values
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
        operations: vec![Operation::PutCells {
            sheet: place.sheet,
            cells,
        }],
    }]
}

/// text cells of the selection that read as `kind` (numbers or dates)
fn convertible(place: &Place, dates: bool) -> Vec<(u32, u32, f64)> {
    place.book.sheets[place.sheet]
        .cells
        .iter_range(place.selection)
        .filter_map(|(r, c, cell)| match &cell.value {
            Value::Text(t) if cell.formula.is_none() => {
                let looks_like_date = t.contains('/') || t.matches('-').count() == 2;
                (looks_like_date == dates)
                    .then(|| parse_number(t, place.book.date_system))
                    .flatten()
                    .map(|n| (r, c, n))
            }
            _ => None,
        })
        .collect()
}

/// retyping the text gives the number or date it shows, with excel's
/// format for how it was written (a date format for dates)
fn conversion(place: &Place, dates: bool) -> Vec<Suggestion> {
    let found = convertible(place, dates);
    if found.len() < 2 {
        return Vec::new();
    }
    let (name, title, why) = if dates {
        (
            "text-to-dates",
            "Convert to dates",
            "These dates are stored as text, so they don't sort or count as dates.",
        )
    } else {
        (
            "text-to-numbers",
            "Convert to numbers",
            "These numbers are stored as text, so SUM and AVERAGE skip them.",
        )
    };
    let operations = found
        .iter()
        .filter_map(|(r, c, _)| {
            let text = text_of(place, *r, *c)?;
            Some(Operation::SetCell {
                sheet: place.sheet,
                row: *r,
                col: *c,
                input: text.trim().to_string(),
            })
        })
        .collect();
    vec![Suggestion {
        id: id(name, place, place.selection.start),
        title: title.into(),
        why: why.into(),
        preview: format!("{} cells", found.len()),
        operations,
    }]
}
fn text_to_numbers(place: &Place) -> Vec<Suggestion> {
    conversion(place, false)
}

fn text_to_dates(place: &Place) -> Vec<Suggestion> {
    conversion(place, true)
}

#[cfg(test)]
mod tests;
