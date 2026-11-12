//! pdf tables to xlsx: the package is unzipped and its xml read back, and
//! (with --ignored, where excel is installed) opened in excel itself.

mod support;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use letters_convert::*;
use support::*;

/// a page titled "Expenses" over a ruled table: a shaded bold heading
/// row, dates, percentages and amounts written in different ways, and a
/// last row whose first two cells are one.
fn expenses_page() -> TestPage {
    let mut page = TestPage::letter();
    page.text(Font::HelveticaBold, 16.0, 72.0, 80.0, "Expenses");
    page.fill_rect(72.0, 100.0, 380.0, 20.0, [217, 217, 217]);
    let edges = [72.0, 192.0, 292.0, 362.0, 452.0];
    for row in 0..=5 {
        let y = 100.0 + row as f32 * 20.0;
        page.line(72.0, y, 452.0, y, 0.75);
    }
    for (index, x) in edges.iter().enumerate() {
        // the line between the first two columns stops above the last row
        let bottom = if index == 1 { 180.0 } else { 200.0 };
        page.line(*x, 100.0, *x, bottom, 0.75);
    }
    let rows = [
        ["Item", "Date", "Share", "Amount"],
        ["Pens", "2024-03-15", "12.5%", "$1,234.50"],
        ["Paper", "15 March 2024", "50%", "($5.00)"],
        ["Ink", "03/15/2024", "7%", "1,000"],
        ["Total", "", "", "$1,229.50"],
    ];
    for (row, cells) in rows.iter().enumerate() {
        let baseline = 114.0 + row as f32 * 20.0;
        let font = if row == 0 {
            Font::HelveticaBold
        } else {
            Font::Helvetica
        };
        for (column, text) in cells.iter().enumerate() {
            if !text.is_empty() {
                page.text(font, 10.0, edges[column] + 4.0, baseline, text);
            }
        }
    }
    page
}

fn export(layout: SheetLayout, name: &str) -> (PathBuf, XlsxReport) {
    let dir = temp_dir(name);
    let pdf = dir.join("expenses.pdf");
    write_pdf(&pdf, &[expenses_page()]);
    let source = PdfSource::open(&pdf).unwrap();
    let path = dir.join("Expenses.xlsx");
    let options = XlsxExportOptions {
        layout,
        ..Default::default()
    };
    let report = pdf_to_xlsx(&source, &options, &path, &mut |_| {}, &CancelFlag::new()).unwrap();
    (path, report)
}

/// what the sheet xml says about each cell: its type, value and style.
struct SheetCells {
    cells: HashMap<String, (Option<String>, String, usize)>,
    strings: Vec<String>,
    /// per style: number format code, bold, fill colour
    styles: Vec<(Option<String>, bool, Option<String>)>,
    merges: Vec<String>,
    widths: Vec<f64>,
    heights: HashMap<usize, f64>,
}

