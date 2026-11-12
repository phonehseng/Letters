//! writing one sheet as a `<table:table>`: columns and rows with repeats
//! for empty runs, cells with their value types, formulas in openformula,
//! merged and covered cells, notes and links, and the kept elements in
//! their schema places.

use crate::address::{CellRef, Range};
use crate::model::*;
use crate::xml::{escape_attribute, escape_text};

use super::write_styles::StyleNames;

/// table children that come before the columns in opendocument's order
const BEFORE_COLUMNS: [&str; 6] = [
    "title",
    "desc",
    "table-source",
    "scenario",
    "forms",
    "shapes",
];

pub fn table_xml(book: &Workbook, index: usize, names: &StyleNames) -> String {
    let sheet = &book.sheets[index];
    let mut out = format!(
        "<table:table table:name=\"{}\" table:style-name=\"{}\">",
        escape_attribute(&sheet.name),
        names.table_style(index)
    );
    for section in sheet
        .carry
        .sections
        .iter()
        .filter(|s| BEFORE_COLUMNS.contains(&s.name.as_str()))
    {
        out.push_str(&section.xml);
    }
    let used = sheet
        .cells
        .used_range()
        .unwrap_or(Range::cell(CellRef::new(0, 0)));
    write_columns(sheet, used, names, &mut out);
    write_rows(book, index, used, names, &mut out);
    for section in sheet
        .carry
        .sections
        .iter()
        .filter(|s| !BEFORE_COLUMNS.contains(&s.name.as_str()))
    {
        out.push_str(&section.xml);
    }
    out.push_str("</table:table>");
    out
}

fn write_columns(sheet: &Sheet, used: Range, names: &StyleNames, out: &mut String) {
    let last = used
        .end
        .col
        .max(sheet.columns.iter().map(|c| c.last).max().unwrap_or(0));
    let mut col = 0;
    while col <= last {
        let style = names.column_style(sheet.column_width(col));
        let hidden = sheet.column_hidden(col);
        let mut run = 1;
        while col + run <= last
            && names.column_style(sheet.column_width(col + run)) == style
            && sheet.column_hidden(col + run) == hidden
        {
            run += 1;
        }
        out.push_str(&format!("<table:table-column table:style-name=\"{style}\""));
        if run > 1 {
            out.push_str(&format!(" table:number-columns-repeated=\"{run}\""));
        }
        if hidden {
            out.push_str(" table:visibility=\"collapse\"");
        }
        out.push_str(" table:default-cell-style-name=\"Default\"/>");
        col += run;
    }
}

fn write_rows(book: &Workbook, index: usize, used: Range, names: &StyleNames, out: &mut String) {
    let sheet = &book.sheets[index];
    let validated_last = sheet
        .validations
        .iter()
        .flat_map(|rule| rule.ranges.iter())
        .filter(|range| range.cell_count() <= MAX_VALIDATED_CELLS)
        .map(|range| range.end.row)
        .max()
        .unwrap_or(0);
    let last_row = used
        .end
        .row
        .max(sheet.rows.keys().next_back().copied().unwrap_or(0))
        .max(validated_last);
    let covered = covered_cells(sheet);
    let mut row = 0;
    while row <= last_row {
        let empty = sheet.cells.row(row).is_empty()
            && !covered.iter().any(|(r, _)| *r == row)
            && validated_columns(sheet, row).is_empty();
        let style = names.row_style(sheet.row_height(row));
        let hidden = sheet.row_hidden(row);
        let mut run = 1;
        if empty {
            while row + run <= last_row
                && sheet.cells.row(row + run).is_empty()
                && validated_columns(sheet, row + run).is_empty()
                && !covered.iter().any(|(r, _)| *r == row + run)
                && names.row_style(sheet.row_height(row + run)) == style
                && sheet.row_hidden(row + run) == hidden
            {
                run += 1;
            }
        }
        out.push_str(&format!("<table:table-row table:style-name=\"{style}\""));
        if run > 1 {
            out.push_str(&format!(" table:number-rows-repeated=\"{run}\""));
        }
        if hidden {
            out.push_str(" table:visibility=\"collapse\"");
        }
        out.push('>');
        if empty {
            out.push_str("<table:table-cell/>");
        } else {
            write_cells(book, index, row, &covered, names, out);
        }
        out.push_str("</table:table-row>");
        row += run;
    }
}

