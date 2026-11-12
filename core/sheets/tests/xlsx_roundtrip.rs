//! xlsx files written and read back: every modelled feature survives, a
//! second save is identical to the first, and xml or parts the model
//! does not know are kept exactly.

use letters_sheets::address::Range;
use letters_sheets::model::*;
use letters_sheets::package::{write_zip, Package, Part};
use letters_sheets::xlsx::{read_xlsx, write_xlsx};

fn sample_workbook() -> Workbook {
    let mut workbook = Workbook::new();
    let bold = workbook.styles.derive(0, |style| {
        style.font.bold = true;
        style.number_format = "0.00%".into();
        style.fill = Fill::solid(Color::rgb("FFFF00"));
    });
    let sheet = &mut workbook.sheets[0];
    sheet.cells.set(0, 0, Cell::value(Value::text("Item")));
    sheet.cells.set(0, 1, Cell::value(Value::text("Price")));
    sheet.cells.set(1, 0, Cell::value(Value::text("Pen")));
    sheet.cells.set(1, 1, Cell::value(Value::Number(1.25)));
    sheet.cells.set(2, 0, Cell::value(Value::Bool(true)));
    sheet
        .cells
        .set(2, 1, Cell::value(Value::Error(ErrorValue::Div0)));
    let mut total = Cell::formula("SUM(B2:B3)+XLOOKUP(1,A:A,B:B)");
    total.value = Value::Number(1.25);
    total.style = bold;
    sheet.cells.set(3, 1, total);
    sheet.merged.push(Range::parse("C1:D2").unwrap());
    sheet.set_column_width(0, 20.0);
    sheet.rows.insert(
        4,
        RowInfo {
            height: Some(30.0),
            custom_height: true,
            ..RowInfo::default()
        },
    );
    sheet.view.frozen = Some(FrozenPanes { rows: 1, cols: 0 });
    sheet.view.tab_color = Some(Color::rgb("FF0000"));
    sheet.view.zoom = 125;
    sheet.validations.push(DataValidation {
        ranges: vec![Range::parse("A2:A10").unwrap()],
        kind: "list".into(),
        formula1: Some("\"Pen,Pencil\"".into()),
        allow_blank: true,
        ..DataValidation::default()
    });
    sheet.conditional_formats.push(ConditionalFormat {
        ranges: vec![Range::parse("B2:B10").unwrap()],
        rules: vec![ConditionalRule {
            kind: "cellIs".into(),
            operator: Some("greaterThan".into()),
            formulas: vec!["1".into()],
            priority: 1,
            ..ConditionalRule::default()
        }],
    });
    sheet.hyperlinks.push(Hyperlink {
        range: Range::parse("A1").unwrap(),
        target: Some("https://example.com/a?b=1&c=2".into()),
        ..Hyperlink::default()
    });
    sheet.comments.push(Comment {
        row: 1,
        col: 1,
        author: "Ann".into(),
        text: "Price in euros".into(),
    });
    sheet.print.area = vec![Range::parse("A1:D10").unwrap()];
    sheet.print.title_rows = Some((0, 0));
    sheet.print.orientation = Orientation::Landscape;
    sheet.print.row_breaks = vec![20];
    workbook.sheets.push(Sheet::new("My data"));
    workbook.sheets[1]
        .cells
        .set(0, 0, Cell::formula("Sheet1!B2*2"));
    workbook.defined_names.push(DefinedName {
        name: "Prices".into(),
        formula: "Sheet1!$B$2:$B$3".into(),
        ..DefinedName::default()
    });
    workbook.metadata.title = Some("Shop".into());
    workbook
}

#[test]
fn every_modelled_feature_survives_writing_and_reading() {
    let workbook = sample_workbook();
    let bytes = write_xlsx(&workbook).unwrap();
    let back = read_xlsx(&bytes).unwrap();
    let (a, b) = (&workbook.sheets[0], &back.sheets[0]);
    assert_eq!(back.sheets.len(), 2);
    assert_eq!(back.sheets[1].name, "My data");
    assert_eq!(b.cells.value(1, 1), &Value::Number(1.25));
    assert_eq!(b.cells.value(2, 0), &Value::Bool(true));
    assert_eq!(b.cells.value(2, 1), &Value::Error(ErrorValue::Div0));
    assert_eq!(
        b.cells.get(3, 1).unwrap().formula.as_ref().unwrap().text,
        "SUM(B2:B3)+XLOOKUP(1,A:A,B:B)"
    );
    let style = back.styles.get(b.cells.get(3, 1).unwrap().style);
    assert!(style.font.bold);
    assert_eq!(style.number_format, "0.00%");
    assert_eq!(b.merged, a.merged);
    assert_eq!(b.column_width(0), 20.0);
    assert_eq!(b.row_height(4), 30.0);
    assert_eq!(b.view.frozen, a.view.frozen);
    assert_eq!(b.view.zoom, 125);
    assert_eq!(
        b.view.tab_color.as_ref().unwrap().rgb.as_deref(),
        Some("FFFF0000")
    );
    assert_eq!(b.validations, a.validations);
    assert_eq!(b.conditional_formats, a.conditional_formats);
    assert_eq!(b.hyperlinks, a.hyperlinks);
    assert_eq!(b.comments, a.comments);
    assert_eq!(b.print.area, a.print.area);
    assert_eq!(b.print.title_rows, Some((0, 0)));
    assert_eq!(b.print.orientation, Orientation::Landscape);
    assert_eq!(b.print.row_breaks, vec![20]);
    assert_eq!(back.defined_names[0].formula, "Sheet1!$B$2:$B$3");
    assert_eq!(back.metadata.title.as_deref(), Some("Shop"));
}