impl SheetCells {
    fn read(path: &Path, sheet: usize) -> Self {
        let sheet_xml = read_zip_entry(path, &format!("xl/worksheets/sheet{sheet}.xml")).unwrap();
        let strings_xml = read_zip_entry(path, "xl/sharedStrings.xml").unwrap();
        let styles_xml = read_zip_entry(path, "xl/styles.xml").unwrap();
        let sheet = roxmltree::Document::parse(&sheet_xml).unwrap();
        let strings = roxmltree::Document::parse(&strings_xml)
            .unwrap()
            .descendants()
            .filter(|n| n.has_tag_name("t"))
            .map(|n| n.text().unwrap_or("").to_string())
            .collect();
        let styles = roxmltree::Document::parse(&styles_xml).unwrap();
        let formats: HashMap<String, String> = styles
            .descendants()
            .filter(|n| n.has_tag_name("numFmt"))
            .map(|n| {
                (
                    n.attribute("numFmtId").unwrap().into(),
                    n.attribute("formatCode").unwrap().into(),
                )
            })
            .collect();
        let fonts: Vec<bool> = styles
            .descendants()
            .filter(|n| n.has_tag_name("font"))
            .map(|n| n.children().any(|c| c.has_tag_name("b")))
            .collect();
        let fills: Vec<Option<String>> = styles
            .descendants()
            .filter(|n| n.has_tag_name("fill"))
            .map(|n| {
                n.descendants()
                    .find(|c| c.has_tag_name("fgColor"))
                    .and_then(|c| c.attribute("rgb"))
                    .map(str::to_string)
            })
            .collect();
        let xfs = styles
            .descendants()
            .find(|n| n.has_tag_name("cellXfs"))
            .unwrap();
        let style_list = xfs
            .children()
            .filter(|n| n.has_tag_name("xf"))
            .map(|xf| {
                let attribute = |name: &str| xf.attribute(name).unwrap().parse::<usize>().unwrap();
                (
                    formats.get(xf.attribute("numFmtId").unwrap()).cloned(),
                    fonts[attribute("fontId")],
                    fills[attribute("fillId")].clone(),
                )
            })
            .collect();
        let cells = sheet
            .descendants()
            .filter(|n| n.has_tag_name("c"))
            .map(|c| {
                let value = c
                    .children()
                    .find(|v| v.has_tag_name("v"))
                    .and_then(|v| v.text())
                    .unwrap_or("")
                    .to_string();
                (
                    c.attribute("r").unwrap().to_string(),
                    (
                        c.attribute("t").map(str::to_string),
                        value,
                        c.attribute("s").unwrap_or("0").parse().unwrap(),
                    ),
                )
            })
            .collect();
        let merges = sheet
            .descendants()
            .filter(|n| n.has_tag_name("mergeCell"))
            .map(|n| n.attribute("ref").unwrap().to_string())
            .collect();
        let widths = sheet
            .descendants()
            .filter(|n| n.has_tag_name("col"))
            .map(|n| n.attribute("width").unwrap().parse().unwrap())
            .collect();
        let heights = sheet
            .descendants()
            .filter(|n| n.has_tag_name("row"))
            .filter_map(|n| {
                Some((
                    n.attribute("r")?.parse().ok()?,
                    n.attribute("ht")?.parse().ok()?,
                ))
            })
            .collect();
        Self {
            cells,
            strings,
            styles: style_list,
            merges,
            widths,
            heights,
        }
    }

    fn text(&self, cell: &str) -> String {
        let (kind, value, _) = &self.cells[cell];
        assert_eq!(kind.as_deref(), Some("s"), "{cell} should be text");
        self.strings[value.parse::<usize>().unwrap()].clone()
    }

    /// a number cell's value and its format code.
    fn number(&self, cell: &str) -> (f64, Option<String>) {
        let (kind, value, style) = &self.cells[cell];
        assert_eq!(*kind, None, "{cell} should be a number");
        (value.parse().unwrap(), self.styles[*style].0.clone())
    }
}

fn excel_width(points: f64) -> f64 {
    let pixels = points * 96.0 / 72.0;
    ((pixels - 5.0) / 7.0 * 100.0 + 0.5).trunc() / 100.0
}