/// every cell of the merged ranges: each must be written, the top-left
/// with its span and the others as covered cells
fn covered_cells(sheet: &Sheet) -> Vec<(u32, u32)> {
    let mut cells = Vec::new();
    for range in &sheet.merged {
        for at in range.iter().take(100_000) {
            cells.push((at.row, at.col));
        }
    }
    cells.sort_unstable();
    cells
}

fn write_cells(
    book: &Workbook,
    index: usize,
    row: u32,
    covered: &[(u32, u32)],
    names: &StyleNames,
    out: &mut String,
) {
    let sheet = &book.sheets[index];
    let mut columns: Vec<u32> = sheet.cells.row(row).iter().map(|(c, _)| *c).collect();
    columns.extend(covered.iter().filter(|(r, _)| *r == row).map(|(_, c)| *c));
    columns.extend(validated_columns(sheet, row));
    columns.sort_unstable();
    columns.dedup();
    let mut next = 0;
    for col in columns {
        if col > next {
            let gap = col - next;
            let repeat = if gap > 1 {
                format!(" table:number-columns-repeated=\"{gap}\"")
            } else {
                String::new()
            };
            out.push_str(&format!("<table:table-cell{repeat}/>"));
        }
        let at = CellRef::new(row, col);
        let is_covered = sheet.merged.iter().any(|m| m.contains(at) && m.start != at);
        out.push_str(&cell_xml(book, index, row, col, is_covered, names));
        next = col + 1;
    }
}

/// the largest validated area written cell by cell, so a huge rule
/// cannot make a huge file
const MAX_VALIDATED_CELLS: u64 = 100_000;

/// empty cells that carry a validation still have to be written
fn validated_columns(sheet: &Sheet, row: u32) -> Vec<u32> {
    let mut columns = Vec::new();
    for rule in &sheet.validations {
        for range in &rule.ranges {
            if range.cell_count() <= MAX_VALIDATED_CELLS
                && (range.start.row..=range.end.row).contains(&row)
            {
                columns.extend(range.start.col..=range.end.col);
            }
        }
    }
    columns
}

fn validation_name(index: usize, sheet: &Sheet, at: CellRef) -> Option<String> {
    sheet
        .validations
        .iter()
        .position(|rule| rule.ranges.iter().any(|range| range.contains(at)))
        .map(|rule| format!("LtrVal{index}_{rule}"))
}

fn cell_xml(
    book: &Workbook,
    index: usize,
    row: u32,
    col: u32,
    covered: bool,
    names: &StyleNames,
) -> String {
    let sheet = &book.sheets[index];
    let element = if covered {
        "table:covered-table-cell"
    } else {
        "table:table-cell"
    };
    let cell = sheet.cells.get(row, col).cloned().unwrap_or_default();
    let mut attributes = String::new();
    if cell.style != 0 {
        attributes.push_str(&format!(
            " table:style-name=\"{}\"",
            names.cell_style(cell.style)
        ));
    }
    if let Some(merge) = sheet
        .merged
        .iter()
        .find(|m| m.start == CellRef::new(row, col))
    {
        attributes.push_str(&format!(
            " table:number-columns-spanned=\"{}\" table:number-rows-spanned=\"{}\"",
            merge.cols(),
            merge.rows()
        ));
    }
    if let Some(formula) = &cell.formula {
        attributes.push_str(&formula_attributes(formula, row, col));
    }
    attributes.push_str(&value_attributes(&cell.value));
    if let Some(name) = validation_name(index, sheet, CellRef::new(row, col)) {
        attributes.push_str(&format!(" table:content-validation-name=\"{name}\""));
    }
    let mut body = String::new();
    if let Some(note) = sheet.comments.iter().find(|n| n.row == row && n.col == col) {
        body.push_str(&format!(
            "<office:annotation><dc:creator>{}</dc:creator>{}</office:annotation>",
            escape_text(&note.author),
            paragraphs(&note.text)
        ));
    }
    let shown = shown_text(book, &cell);
    let link = sheet
        .hyperlinks
        .iter()
        .find(|l| l.range.contains(CellRef::new(row, col)));
    match link {
        Some(link) => {
            let href = link
                .target
                .clone()
                .or_else(|| link.location.as_ref().map(|l| format!("#{l}")))
                .unwrap_or_default();
            body.push_str(&format!(
                "<text:p><text:a xlink:href=\"{}\" xlink:type=\"simple\">{}</text:a></text:p>",
                escape_attribute(&href),
                odf_text(&shown)
            ));
        }
        None if !shown.is_empty() => body.push_str(&paragraphs(&shown)),
        None => {}
    }
    if body.is_empty() {
        format!("<{element}{attributes}/>")
    } else {
        format!("<{element}{attributes}>{body}</{element}>")
    }
}

