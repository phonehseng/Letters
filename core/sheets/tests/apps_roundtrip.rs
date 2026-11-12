//! files written by excel and by libreoffice, read by letters, changed in
//! one cell, written, and opened again by the same application: nothing
//! else may be lost and nothing may need repair. run with
//! `cargo test -p letters-sheets --release --test apps_roundtrip -- --ignored --nocapture --test-threads=1`.

mod harness;

use std::path::Path;

use harness::apps;
use letters_sheets::address::Range;
use letters_sheets::model::*;
use letters_sheets::ods::{read_ods, write_ods};
use letters_sheets::xlsx::{read_xlsx, write_xlsx};

/// a workbook with a little of everything
fn rich_workbook() -> Workbook {
    let mut book = Workbook::new();
    let money = book.styles.derive(0, |s| {
        s.number_format = "$#,##0.00".into();
        s.font.bold = true;
        s.fill = Fill::solid(Color::rgb("DDEBF7"));
    });
    let sheet = &mut book.sheets[0];
    for (row, (item, price)) in [("Item", 0.0), ("Pen", 1.5), ("Ink", 4.25), ("Pad", 2.0)]
        .iter()
        .enumerate()
    {
        sheet
            .cells
            .set(row as u32, 0, Cell::value(Value::text(*item)));
        if row > 0 {
            sheet.cells.set(
                row as u32,
                1,
                Cell {
                    value: Value::Number(*price),
                    style: money,
                    ..Cell::default()
                },
            );
        }
    }
    sheet.cells.set(0, 1, Cell::value(Value::text("Price")));
    sheet.cells.set(
        4,
        1,
        Cell {
            formula: Some(Formula::new("SUM(B2:B4)")),
            value: Value::Number(7.75),
            style: money,
            ..Cell::default()
        },
    );
    sheet.merged.push(Range::parse("D1:E1").unwrap());
    sheet
        .cells
        .set(0, 3, Cell::value(Value::text("Merged title")));
    sheet.validations.push(DataValidation {
        ranges: vec![Range::parse("C2:C4").unwrap()],
        kind: "list".into(),
        formula1: Some("\"Yes,No\"".into()),
        allow_blank: true,
        show_error_message: true,
        ..DataValidation::default()
    });
    sheet.conditional_formats.push(ConditionalFormat {
        ranges: vec![Range::parse("B2:B4").unwrap()],
        rules: vec![ConditionalRule {
            kind: "cellIs".into(),
            operator: Some("greaterThan".into()),
            formulas: vec!["2".into()],
            priority: 1,
            ..ConditionalRule::default()
        }],
    });
    sheet.comments.push(Comment {
        row: 1,
        col: 0,
        author: "Ann".into(),
        text: "Blue ink".into(),
    });
    sheet.hyperlinks.push(Hyperlink {
        range: Range::parse("A3").unwrap(),
        target: Some("https://example.com/".into()),
        ..Hyperlink::default()
    });
    sheet.view.frozen = Some(FrozenPanes { rows: 1, cols: 0 });
    book.sheets.push(Sheet::new("Notes"));
    book.sheets[1].cells.set(
        0,
        0,
        Cell {
            formula: Some(Formula::new("Sheet1!B5*2")),
            value: Value::Number(15.5),
            ..Cell::default()
        },
    );
    book.defined_names.push(DefinedName {
        name: "Total".into(),
        formula: "Sheet1!$B$5".into(),
        ..DefinedName::default()
    });
    book
}

fn excel_counts(path: &Path) -> String {
    let out = path.with_extension("counts.tsv");
    apps::excel("check", path, &out, "", 1).expect("excel opens the file");
    std::fs::read_to_string(out).unwrap()
}

