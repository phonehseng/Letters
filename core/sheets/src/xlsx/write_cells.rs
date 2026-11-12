//! the `<sheetData>` of a worksheet and the shared string table it fills.
//! shared formula groups are written back as groups when the cells still
//! hold the master's formula moved to their position.

use std::collections::HashMap;

use crate::address::CellRef;
use crate::formula::{self, Expr};
use crate::model::{Cell, FormulaKind, Sheet, Value};
use crate::xml::{escape_attribute, escape_text};

use super::read_cells::SharedString;

/// the shared string table being built while the sheets are written
#[derive(Default)]
pub struct StringTable {
    pub items: Vec<SharedString>,
    index: HashMap<(String, Option<String>), usize>,
    /// how many cells refer to a string (excel writes it as `count`)
    pub uses: usize,
}

impl StringTable {
    pub fn add(&mut self, text: &str, rich: Option<&String>) -> usize {
        self.uses += 1;
        let key = (text.to_string(), rich.cloned());
        if let Some(index) = self.index.get(&key) {
            return *index;
        }
        self.items.push(SharedString {
            text: text.to_string(),
            rich: rich.cloned(),
        });
        self.index.insert(key, self.items.len() - 1);
        self.items.len() - 1
    }

    pub fn to_xml(&self) -> String {
        let mut out =
            String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
        out.push_str(&format!(
            "<sst xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" count=\"{}\" uniqueCount=\"{}\">",
            self.uses,
            self.items.len()
        ));
        for item in &self.items {
            match &item.rich {
                Some(rich) => out.push_str(rich),
                None => out.push_str(&format!("<si>{}</si>", text_element(&item.text))),
            }
        }
        out.push_str("</sst>");
        out
    }
}

/// `<t>` with xml:space="preserve" when leading or trailing spaces matter
fn text_element(text: &str) -> String {
    let needs_preserve =
        text.starts_with(char::is_whitespace) || text.ends_with(char::is_whitespace);
    if needs_preserve {
        format!("<t xml:space=\"preserve\">{}</t>", escape_text(text))
    } else {
        format!("<t>{}</t>", escape_text(text))
    }
}

