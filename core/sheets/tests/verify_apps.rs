//! the verification harness against the reference applications (run with
//! `cargo test -p letters-sheets --release --test verify_apps -- --ignored --nocapture`).
//! it writes thousands of formula cases and number formats into
//! workbooks, has excel (when installed) and libreoffice calculate and
//! show them, and compares with letters cell by cell. the report goes to
//! target-sheets/harness/report.md.

mod harness;

use letters_sheets::calc::recalculate;
use letters_sheets::model::*;
use letters_sheets::numfmt::{format_value, Locale};
use letters_sheets::xlsx::{read_xlsx, write_xlsx};

use harness::apps;
use harness::cases::{all_cases, data_sheet, Case};
use harness::report::{markdown, same, Outcome};

/// the column of the cases sheet holding the formulas (C, one based)
const FORMULA_COLUMN: u32 = 3;

fn cases_workbook(cases: &[Case]) -> Workbook {
    let mut book = Workbook::new();
    book.sheets[0].name = "Cases".into();
    data_sheet(&mut book);
    for (row, case) in cases.iter().enumerate() {
        let row = row as u32;
        let sheet = &mut book.sheets[0];
        sheet
            .cells
            .set(row, 0, Cell::value(Value::text(case.family)));
        sheet
            .cells
            .set(row, 1, Cell::value(Value::text(case.formula.clone())));
        let mut cell = Cell::formula(&case.formula);
        // a dynamic array formula, so excel 365 uses array semantics as
        // letters does, rather than the implicit intersection of old files
        if let Some(formula) = cell.formula.as_mut() {
            formula.kind = FormulaKind::Dynamic {
                spill: Some(letters_sheets::Range::cell(letters_sheets::CellRef::new(
                    row, 2,
                ))),
            };
        }
        sheet.cells.set(row, 2, cell);
    }
    book.calculation.full_calc_on_load = true;
    book
}

fn column_values(book: &Workbook, sheet: &str, count: usize) -> Vec<Value> {
    let index = book.sheet_index(sheet).expect("the sheet is there");
    (0..count as u32)
        .map(|row| book.value(index, row, FORMULA_COLUMN - 1))
        .collect()
}

#[test]
#[ignore = "needs Microsoft Excel on windows and LibreOffice"]
fn formulas_agree_with_excel_and_libreoffice() {
    let cases = all_cases();
    let book = cases_workbook(&cases);
    let mut ours = book.clone();
    recalculate(&mut ours);
    let ours = column_values(&ours, "Cases", cases.len());
    let folder = apps::work_folder("formulas");
    let path = folder.join("cases.xlsx");
    std::fs::write(&path, write_xlsx(&book).unwrap()).unwrap();
    let excel: Option<Vec<Value>> = if apps::excel_available() {
        let rows = apps::excel_values(&path, "Cases", FORMULA_COLUMN).expect("excel answers");
        let mut values = vec![Value::Blank; cases.len()];
        for (row, value, _) in rows {
            if let Some(slot) = values.get_mut(row as usize - 1) {
                *slot = value;
            }
        }
        Some(values)
    } else {
        None
    };
    let libreoffice = apps::libreoffice_path().map(|_| {
        let out = folder.join("lo");
        std::fs::create_dir_all(&out).unwrap();
        apps::libreoffice_convert(&path, "xlsx:Calc MS Excel 2007 XML", &out)
            .expect("libreoffice converts");
        let converted = read_xlsx(&std::fs::read(out.join("cases.xlsx")).unwrap()).unwrap();
        column_values(&converted, "Cases", cases.len())
    });
    let outcomes: Vec<Outcome> = cases
        .iter()
        .enumerate()
        .map(|(index, case)| Outcome {
            family: case.family.to_string(),
            formula: case.formula.clone(),
            ours: ours[index].clone(),
            excel: excel.as_ref().map(|v| v[index].clone()),
            libreoffice: libreoffice.as_ref().map(|v| v[index].clone()),
        })
        .collect();
    let report = markdown(&outcomes);
    std::fs::write(folder.join("report.md"), &report).unwrap();
    println!("{}", report.split("### Disagreements").next().unwrap_or(""));
    let agree = outcomes
        .iter()
        .filter(|o| o.excel.as_ref().is_some_and(|e| same(&o.ours, e)))
        .count();
    println!(
        "{agree} of {} agree with excel; details in {}",
        outcomes.len(),
        folder.join("report.md").display()
    );
}

