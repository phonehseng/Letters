//! what the grid draws: for a visible range, every cell's shown text,
//! colour, alignment and flags, computed straight from the sparse grid
//! so scrolling a 200,000-row sheet only touches the rows on screen,
//! with what the sheet's conditional formats change on top.

use std::collections::HashMap;

use serde::Serialize;

use crate::address::{CellRef, Range};
use crate::conditional::{looks_in, ConditionalLook};
use crate::model::{Cell, Sheet, Value, Workbook};
use crate::numfmt::{format_value, Locale};

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewCell {
    pub row: u32,
    pub col: u32,
    /// the text as the cell shows it
    pub text: String,
    /// index into the workbook's style table
    pub style: u32,
    /// "left", "center" or "right": the style's, or excel's default
    /// (numbers right, logical values and errors centred, text left)
    pub align: &'static str,
    /// a colour the number format asks for ("Red"), or the font's, as "#RRGGBB"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_formula: bool,
    /// the value is the file's, not recalculated (see the calc report)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub not_recalculated: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub has_note: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub has_link: bool,
    /// the merged range this cell starts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge: Option<Range>,
    /// what matching conditional format rules draw over the cell's own
    /// look; left out when no rule changes anything
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditional: Option<ConditionalLook>,
}

/// the text a cell shows with its number format
pub fn shown(book: &Workbook, cell: &Cell) -> String {
    shown_with(book, cell, &Locale::default())
}

fn shown_with(book: &Workbook, cell: &Cell, locale: &Locale) -> String {
    let code = &book.styles.get(cell.style).number_format;
    format_value(&cell.value, code, locale, book.date_system).text
}

fn named_color(name: &str) -> Option<String> {
    let rgb = match name.to_ascii_lowercase().as_str() {
        "red" => "FF0000",
        "blue" => "0000FF",
        "green" => "00FF00",
        "yellow" => "FFFF00",
        "magenta" => "FF00FF",
        "cyan" => "00FFFF",
        "white" => "FFFFFF",
        "black" => "000000",
        _ => return None,
    };
    Some(format!("#{rgb}"))
}

/// every non-empty cell inside `range` with its conditional formatting,
/// plus the empty cells a rule marks (excel colours an empty cell that
/// "is less than 50"), in row order
pub fn cells_in(book: &Workbook, sheet: usize, range: Range, locale: &Locale) -> Vec<ViewCell> {
    let mut cells = own_cells_in(book, sheet, range, locale);
    let looks = looks_in(book, sheet, range);
    if looks.is_empty() {
        return cells;
    }
    let Some(target) = book.sheets.get(sheet) else {
        return cells;
    };
    let mut looks: HashMap<CellRef, ConditionalLook> = looks.into_iter().collect();
    for cell in &mut cells {
        cell.conditional = looks.remove(&CellRef::new(cell.row, cell.col));
    }
    // what is left are empty cells
    for (at, look) in looks {
        let mut blank = flagged_cell(target, at);
        blank.conditional = Some(look);
        cells.push(blank);
    }
    cells.sort_by_key(|cell| (cell.row, cell.col));
    cells
}

/// an empty cell with the flags it would have (a note, a link, a merge)
fn flagged_cell(target: &Sheet, at: CellRef) -> ViewCell {
    ViewCell {
        row: at.row,
        col: at.col,
        text: String::new(),
        style: 0,
        align: "left",
        color: None,
        background: None,
        is_formula: false,
        not_recalculated: false,
        has_note: target
            .comments
            .iter()
            .any(|n| n.row == at.row && n.col == at.col),
        has_link: target.hyperlinks.iter().any(|l| l.range.contains(at)),
        merge: target.merged.iter().copied().find(|m| m.start == at),
        conditional: None,
    }
}