#[test]
fn saving_twice_gives_the_same_bytes() {
    let first = write_xlsx(&sample_workbook()).unwrap();
    let second = write_xlsx(&read_xlsx(&first).unwrap()).unwrap();
    let third = write_xlsx(&read_xlsx(&second).unwrap()).unwrap();
    assert_eq!(second, third);
}

/// a hand-made package with things the model does not know
fn package_with_unknowns() -> Vec<u8> {
    let parts = vec![
        Part { name: "[Content_Types].xml".into(), bytes: br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="bin" ContentType="application/vnd.ms-office.vbaProject"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.ms-excel.sheet.macroEnabled.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/pivotTables/pivotTable1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml"/></Types>"#.to_vec() },
        Part { name: "_rels/.rels".into(), bytes: br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.to_vec() },
        Part { name: "xl/workbook.xml".into(), bytes: br#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><fileVersion appName="xl"/><workbookPr codeName="ThisWorkbook"/><sheets><sheet name="Data" sheetId="7" r:id="rId1"/></sheets><extLst><ext uri="{x}"><keep/></ext></extLst></workbook>"#.to_vec() },
        Part { name: "xl/_rels/workbook.xml.rels".into(), bytes: br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId9" Type="http://schemas.microsoft.com/office/2006/relationships/vbaProject" Target="vbaProject.bin"/></Relationships>"#.to_vec() },
        Part { name: "xl/worksheets/sheet1.xml".into(), bytes: br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetPr codeName="Sheet1"><tabColor rgb="FF00FF00"/></sheetPr><sheetViews><sheetView workbookViewId="0" view="pageLayout"/></sheetViews><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>2</v></c></row></sheetData><sheetProtection sheet="1" objects="1"/><mergeCells count="1"><mergeCell ref="D1:E1"/></mergeCells><ignoredErrors><ignoredError sqref="A1" numberStoredAsText="1"/></ignoredErrors><extLst><ext uri="{y}"><mine/></ext></extLst></worksheet>"#.to_vec() },
        Part { name: "xl/vbaProject.bin".into(), bytes: vec![1, 2, 3, 4] },
        Part { name: "xl/pivotTables/pivotTable1.xml".into(), bytes: b"<pivotTableDefinition/>".to_vec() },
    ];
    write_zip(&parts, &[]).unwrap()
}

fn part_text(bytes: &[u8], name: &str) -> String {
    String::from_utf8(Package::read(bytes).unwrap().bytes(name).unwrap().to_vec()).unwrap()
}

#[test]
fn unknown_parts_and_elements_are_kept_after_an_edit() {
    let mut workbook = read_xlsx(&package_with_unknowns()).unwrap();
    assert!(workbook.has_macros);
    workbook.sheets[0]
        .cells
        .set(0, 0, Cell::value(Value::Number(42.0)));
    let bytes = write_xlsx(&workbook).unwrap();
    let package = Package::read(&bytes).unwrap();
    assert_eq!(package.bytes("xl/vbaProject.bin").unwrap(), &[1, 2, 3, 4]);
    assert_eq!(
        package.bytes("xl/pivotTables/pivotTable1.xml").unwrap(),
        b"<pivotTableDefinition/>"
    );
    let sheet = part_text(&bytes, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains("<sheetPr codeName=\"Sheet1\"><tabColor rgb=\"FF00FF00\"/></sheetPr>"));
    assert!(sheet.contains("view=\"pageLayout\""));
    assert!(sheet.contains("<sheetProtection sheet=\"1\" objects=\"1\"/>"));
    assert!(sheet.contains("<ignoredErrors>"));
    assert!(sheet.contains("<mine/>"));
    assert!(sheet.contains("<v>42</v>"));
    let book = part_text(&bytes, "xl/workbook.xml");
    assert!(book.contains("codeName=\"ThisWorkbook\""));
    assert!(book.contains("sheetId=\"7\""));
    assert!(book.contains("<keep/>"));
    let rels = part_text(&bytes, "xl/_rels/workbook.xml.rels");
    assert!(rels.contains("vbaProject.bin"));
    let types = part_text(&bytes, "[Content_Types].xml");
    assert!(types.contains("macroEnabled"));
    assert!(types.contains("pivotTable+xml"));
}

#[test]
fn broken_files_give_errors_not_panics() {
    let good = package_with_unknowns();
    for cut in [10, 100, good.len() / 2, good.len() - 10] {
        let _ = read_xlsx(&good[..cut]);
    }
    let package = Package::read(&good).unwrap();
    for victim in 0..package.parts.len() {
        let mut parts = package.parts.clone();
        parts[victim].bytes = b"<broken".to_vec();
        let bytes = write_zip(&parts, &[]).unwrap();
        let _ = read_xlsx(&bytes);
    }
    assert!(read_xlsx(b"not a zip").is_err());
}
