//! ods files written and read back, conversions between xlsx and ods,
//! and broken ods input.

use letters_sheets::address::Range;
use letters_sheets::calc::recalculate;
use letters_sheets::model::*;
use letters_sheets::ods::{read_ods, write_ods};
use letters_sheets::package::{write_zip, Package};
use letters_sheets::xlsx::{read_xlsx, write_xlsx};

fn sample() -> Workbook {
    let mut book = Workbook::new();
    let percent = book.styles.derive(0, |s| {
        s.number_format = "0.0%".into();
        s.font.bold = true;
        s.fill = Fill::solid(Color::rgb("FFFF00"));
    });
    let date = book
        .styles
        .derive(0, |s| s.number_format = "yyyy-mm-dd".into());
    let sheet = &mut book.sheets[0];
    sheet
        .cells
        .set(0, 0, Cell::value(Value::text("Name  with  spaces")));
    sheet.cells.set(
        0,
        1,
        Cell {
            value: Value::Number(0.25),
            style: percent,
            ..Cell::default()
        },
    );
    sheet.cells.set(1, 0, Cell::value(Value::Bool(true)));
    sheet.cells.set(
        1,
        1,
        Cell {
            value: Value::Number(45352.0),
            style: date,
            ..Cell::default()
        },
    );
    sheet.cells.set(2, 0, Cell::formula("SUM(B1:B2)+Other!A1"));
    sheet
        .cells
        .set(2, 1, Cell::formula("IF(A2,\"yes\",\"no\")"));
    sheet.merged.push(Range::parse("D1:E2").unwrap());
    sheet.set_column_width(0, 25.0);
    sheet.rows.insert(
        5,
        RowInfo {
            height: Some(30.0),
            custom_height: true,
            ..RowInfo::default()
        },
    );
    sheet.comments.push(Comment {
        row: 0,
        col: 0,
        author: "Ann".into(),
        text: "hello".into(),
    });
    sheet.hyperlinks.push(Hyperlink {
        range: Range::parse("B1").unwrap(),
        target: Some("https://example.com".into()),
        ..Hyperlink::default()
    });
    sheet.validations.push(DataValidation {
        ranges: vec![Range::parse("F1:F3").unwrap()],
        kind: "list".into(),
        formula1: Some("\"Yes,No\"".into()),
        allow_blank: true,
        show_error_message: true,
        ..DataValidation::default()
    });
    sheet.view.frozen = Some(FrozenPanes { rows: 1, cols: 0 });
    book.sheets.push(Sheet::new("Other"));
    book.sheets[1]
        .cells
        .set(0, 0, Cell::value(Value::Number(10.0)));
    book.defined_names.push(DefinedName {
        name: "Total".into(),
        formula: "Sheet1!$A$3".into(),
        ..DefinedName::default()
    });
    book.metadata.title = Some("Sample".into());
    recalculate(&mut book);
    book
}

#[test]
fn ods_keeps_what_the_model_holds() {
    let book = sample();
    let back = read_ods(&write_ods(&book).unwrap()).unwrap();
    let (a, b) = (&book.sheets[0], &back.sheets[0]);
    assert_eq!(back.sheets.len(), 2);
    assert_eq!(b.cells.value(0, 0), &Value::text("Name  with  spaces"));
    assert_eq!(b.cells.value(1, 0), &Value::Bool(true));
    assert_eq!(b.cells.value(1, 1), &Value::Number(45352.0));
    assert_eq!(
        b.cells.get(2, 0).unwrap().formula.as_ref().unwrap().text,
        "SUM(B1:B2)+Other!A1"
    );
    let style = back.styles.get(b.cells.get(0, 1).unwrap().style);
    assert_eq!(style.number_format, "0.0%");
    assert!(style.font.bold);
    assert_eq!(
        back.styles
            .get(b.cells.get(1, 1).unwrap().style)
            .number_format,
        "yyyy-mm-dd"
    );
    assert_eq!(b.merged, a.merged);
    assert!(
        (b.column_width(0) - 25.0).abs() < 0.2,
        "{}",
        b.column_width(0)
    );
    assert!((b.row_height(5) - 30.0).abs() < 0.1);
    assert_eq!(b.comments, a.comments);
    assert_eq!(b.hyperlinks[0].target, a.hyperlinks[0].target);
    assert_eq!(b.validations[0].formula1, a.validations[0].formula1);
    assert_eq!(b.validations[0].ranges, a.validations[0].ranges);
    assert_eq!(b.view.frozen, a.view.frozen);
    assert_eq!(back.defined_names[0].formula, "Sheet1!$A$3");
    assert_eq!(back.metadata.title.as_deref(), Some("Sample"));
}