/// what excel saw, without the file name (it differs by design)
fn without_name(counts: &str) -> String {
    counts
        .lines()
        .filter(|line| !line.starts_with("name\t"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// the features letters itself reads from a workbook
fn features(book: &Workbook) -> String {
    book.sheets
        .iter()
        .map(|s| {
            format!(
                "{}: merged {} validations {} formats {} notes {} links {} drawings {}",
                s.name,
                s.merged.len(),
                s.validations.len(),
                s.conditional_formats.len(),
                s.comments.len(),
                s.hyperlinks.len(),
                s.drawings.len()
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn edit_one_cell(book: &mut Workbook) {
    book.set_value(0, 3, 0, Value::text("Pencil"));
}

#[test]
#[ignore]
fn excel_files_survive_a_letters_edit() {
    if !apps::excel_available() {
        println!("excel is not installed; skipped");
        return;
    }
    let folder = apps::work_folder("excel-roundtrip");
    let ours = folder.join("ours.xlsx");
    std::fs::write(&ours, write_xlsx(&rich_workbook()).unwrap()).unwrap();
    // excel opens what letters writes, without repairs
    let first = excel_counts(&ours);
    assert!(!first.contains("Repaired"), "{first}");
    let excel_file = folder.join("excel.xlsx");
    apps::excel("decorate", &ours, &excel_file, "", 1).expect("excel saves its own copy");
    let before = excel_counts(&excel_file);
    let mut book = read_xlsx(&std::fs::read(&excel_file).unwrap()).unwrap();
    assert!(
        book.sheets[0]
            .drawings
            .iter()
            .any(|d| matches!(d, DrawingObject::Chart { .. })),
        "the chart is seen"
    );
    edit_one_cell(&mut book);
    let edited = folder.join("edited.xlsx");
    std::fs::write(&edited, write_xlsx(&book).unwrap()).unwrap();
    let after = excel_counts(&edited);
    println!("excel before:\n{before}\nexcel after a letters edit:\n{after}");
    assert!(!after.contains("Repaired"));
    assert_eq!(without_name(&after), without_name(&before));
}

#[test]
#[ignore]
fn libreoffice_files_survive_a_letters_edit() {
    if apps::libreoffice_path().is_none() {
        println!("libreoffice is not installed; skipped");
        return;
    }
    let folder = apps::work_folder("libreoffice-roundtrip");
    let ours = folder.join("ours.xlsx");
    std::fs::write(&ours, write_xlsx(&rich_workbook()).unwrap()).unwrap();
    let lo = folder.join("lo");
    std::fs::create_dir_all(&lo).unwrap();
    apps::libreoffice_convert(&ours, "ods", &lo).expect("libreoffice writes ods");
    apps::libreoffice_convert(&ours, "xlsx:Calc MS Excel 2007 XML", &folder.join("lo"))
        .expect("libreoffice writes xlsx");
    for (name, ods) in [("ours.ods", true), ("ours.xlsx", false)] {
        let source = lo.join(name);
        let bytes = std::fs::read(&source).unwrap();
        let mut book = if ods {
            read_ods(&bytes).unwrap()
        } else {
            read_xlsx(&bytes).unwrap()
        };
        let before = features(&book);
        edit_one_cell(&mut book);
        let edited = folder.join(format!("edited-{name}"));
        std::fs::write(
            &edited,
            if ods {
                write_ods(&book).unwrap()
            } else {
                write_xlsx(&book).unwrap()
            },
        )
        .unwrap();
        // libreoffice reads the edited file and writes it again
        let check = folder.join(format!("check-{}", if ods { "ods" } else { "xlsx" }));
        std::fs::create_dir_all(&check).unwrap();
        let filter = if ods {
            "ods"
        } else {
            "xlsx:Calc MS Excel 2007 XML"
        };
        apps::libreoffice_convert(&edited, filter, &check)
            .expect("libreoffice opens the edited file");
        let again_bytes = std::fs::read(check.join(format!("edited-{name}"))).unwrap();
        let again = if ods {
            read_ods(&again_bytes).unwrap()
        } else {
            read_xlsx(&again_bytes).unwrap()
        };
        println!(
            "{name}: before {before}\n{name}: after  {}",
            features(&again)
        );
        assert_eq!(features(&again), before, "{name}");
        assert_eq!(again.value_at("A4"), Value::text("Pencil"));
    }
}

#[test]
#[ignore]
fn xlsx_and_ods_convert_both_ways_through_libreoffice() {
    if apps::libreoffice_path().is_none() {
        return;
    }
    let folder = apps::work_folder("conversion");
    let book = rich_workbook();
    let ods = folder.join("ours.ods");
    std::fs::write(&ods, write_ods(&book).unwrap()).unwrap();
    let out = folder.join("lo");
    std::fs::create_dir_all(&out).unwrap();
    apps::libreoffice_convert(&ods, "xlsx:Calc MS Excel 2007 XML", &out)
        .expect("libreoffice reads our ods");
    let converted = read_xlsx(&std::fs::read(out.join("ours.xlsx")).unwrap()).unwrap();
    assert_eq!(converted.value_at("B5"), Value::Number(7.75));
    assert_eq!(converted.value_at("Notes!A1"), Value::Number(15.5));
    // conditional formats are not written to ods yet (see the readme)
    let without_formats = |text: String| text.replace("formats 1", "formats 0");
    assert_eq!(
        without_formats(features(&converted)),
        without_formats(features(&book))
    );
}