/// numbers as xsd:double text: rust's shortest exact form, with an
/// exponent for very large or small values so a cell never holds 300 digits
pub fn number_text(value: f64) -> String {
    if !value.is_finite() {
        return "0".into();
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && !(1e-6..1e16).contains(&magnitude) {
        format!("{value:e}")
    } else {
        format!("{value}")
    }
}

/// the masters of a sheet's shared formula groups
struct GroupMaster {
    at: CellRef,
    expr: Expr,
}

fn shared_masters(sheet: &Sheet) -> HashMap<u32, GroupMaster> {
    let mut masters = HashMap::new();
    for (row, col, cell) in sheet.cells.iter() {
        let Some(formula) = &cell.formula else {
            continue;
        };
        if let FormulaKind::Shared {
            group,
            range: Some(_),
        } = formula.kind
        {
            if let Ok(expr) = formula::parse(&formula.text) {
                masters.insert(
                    group,
                    GroupMaster {
                        at: CellRef::new(row, col),
                        expr,
                    },
                );
            }
        }
    }
    masters
}

/// the formula text as xlsx stores it: newer functions get "_xlfn."
pub fn file_formula_text(text: &str) -> String {
    let might_need_prefix = text.contains('(') && text.bytes().any(|b| b.is_ascii_alphabetic());
    if !might_need_prefix {
        return text.to_string();
    }
    match formula::parse(text) {
        Ok(expr) if needs_prefix(&expr) => formula::to_file_formula(&expr),
        _ => text.to_string(),
    }
}

fn needs_prefix(expr: &Expr) -> bool {
    expr.function_names()
        .iter()
        .any(|name| formula::print::file_prefix(name).is_some())
}

pub fn write_sheet_data(sheet: &Sheet, strings: &mut StringTable, out: &mut String) {
    let masters = shared_masters(sheet);
    out.push_str("<sheetData>");
    let mut cell_rows = sheet.cells.row_numbers().peekable();
    let mut info_rows = sheet.rows.keys().copied().peekable();
    loop {
        let row = match (cell_rows.peek(), info_rows.peek()) {
            (None, None) => break,
            (Some(a), Some(b)) => *a.min(b),
            (Some(a), None) => *a,
            (None, Some(b)) => *b,
        };
        if cell_rows.peek() == Some(&row) {
            cell_rows.next();
        }
        if info_rows.peek() == Some(&row) {
            info_rows.next();
        }
        write_row(sheet, row, strings, &masters, out);
    }
    out.push_str("</sheetData>");
}

fn write_row(
    sheet: &Sheet,
    row: u32,
    strings: &mut StringTable,
    masters: &HashMap<u32, GroupMaster>,
    out: &mut String,
) {
    out.push_str(&format!("<row r=\"{}\"", row + 1));
    if let Some(info) = sheet.rows.get(&row) {
        if let Some(style) = info.style {
            out.push_str(&format!(" s=\"{style}\" customFormat=\"1\""));
        }
        if let Some(height) = info.height {
            out.push_str(&format!(" ht=\"{height}\""));
        }
        let flags = [
            ("hidden", info.hidden),
            ("customHeight", info.custom_height),
            ("collapsed", info.collapsed),
        ];
        for (name, on) in flags {
            if on {
                out.push_str(&format!(" {name}=\"1\""));
            }
        }
        if let Some(level) = info.outline_level {
            out.push_str(&format!(" outlineLevel=\"{level}\""));
        }
        for (key, value) in &info.attributes {
            out.push_str(&format!(" {key}=\"{}\"", escape_attribute(value)));
        }
    }
    let cells = sheet.cells.row(row);
    if cells.is_empty() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    for (col, cell) in cells {
        write_cell(CellRef::new(row, *col), cell, strings, masters, out);
    }
    out.push_str("</row>");
}

fn write_cell(
    at: CellRef,
    cell: &Cell,
    strings: &mut StringTable,
    masters: &HashMap<u32, GroupMaster>,
    out: &mut String,
) {
    out.push_str(&format!("<c r=\"{}\"", at.to_a1()));
    if cell.style != 0 {
        out.push_str(&format!(" s=\"{}\"", cell.style));
    }
    let rich = cell
        .extra
        .as_ref()
        .and_then(|extra| extra.rich_text.as_ref());
    let (kind, value) = cell_value(cell, strings, rich);
    if let Some(kind) = kind {
        out.push_str(&format!(" t=\"{kind}\""));
    }
    if let Some(extra) = &cell.extra {
        for (key, value) in &extra.attributes {
            out.push_str(&format!(" {key}=\"{}\"", escape_attribute(value)));
        }
    }
    if is_spilling(cell) {
        // cell metadata 1 is the dynamic array marker (see DYNAMIC_ARRAY_METADATA)
        out.push_str(" cm=\"1\"");
    }
    let formula = cell
        .formula
        .as_ref()
        .map(|_| formula_element(at, cell, masters));
    if formula.is_none() && value.is_none() {
        out.push_str("/>");
        return;
    }
    out.push('>');
    if let Some(formula) = formula {
        out.push_str(&formula);
    }
    if let Some(value) = value {
        out.push_str(&format!("<v>{}</v>", escape_text(&value)));
    }
    out.push_str("</c>");
}

/// a dynamic array formula that has been calculated as one (excel 365
/// marks these even when the result fits in their own cell)
pub fn is_spilling(cell: &Cell) -> bool {
    matches!(
        cell.formula.as_ref().map(|f| &f.kind),
        Some(FormulaKind::Dynamic { spill: Some(_) })
    )
}

/// the metadata part excel 365 writes so that `cm="1"` marks a dynamic
/// array formula; without it excel shows such formulas as old array
/// formulas in braces
pub const DYNAMIC_ARRAY_METADATA: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray"><metadataTypes count="1"><metadataType name="XLDAPR" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1" cellMeta="1"/></metadataTypes><futureMetadata name="XLDAPR" count="1"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk></futureMetadata><cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata></metadata>"#;

/// the cell's `t` attribute and `<v>` text
fn cell_value(
    cell: &Cell,
    strings: &mut StringTable,
    rich: Option<&String>,
) -> (Option<&'static str>, Option<String>) {
    match &cell.value {
        Value::Blank => (None, None),
        Value::Number(n) => (None, Some(number_text(*n))),
        Value::Bool(b) => (Some("b"), Some(if *b { "1" } else { "0" }.to_string())),
        Value::Error(e) => (Some("e"), Some(e.as_str().to_string())),
        // a formula's text result is stored in the cell, not the table
        Value::Text(text) if cell.formula.is_some() => (Some("str"), Some(text.clone())),
        Value::Text(text) => {
            let rich = rich.filter(|raw| rich_text_matches(raw, text));
            (Some("s"), Some(strings.add(text, rich).to_string()))
        }
    }
}

/// kept rich text is only reused while the cell still says the same thing
fn rich_text_matches(raw: &str, text: &str) -> bool {
    let mut reader = crate::xml::Reader::new(raw);
    match reader.root() {
        Ok(tag) => reader.element_text(&tag).is_ok_and(|plain| plain == text),
        Err(_) => false,
    }
}

fn formula_element(at: CellRef, cell: &Cell, masters: &HashMap<u32, GroupMaster>) -> String {
    let Some(formula) = &cell.formula else {
        return String::new();
    };
    let mut attributes = String::new();
    if let Some(extra) = &cell.extra {
        for (key, value) in &extra.formula_attributes {
            attributes.push_str(&format!(" {key}=\"{}\"", escape_attribute(value)));
        }
    }
    let text = escape_text(&file_formula_text(&formula.text));
    match &formula.kind {
        FormulaKind::Normal => format!("<f{attributes}>{text}</f>"),
        FormulaKind::Dynamic { spill: Some(range) } => {
            format!(
                "<f t=\"array\" ref=\"{}\"{attributes}>{text}</f>",
                range.to_a1()
            )
        }
        FormulaKind::Dynamic { .. } => format!("<f{attributes}>{text}</f>"),
        FormulaKind::Array { range } => format!(
            "<f t=\"array\" ref=\"{}\"{attributes}>{text}</f>",
            range.to_a1()
        ),
        FormulaKind::DataTable { attributes: kept } => {
            let kept: String = kept
                .iter()
                .map(|(key, value)| format!(" {key}=\"{}\"", escape_attribute(value)))
                .collect();
            format!("<f{kept}>{text}</f>")
        }
        FormulaKind::Shared {
            group,
            range: Some(range),
        } if masters.contains_key(group) => {
            format!(
                "<f t=\"shared\" ref=\"{}\" si=\"{group}\"{attributes}>{text}</f>",
                range.to_a1()
            )
        }
        FormulaKind::Shared { group, range: None }
            if still_shared(at, &formula.text, masters.get(group)) =>
        {
            format!("<f t=\"shared\" si=\"{group}\"{attributes}/>")
        }
        FormulaKind::Shared { .. } => format!("<f{attributes}>{text}</f>"),
    }
}

/// true when the cell holds exactly its master's formula moved to it
fn still_shared(at: CellRef, text: &str, master: Option<&GroupMaster>) -> bool {
    let Some(master) = master else { return false };
    let rows = i64::from(at.row) - i64::from(master.at.row);
    let cols = i64::from(at.col) - i64::from(master.at.col);
    let moved = formula::refs::translate(&master.expr, rows, cols);
    formula::to_string(&moved) == text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_strings_are_written_compactly() {
        assert_eq!(number_text(0.1), "0.1");
        assert_eq!(number_text(1e300), "1e300");
        assert_eq!(number_text(42.0), "42");
        let mut strings = StringTable::default();
        assert_eq!(strings.add("a", None), 0);
        assert_eq!(strings.add("b", None), 1);
        assert_eq!(strings.add("a", None), 0);
        assert!(strings.to_xml().contains("count=\"3\" uniqueCount=\"2\""));
        assert_eq!(text_element(" x"), "<t xml:space=\"preserve\"> x</t>");
    }

    #[test]
    fn newer_functions_get_their_file_prefix() {
        assert_eq!(
            file_formula_text("XLOOKUP(A1,B:B,C:C)"),
            "_xlfn.XLOOKUP(A1,B:B,C:C)"
        );
        assert_eq!(file_formula_text("SUM(A1:A3)"), "SUM(A1:A3)");
        assert_eq!(file_formula_text("A1+1"), "A1+1");
    }
}