/// every non-empty cell inside `range` with its own look only, for what
/// reads the values rather than draws them (copying, filter lists)
pub fn own_cells_in(book: &Workbook, sheet: usize, range: Range, locale: &Locale) -> Vec<ViewCell> {
    let Some(target) = book.sheets.get(sheet) else {
        return Vec::new();
    };
    let theme = &book.styles.theme_colors;
    let mut out = Vec::new();
    for (row, col, cell) in target.cells.iter_range(range) {
        let style = book.styles.get(cell.style);
        let code = &style.number_format;
        let formatted = format_value(&cell.value, code, locale, book.date_system);
        let default_align = match cell.value {
            Value::Number(_) => "right",
            Value::Bool(_) | Value::Error(_) => "center",
            _ => "left",
        };
        let align = match style.alignment.horizontal.as_deref() {
            Some("center") | Some("centerContinuous") => "center",
            Some("right") => "right",
            Some("left") => "left",
            _ => default_align,
        };
        let at = CellRef::new(row, col);
        out.push(ViewCell {
            row,
            col,
            text: formatted.text,
            style: cell.style,
            align,
            color: formatted
                .color
                .as_deref()
                .and_then(named_color)
                .or_else(|| style.font.color.as_ref().and_then(|c| c.to_css(theme))),
            background: style.fill.background().and_then(|c| c.to_css(theme)),
            is_formula: cell.formula.is_some(),
            not_recalculated: cell.not_recalculated,
            has_note: target.comments.iter().any(|n| n.row == row && n.col == col),
            has_link: target.hyperlinks.iter().any(|l| l.range.contains(at)),
            merge: target.merged.iter().copied().find(|m| m.start == at),
            conditional: None,
        });
    }
    out
}

/// the formula bar's text for a cell: "=" and the formula, or the value
/// as it would be typed
pub fn edit_text(book: &Workbook, sheet: usize, row: u32, col: u32) -> String {
    let Some(cell) = book.sheets.get(sheet).and_then(|s| s.cells.get(row, col)) else {
        return String::new();
    };
    if let Some(formula) = &cell.formula {
        return format!("={}", formula.text);
    }
    match &cell.value {
        Value::Text(text)
            if crate::calc::text_numbers::parse_number(text, book.date_system).is_some() =>
        {
            format!("'{text}")
        }
        Value::Number(_) if book.styles.get(cell.style).number_format != "General" => {
            shown(book, cell)
        }
        value => crate::calc::value::to_text(value).unwrap_or_else(|e| e.as_str().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_cells_are_formatted() {
        let mut book = Workbook::new();
        let red = book
            .styles
            .derive(0, |s| s.number_format = "0.00;[Red]-0.00".into());
        book.sheets[0].cells.set(
            0,
            0,
            Cell {
                value: Value::Number(-1.5),
                style: red,
                ..Cell::default()
            },
        );
        book.sheets[0]
            .cells
            .set(0, 1, Cell::value(Value::text("12")));
        book.sheets[0]
            .cells
            .set(500, 0, Cell::value(Value::Bool(true)));
        let cells = cells_in(
            &book,
            0,
            Range::parse("A1:Z100").unwrap(),
            &Locale::default(),
        );
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].text, "-1.50");
        assert_eq!(cells[0].color.as_deref(), Some("#FF0000"));
        assert_eq!(cells[0].align, "right");
        assert_eq!(edit_text(&book, 0, 0, 1), "'12");
    }

    #[test]
    fn conditional_formats_ride_with_their_cells() {
        let mut book = Workbook::new();
        book.sheets[0]
            .cells
            .set(0, 0, Cell::value(Value::Number(80.0)));
        book.sheets[0]
            .cells
            .set(1, 0, Cell::value(Value::Number(20.0)));
        let red = book.styles.add_differential(Some("9C0006"), None, false);
        book.sheets[0]
            .conditional_formats
            .push(crate::model::ConditionalFormat {
                ranges: vec![Range::parse("A1:A3").unwrap()],
                rules: vec![crate::model::ConditionalRule {
                    kind: "cellIs".into(),
                    operator: Some("lessThan".into()),
                    formulas: vec!["50".into()],
                    format: Some(red),
                    priority: 1,
                    ..Default::default()
                }],
            });
        let view = Range::parse("A1:B5").unwrap();
        let cells = cells_in(&book, 0, view, &Locale::default());
        // A1 (80) has nothing, A2 (20) is red, and the empty A3 counts as 0
        assert_eq!(cells.len(), 3);
        assert_eq!(cells[0].conditional, None);
        let red_text = cells[1].conditional.as_ref().unwrap();
        assert_eq!(red_text.color.as_deref(), Some("#9C0006"));
        assert_eq!((cells[2].row, cells[2].text.as_str()), (2, ""));
        assert!(cells[2].conditional.is_some());
        // what reads values (copying, filter lists) sees only real cells
        assert_eq!(own_cells_in(&book, 0, view, &Locale::default()).len(), 2);
        let json = serde_json::to_string(&cells[0]).unwrap();
        assert!(
            !json.contains("conditional"),
            "nothing sent without a rule: {json}"
        );
    }
}
