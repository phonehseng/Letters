//! formatting the selection: number formats, bold, colours, alignment and
//! the engine's style presets, as one undoable edit.
//!
//! the engine's `SetStyle` gives a whole range one style, which would
//! wipe a bold cell's colour when only the number format changes. so each
//! cell keeps its own style with just the one thing changed, and all of
//! them go back in a single `PutCells`.

use std::collections::HashMap;

use letters_sheets::edit::Operation;
use letters_sheets::model::{CellStyle, Color, Fill};
use letters_sheets::{Range, Workbook};
use serde::Deserialize;

use super::history::Edit;

/// one thing to change about the selected cells' look.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(
    tag = "change",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StyleChange {
    NumberFormat {
        code: String,
    },
    Bold {
        on: bool,
    },
    Italic {
        on: bool,
    },
    Underline {
        on: bool,
    },
    Strike {
        on: bool,
    },
    /// "left", "center", "right", or none for excel's default
    Align {
        horizontal: Option<String>,
    },
    Wrap {
        on: bool,
    },
    /// "#RRGGBB", or none to clear
    Fill {
        color: Option<String>,
    },
    FontColor {
        color: Option<String>,
    },
    /// one of the engine's style presets ("Title", "Heading 1"...)
    Preset {
        name: String,
    },
}

/// more cells than this in one format change is almost surely a
/// mistake (a whole sheet of empty cells), and would be slow to undo
const MOST_CELLS: u64 = 1_000_000;

/// the edit that applies `change` to every cell of `ranges`.
pub fn style_edit(
    book: &mut Workbook,
    sheet: usize,
    ranges: &[Range],
    change: &StyleChange,
) -> Result<Edit, String> {
    let target = book
        .sheets
        .get(sheet)
        .ok_or("That sheet doesn’t exist any more.")?;
    let used = target.cells.used_range();
    let ranges: Vec<Range> = ranges
        .iter()
        .filter_map(|range| clamp_to_used(*range, used))
        .collect();
    let count: u64 = ranges.iter().map(Range::cell_count).sum();
    if count > MOST_CELLS {
        return Err(
            "That is too many cells to format at once. Select only the cells with data.".into(),
        );
    }
    let preset = preset_style(change)?;
    let mut derived: HashMap<u32, u32> = HashMap::new();
    let mut cells = Vec::with_capacity(count as usize);
    for range in &ranges {
        for at in range.iter() {
            let mut cell = book.sheets[sheet]
                .cells
                .get(at.row, at.col)
                .cloned()
                .unwrap_or_default();
            let old = cell.style;
            cell.style = *derived
                .entry(old)
                .or_insert_with(|| derive(book, old, change, preset.as_ref()));
            cells.push((at.row, at.col, Some(cell)));
        }
    }
    Ok(Edit::Book(Operation::PutCells { sheet, cells }))
}

/// whole rows or columns stop at the last used cell; other ranges stay
fn clamp_to_used(range: Range, used: Option<Range>) -> Option<Range> {
    if !(range.is_whole_columns() || range.is_whole_rows()) {
        return Some(range);
    }
    let used = used?;
    let bounds = Range::from_bounds(0, 0, used.end.row, used.end.col);
    range.intersection(&bounds)
}

fn preset_style(change: &StyleChange) -> Result<Option<CellStyle>, String> {
    let StyleChange::Preset { name } = change else {
        return Ok(None);
    };
    letters_sheets::help::presets::style_presets()
        .into_iter()
        .find(|preset| preset.name == name)
        .map(|preset| Some(preset.style))
        .ok_or_else(|| format!("There is no cell style called “{name}”."))
}

fn derive(book: &mut Workbook, base: u32, change: &StyleChange, preset: Option<&CellStyle>) -> u32 {
    book.styles.derive(base, |style| match change {
        StyleChange::NumberFormat { code } => style.number_format = code.clone(),
        StyleChange::Bold { on } => style.font.bold = *on,
        StyleChange::Italic { on } => style.font.italic = *on,
        StyleChange::Underline { on } => style.font.underline = on.then(|| "single".into()),
        StyleChange::Strike { on } => style.font.strike = *on,
        StyleChange::Align { horizontal } => style.alignment.horizontal = horizontal.clone(),
        StyleChange::Wrap { on } => style.alignment.wrap_text = *on,
        StyleChange::Fill { color } => {
            style.fill = color
                .as_deref()
                .map(|hex| Fill::solid(Color::rgb(hex)))
                .unwrap_or_default();
        }
        StyleChange::FontColor { color } => {
            style.font.color = color.as_deref().map(Color::rgb);
        }
        StyleChange::Preset { .. } => {
            if let Some(preset) = preset {
                // a preset sets the look but keeps the number format, as
                // excel's cell styles leave "number" alone by default
                let number_format = std::mem::take(&mut style.number_format);
                *style = preset.clone();
                style.number_format = number_format;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_sheets::{Cell, Value};

    /// a new empty cell with a style, for tests.
    fn styled(book: &mut Workbook, change: impl FnOnce(&mut CellStyle)) -> Cell {
        Cell {
            style: book.styles.derive(0, change),
            ..Cell::default()
        }
    }

    fn apply(book: &mut Workbook, edit: &Edit) {
        let Edit::Book(operation) = edit else {
            panic!("a style change is a book edit")
        };
        letters_sheets::edit::apply(book, operation).unwrap();
    }

    #[test]
    fn a_number_format_keeps_each_cells_other_formatting() {
        let mut book = Workbook::new();
        let bold = styled(&mut book, |s| s.font.bold = true);
        book.sheets[0].cells.set(
            0,
            0,
            Cell {
                value: Value::Number(0.25),
                ..bold
            },
        );
        book.sheets[0]
            .cells
            .set(1, 0, Cell::value(Value::Number(0.5)));
        let range = Range::parse("A1:A2").unwrap();
        let edit = style_edit(
            &mut book,
            0,
            &[range],
            &StyleChange::NumberFormat { code: "0%".into() },
        )
        .unwrap();
        apply(&mut book, &edit);
        let first = book
            .styles
            .get(book.sheets[0].cells.get(0, 0).unwrap().style);
        let second = book
            .styles
            .get(book.sheets[0].cells.get(1, 0).unwrap().style);
        assert!(first.font.bold && first.number_format == "0%");
        assert!(!second.font.bold && second.number_format == "0%");
    }

    #[test]
    fn whole_columns_stop_at_the_last_used_row() {
        let mut book = Workbook::new();
        book.sheets[0]
            .cells
            .set(4, 1, Cell::value(Value::Number(1.0)));
        let column = Range::from_bounds(0, 1, letters_sheets::address::MAX_ROWS - 1, 1);
        let edit = style_edit(&mut book, 0, &[column], &StyleChange::Bold { on: true }).unwrap();
        let Edit::Book(Operation::PutCells { cells, .. }) = edit else {
            panic!("expected cells")
        };
        assert_eq!(cells.len(), 5);
    }

    #[test]
    fn a_preset_by_an_unknown_name_is_refused_in_words() {
        let mut book = Workbook::new();
        let change = StyleChange::Preset {
            name: "Sparkly".into(),
        };
        let error = style_edit(&mut book, 0, &[Range::parse("A1").unwrap()], &change).unwrap_err();
        assert!(error.contains("Sparkly"));
    }
}