#[test]
fn xlsx_to_ods_and_back_keeps_values_formulas_and_formats() {
    let book = sample();
    let through_ods =
        read_ods(&write_ods(&read_xlsx(&write_xlsx(&book).unwrap()).unwrap()).unwrap()).unwrap();
    let again = read_xlsx(&write_xlsx(&through_ods).unwrap()).unwrap();
    let mut recalculated = again.clone();
    recalculate(&mut recalculated);
    assert_eq!(recalculated.value_at("A3"), book.value_at("A3"));
    assert_eq!(recalculated.value_at("B3"), Value::text("yes"));
    assert_eq!(
        again
            .styles
            .get(again.sheets[0].cells.get(0, 1).unwrap().style)
            .number_format,
        "0.0%"
    );
}

#[test]
fn the_package_follows_opendocument_rules() {
    let bytes = write_ods(&sample()).unwrap();
    // mimetype first, stored, so tools can recognise the file
    assert_eq!(&bytes[30..38], b"mimetype");
    let package = Package::read(&bytes).unwrap();
    let manifest =
        String::from_utf8(package.bytes("META-INF/manifest.xml").unwrap().to_vec()).unwrap();
    assert!(manifest.contains("content.xml"));
}

#[test]
fn broken_ods_files_give_errors_not_panics() {
    let good = write_ods(&sample()).unwrap();
    for cut in [10, 200, good.len() / 2] {
        let _ = read_ods(&good[..cut]);
    }
    let package = Package::read(&good).unwrap();
    for victim in 0..package.parts.len() {
        let mut parts = package.parts.clone();
        let half = parts[victim].bytes.len() / 2;
        parts[victim].bytes.truncate(half);
        let _ = read_ods(&write_zip(&parts, &["mimetype"]).unwrap());
    }
}

/// libreoffice's conditional formats as it writes them, put into a file
/// letters wrote: a "Bad" style and B1:B3 greater than 50 in it
const LIBREOFFICE_RULE: &str = r#"<calcext:conditional-formats><calcext:conditional-format calcext:target-range-address="Sheet1.A1:Sheet1.A3"><calcext:condition calcext:apply-style-name="Bad" calcext:value="&gt;50" calcext:base-cell-address="Sheet1.A1"/></calcext:conditional-format></calcext:conditional-formats>"#;
const BAD_STYLE: &str = r##"<style:style style:name="Bad" style:family="table-cell"><style:table-cell-properties fo:background-color="#ffc7ce"/><style:text-properties fo:color="#9c0006"/></style:style>"##;

fn libreoffice_file() -> Vec<u8> {
    let mut book = Workbook::new();
    for (row, mark) in [30.0, 60.0, 90.0].into_iter().enumerate() {
        book.sheets[0]
            .cells
            .set(row as u32, 0, Cell::value(Value::Number(mark)));
    }
    let package = Package::read(&write_ods(&book).unwrap()).unwrap();
    let mut parts = package.parts.clone();
    let content = parts.iter_mut().find(|p| p.name == "content.xml").unwrap();
    let xml = String::from_utf8(content.bytes.clone()).unwrap();
    let xml = xml
        .replacen(
            "</office:automatic-styles>",
            &format!("{BAD_STYLE}</office:automatic-styles>"),
            1,
        )
        .replacen(
            "</table:table>",
            &format!("{LIBREOFFICE_RULE}</table:table>"),
            1,
        );
    content.bytes = xml.into_bytes();
    write_zip(&parts, &["mimetype"]).unwrap()
}

#[test]
fn libreoffice_conditional_formats_are_drawn_and_saved_as_they_came() {
    use letters_sheets::numfmt::Locale;
    use letters_sheets::view::cells_in;
    let book = read_ods(&libreoffice_file()).unwrap();
    let view = Range::parse("A1:A3").unwrap();
    let fills = |book: &Workbook| -> Vec<Option<String>> {
        cells_in(book, 0, view, &Locale::default())
            .into_iter()
            .map(|cell| cell.conditional.and_then(|look| look.fill))
            .collect()
    };
    let pink = Some("#FFC7CE".to_string());
    assert_eq!(fills(&book), vec![None, pink.clone(), pink.clone()]);
    // saved to ods, the rule is the file's own xml
    let saved = write_ods(&book).unwrap();
    let content = Package::read(&saved).unwrap();
    let content = String::from_utf8(content.bytes("content.xml").unwrap().to_vec()).unwrap();
    assert_eq!(content.matches(LIBREOFFICE_RULE).count(), 1);
    // converted to xlsx, it becomes excel's rule with its differential format
    let as_xlsx = read_xlsx(&write_xlsx(&book).unwrap()).unwrap();
    assert_eq!(
        as_xlsx.sheets[0].conditional_formats,
        book.sheets[0].conditional_formats
    );
    assert_eq!(fills(&as_xlsx), vec![None, pink.clone(), pink]);
}