#[test]
fn table_cells_are_typed_and_formatted() {
    let (path, report) = export(SheetLayout::SheetPerTable, "per-table");
    assert_eq!(report.tables, 1);
    assert_eq!(report.sheets, ["Table 1 (page 1)"]);
    let names = zip_entry_names(&path);
    for part in [
        "[Content_Types].xml",
        "_rels/.rels",
        "xl/workbook.xml",
        "xl/styles.xml",
        "xl/sharedStrings.xml",
        "xl/worksheets/sheet1.xml",
    ] {
        assert!(names.iter().any(|n| n == part), "{part} missing");
    }
    let sheet = SheetCells::read(&path, 1);
    assert_eq!(sheet.text("A1"), "Item");
    assert_eq!(sheet.text("D1"), "Amount");
    let header_style = &sheet.styles[sheet.cells["A1"].2];
    assert!(header_style.1, "the heading row is bold");
    assert_eq!(header_style.2.as_deref(), Some("FFD9D9D9"));

    assert_eq!(sheet.text("A2"), "Pens");
    assert_eq!(sheet.number("B2"), (45366.0, Some("yyyy-mm-dd".into())));
    assert_eq!(sheet.number("C2"), (0.125, Some("0.0%".into())));
    assert_eq!(sheet.number("D2"), (1234.5, Some("\"$\"#,##0.00".into())));
    assert_eq!(sheet.number("B3"), (45366.0, Some("d mmmm yyyy".into())));
    assert_eq!(sheet.number("C3"), (0.5, Some("0%".into())));
    assert_eq!(
        sheet.number("D3"),
        (-5.0, Some("\"$\"0.00;(\"$\"0.00)".into()))
    );
    assert_eq!(sheet.number("B4"), (45366.0, Some("mm/dd/yyyy".into())));
    assert_eq!(sheet.number("D4"), (1000.0, Some("#,##0".into())));
    assert_eq!(sheet.text("A5"), "Total");
    assert_eq!(sheet.merges, ["A5:B5"]);

    let expected: Vec<f64> = [120.0, 100.0, 70.0, 90.0]
        .iter()
        .map(|w| excel_width(*w))
        .collect();
    assert_eq!(sheet.widths, expected);
    for row in 1..=5 {
        assert_eq!(sheet.heights[&row], 20.0);
    }
}

#[test]
fn a_sheet_per_page_keeps_the_text_around_tables() {
    let (path, report) = export(SheetLayout::SheetPerPage, "per-page");
    assert_eq!(report.sheets, ["Page 1"]);
    let sheet = SheetCells::read(&path, 1);
    assert_eq!(sheet.text("A1"), "Expenses");
    // a spacer row keeps the gap above the table, then the table follows
    assert!(sheet.heights[&2] > 4.0);
    assert_eq!(sheet.text("A3"), "Item");
    assert_eq!(sheet.number("D4").0, 1234.5);
    assert_eq!(sheet.merges, ["A7:B7"]);
}

#[test]
fn pages_without_tables_still_give_a_sheet() {
    let dir = temp_dir("no-tables");
    let pdf = dir.join("words.pdf");
    let mut page = TestPage::letter();
    page.text(Font::Helvetica, 11.0, 72.0, 100.0, "Just a sentence.");
    write_pdf(&pdf, &[page]);
    let source = PdfSource::open(&pdf).unwrap();
    let path = dir.join("Words.xlsx");
    let options = XlsxExportOptions {
        layout: SheetLayout::SheetPerTable,
        ..Default::default()
    };
    let report = pdf_to_xlsx(&source, &options, &path, &mut |_| {}, &CancelFlag::new()).unwrap();
    assert_eq!(report.tables, 0);
    assert_eq!(report.notes.len(), 1);
    assert_eq!(SheetCells::read(&path, 1).text("A1"), "Just a sentence.");
}

#[test]
#[ignore = "needs Microsoft Excel"]
fn excel_opens_the_workbook_without_repairs() {
    let (path, _) = export(SheetLayout::SheetPerTable, "excel");
    let path = path.to_string_lossy().into_owned();
    let found = run_office_check(
        "excel-check.ps1",
        &[&path, "A1,B2,C2,D2,D3,D4,A5"],
        "EXCEL.EXE",
        120,
    );
    assert_eq!(found["name"], "Expenses.xlsx", "a repaired file is renamed");
    assert_eq!(found["sheets"], "1");
    assert_eq!(found["A1.text"], "Item");
    assert_eq!(found["B2.value"], "45366");
    assert_eq!(found["B2.text"], "2024-03-15");
    assert_eq!(found["C2.text"], "12.5%");
    assert_eq!(found["D2.value"], "1234.5");
    assert_eq!(found["D2.text"], "$1,234.50");
    assert_eq!(found["D3.text"], "($5.00)");
    assert_eq!(found["D4.text"], "1,000");
    assert_eq!(found["A5.merged"], "True");
}
