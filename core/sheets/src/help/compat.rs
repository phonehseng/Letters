//! notes in plain words when a workbook uses something the other family
//! lacks: excel 365 functions saved to ods, google sheets functions in a
//! file exported from sheets, and features a format can't hold. stored
//! values are always kept; the note says what will and won't recalculate.

use serde::Serialize;

use crate::formula;
use crate::model::Workbook;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    Xlsx,
    Ods,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityNote {
    pub text: String,
    /// up to ten cells where it happens, as "Sheet1!B2"
    pub cells: Vec<String>,
}

/// excel functions libreoffice only has from version 24.8, or not at all
const EXCEL_ONLY: [&str; 16] = [
    "XLOOKUP",
    "XMATCH",
    "FILTER",
    "SORT",
    "SORTBY",
    "UNIQUE",
    "SEQUENCE",
    "RANDARRAY",
    "LET",
    "LAMBDA",
    "TEXTSPLIT",
    "TEXTBEFORE",
    "TEXTAFTER",
    "VSTACK",
    "HSTACK",
    "TAKE",
];

/// google sheets functions excel and libreoffice don't have
const SHEETS_ONLY: [&str; 16] = [
    "QUERY",
    "ARRAYFORMULA",
    "IMPORTRANGE",
    "IMPORTHTML",
    "IMPORTXML",
    "IMPORTDATA",
    "GOOGLEFINANCE",
    "GOOGLETRANSLATE",
    "SPARKLINE",
    "IMAGE",
    "REGEXMATCH",
    "REGEXEXTRACT",
    "REGEXREPLACE",
    "SPLIT",
    "FLATTEN",
    "DETECTLANGUAGE",
];

fn cell_name(book: &Workbook, sheet: usize, row: u32, col: u32) -> String {
    let name = formula::print::quote_sheet(&book.sheets[sheet].name);
    format!("{name}!{}", crate::address::CellRef::new(row, col).to_a1())
}

/// the cells whose formulas use any of `names` (up to ten), and which names
fn cells_using(book: &Workbook, names: &[&str]) -> (Vec<String>, Vec<String>) {
    let mut cells = Vec::new();
    let mut used: Vec<String> = Vec::new();
    for (index, sheet) in book.sheets.iter().enumerate() {
        for (row, col, cell) in sheet.cells.iter() {
            let Some(formula) = &cell.formula else {
                continue;
            };
            let Ok(expr) = formula::parse(&formula.text) else {
                continue;
            };
            let hits: Vec<String> = expr
                .function_names()
                .into_iter()
                .filter(|n| names.contains(&n.as_str()))
                .collect();
            if hits.is_empty() {
                continue;
            }
            if cells.len() < 10 {
                cells.push(cell_name(book, index, row, col));
            }
            for hit in hits {
                if !used.contains(&hit) {
                    used.push(hit);
                }
            }
        }
    }
    (cells, used)
}

pub fn compatibility(book: &Workbook, target: Target) -> Vec<CompatibilityNote> {
    let mut notes = Vec::new();
    let (cells, used) = cells_using(book, &SHEETS_ONLY);
    if !used.is_empty() {
        notes.push(CompatibilityNote {
            text: format!("This workbook uses Google Sheets functions ({}) that Excel, LibreOffice and Letters don't have. The values Google Sheets calculated are kept and shown, but these cells won't update.", used.join(", ")),
            cells,
        });
    }
    // google sheets exports such formulas wrapped as __xludf.DUMMYFUNCTION("...")
    let (cells, used) = cells_using(book, &["DUMMYFUNCTION"]);
    if !used.is_empty() {
        notes.push(CompatibilityNote {
            text: "These cells came from Google Sheets with formulas only Sheets can run; Sheets stored them as text inside DUMMYFUNCTION. Their last values are kept.".into(),
            cells,
        });
    }
    if target == Target::Ods {
        let (cells, used) = cells_using(book, &EXCEL_ONLY);
        if !used.is_empty() {
            notes.push(CompatibilityNote {
                text: format!("{} came with Excel 365. LibreOffice older than 24.8 shows #NAME? for them; newer LibreOffice and Google Sheets calculate them.", used.join(", ")),
                cells,
            });
        }
        let formats: usize = book
            .sheets
            .iter()
            .map(|s| s.conditional_formats.len())
            .sum();
        if formats > 0 && book.carry.source != Some(crate::model::SourceFormat::Ods) {
            notes.push(CompatibilityNote {
                text: format!(
                    "{formats} conditional format{} won't be saved in the OpenDocument file yet.",
                    if formats == 1 { "" } else { "s" }
                ),
                cells: Vec::new(),
            });
        }
        let tables: usize = book.sheets.iter().map(|s| s.tables.len()).sum();
        if tables > 0 {
            notes.push(CompatibilityNote { text: "Tables are saved as ordinary ranges in OpenDocument; formulas like Table1[Price] still work in Letters but not in LibreOffice.".into(), cells: Vec::new() });
        }
        if book.has_macros {
            notes.push(CompatibilityNote {
                text: "The Excel macros in this file are not saved in OpenDocument.".into(),
                cells: Vec::new(),
            });
        }
    }
    if book.has_macros && target == Target::Xlsx {
        notes.push(CompatibilityNote {
            text: "This file contains macros. Letters keeps them unchanged but never runs them."
                .into(),
            cells: Vec::new(),
        });
    }
    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Cell;

    #[test]
    fn notes_name_the_functions_and_cells() {
        let mut book = Workbook::new();
        book.sheets[0]
            .cells
            .set(0, 0, Cell::formula("XLOOKUP(1,B:B,C:C)"));
        book.sheets[0]
            .cells
            .set(1, 0, Cell::formula("QUERY(A1:B5,\"select A\")"));
        let notes = compatibility(&book, Target::Ods);
        assert!(notes[0].text.contains("QUERY"));
        assert_eq!(notes[0].cells, vec!["Sheet1!A2".to_string()]);
        assert!(notes[1].text.starts_with("XLOOKUP came with Excel 365"));
        assert_eq!(
            compatibility(&book, Target::Xlsx).len(),
            1,
            "excel has XLOOKUP"
        );
    }
}