fn formula_attributes(formula: &Formula, row: u32, col: u32) -> String {
    let text = match crate::formula::parse(&formula.text) {
        Ok(expr) => crate::formula::to_open_formula(&expr),
        Err(_) => format!("of:={}", formula.text),
    };
    let mut out = format!(" table:formula=\"{}\"", escape_attribute(&text));
    let range = match &formula.kind {
        FormulaKind::Array { range } | FormulaKind::Dynamic { spill: Some(range) } => Some(*range),
        _ => None,
    };
    if let Some(range) = range.filter(|r| r.start == CellRef::new(row, col)) {
        out.push_str(&format!(
            " table:number-matrix-columns-spanned=\"{}\" table:number-matrix-rows-spanned=\"{}\"",
            range.cols(),
            range.rows()
        ));
    }
    out
}

fn value_attributes(value: &Value) -> String {
    match value {
        Value::Number(n) => format!(" office:value-type=\"float\" office:value=\"{}\" calcext:value-type=\"float\"", crate::xlsx::number_text(*n)),
        Value::Bool(b) => format!(" office:value-type=\"boolean\" office:boolean-value=\"{b}\" calcext:value-type=\"boolean\""),
        Value::Text(_) => " office:value-type=\"string\" calcext:value-type=\"string\"".into(),
        Value::Error(_) => " office:value-type=\"string\" office:string-value=\"\" calcext:value-type=\"error\"".into(),
        Value::Blank => String::new(),
    }
}

/// the text a cell shows, as libreoffice puts it in text:p
fn shown_text(book: &Workbook, cell: &Cell) -> String {
    match &cell.value {
        Value::Text(text) => text.clone(),
        Value::Blank => String::new(),
        value => {
            let code = &book.styles.get(cell.style).number_format;
            crate::numfmt::format_value(
                value,
                code,
                &crate::numfmt::Locale::default(),
                book.date_system,
            )
            .text
        }
    }
}

fn paragraphs(text: &str) -> String {
    text.split('\n')
        .map(|line| format!("<text:p>{}</text:p>", odf_text(line)))
        .collect()
}

/// text for a paragraph: runs of spaces as text:s, which opendocument
/// needs because xml would collapse them, and tabs as text:tab
pub fn odf_text(text: &str) -> String {
    let mut out = String::new();
    let mut spaces = 0;
    let flush = |out: &mut String, spaces: &mut usize, at_start: bool| {
        if *spaces == 0 {
            return;
        }
        let plain = if at_start { 0 } else { 1 };
        if plain == 1 {
            out.push(' ');
        }
        let rest = *spaces - plain;
        if rest == 1 {
            out.push_str("<text:s/>");
        } else if rest > 1 {
            out.push_str(&format!("<text:s text:c=\"{rest}\"/>"));
        }
        *spaces = 0;
    };
    for c in text.chars() {
        if c == ' ' {
            spaces += 1;
            continue;
        }
        let at_start = out.is_empty();
        flush(&mut out, &mut spaces, at_start);
        match c {
            '\t' => out.push_str("<text:tab/>"),
            _ => out.push_str(&escape_text(&c.to_string())),
        }
    }
    let at_start = out.is_empty();
    flush(&mut out, &mut spaces, at_start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_are_kept_as_text_s() {
        assert_eq!(odf_text("a  b"), "a <text:s/>b");
        assert_eq!(odf_text("  x"), "<text:s text:c=\"2\"/>x");
        assert_eq!(odf_text("a<b"), "a&lt;b");
    }
}