/// (value, format code) pairs for the display check
fn format_cases() -> Vec<(f64, &'static str)> {
    let values = [
        0.0,
        1.0,
        -1.0,
        1234.5678,
        -1234.5678,
        0.5,
        0.123456,
        1e6,
        0.00001,
        45352.75,
        1.5,
        123456789012.0,
        3.14158,
        2.675,
        1e-10,
    ];
    let codes = [
        "General",
        "0",
        "0.00",
        "#,##0",
        "#,##0.00",
        "0%",
        "0.00%",
        "0.00E+00",
        "##0.0E+0",
        "# ?/?",
        "# ??/??",
        "?/?",
        "# ?/8",
        "$#,##0.00",
        "$#,##0.00_);($#,##0.00)",
        "#,##0;[Red](#,##0)",
        "0.0,,\"M\"",
        "#,##0,\"K\"",
        "000-0000",
        "0.0#",
        "0.0?",
        "#.##",
        "m/d/yyyy",
        "d-mmm-yy",
        "dddd, mmmm d, yyyy",
        "yyyy-mm-dd hh:mm:ss",
        "h:mm AM/PM",
        "h:mm:ss",
        "[h]:mm:ss",
        "[mm]:ss",
        "mm:ss.0",
        "\"Total: \"0.00",
        "0;-0;\"zero\"",
        "[>=1000]#,##0;[<0]\"neg\";0.0",
        "_(* #,##0.00_);_(* (#,##0.00);_(* \"-\"??_);_(@_)",
        "@",
        "0.000",
        "#",
        "0 \"kg\"",
        "mmm",
        "mmmmm",
        "d",
        "ddd",
        "yy",
        "AM/PM h",
        "[$€-407] #,##0.00",
    ];
    let mut cases = Vec::new();
    for value in values {
        for code in codes {
            cases.push((value, code));
        }
    }
    cases
}

/// splits one csv line written by libreoffice (quotes around text)
fn csv_fields(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                fields.last_mut().unwrap().push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(String::new()),
            c => fields.last_mut().unwrap().push(c),
        }
    }
    fields
}

#[test]
#[ignore = "needs Microsoft Excel on windows and LibreOffice"]
fn number_formats_show_what_excel_and_libreoffice_show() {
    let cases = format_cases();
    let mut book = Workbook::new();
    book.sheets[0].name = "Formats".into();
    book.sheets[0].set_column_width(2, 60.0);
    for (row, (value, code)) in cases.iter().enumerate() {
        let style = book
            .styles
            .derive(0, |s| s.number_format = code.to_string());
        let sheet = &mut book.sheets[0];
        sheet
            .cells
            .set(row as u32, 0, Cell::value(Value::Number(*value)));
        sheet
            .cells
            .set(row as u32, 1, Cell::value(Value::text(*code)));
        sheet.cells.set(
            row as u32,
            2,
            Cell {
                value: Value::Number(*value),
                style,
                ..Cell::default()
            },
        );
    }
    let folder = apps::work_folder("formats");
    let path = folder.join("formats.xlsx");
    std::fs::write(&path, write_xlsx(&book).unwrap()).unwrap();
    let excel = apps::excel_available()
        .then(|| apps::excel_values(&path, "Formats", 3).expect("excel answers"));
    let libreoffice = apps::libreoffice_path().map(|_| {
        let out = folder.join("lo");
        std::fs::create_dir_all(&out).unwrap();
        apps::libreoffice_convert(
            &path,
            "csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true",
            &out,
        )
        .expect("libreoffice converts");
        let text = std::fs::read_to_string(out.join("formats.csv")).unwrap();
        text.lines()
            .map(|line| csv_fields(line).get(2).cloned().unwrap_or_default())
            .collect::<Vec<_>>()
    });
    // excel fills a cell's width for "*" fills and "#" overflow marks;
    // those depend on the column, so runs of them are compared as one
    let fitted = |text: String, code: &str| {
        if !text.is_empty() && text.chars().all(|c| c == '#') {
            return "#".to_string();
        }
        if code.contains('*') {
            return text.trim_start().to_string();
        }
        text
    };
    let mut outcomes = Vec::new();
    for (row, (value, code)) in cases.iter().enumerate() {
        let ours = format_value(
            &Value::Number(*value),
            code,
            &Locale::default(),
            DateSystem::Excel1900,
        )
        .text;
        let excel_text = excel
            .as_ref()
            .and_then(|rows| rows.iter().find(|r| r.0 as usize == row + 1))
            .map(|r| Value::Text(fitted(r.2.clone(), code)));
        let lo_text = libreoffice
            .as_ref()
            .map(|rows| Value::Text(fitted(rows.get(row).cloned().unwrap_or_default(), code)));
        outcomes.push(Outcome {
            family: "number formats".into(),
            formula: format!("{value} as {code}"),
            ours: Value::Text(fitted(ours, code)),
            excel: excel_text,
            libreoffice: lo_text,
        });
    }
    let report = markdown(&outcomes);
    std::fs::write(folder.join("report.md"), &report).unwrap();
    println!("{}", report.split("### Disagreements").next().unwrap_or(""));
}
