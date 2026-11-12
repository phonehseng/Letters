//! timings and memory for a 200,000-row workbook (run in release:
//! `cargo test -p letters-sheets --release --test large -- --ignored --nocapture`).

use std::time::Instant;

use letters_sheets::address::Range;
use letters_sheets::analysis::StatsCache;
use letters_sheets::calc::Engine;
use letters_sheets::model::*;
use letters_sheets::numfmt::Locale;
use letters_sheets::view::cells_in;
use letters_sheets::xlsx::{read_xlsx, write_xlsx};

const ROWS: u32 = 200_000;

/// the process's memory in megabytes now and at its peak (windows only;
/// zeros elsewhere)
fn memory_megabytes() -> (f64, f64) {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct Counters {
            size: u32,
            page_faults: u32,
            peak_working_set: usize,
            working_set: usize,
            quota_peak_paged: usize,
            quota_paged: usize,
            quota_peak_nonpaged: usize,
            quota_nonpaged: usize,
            pagefile: usize,
            peak_pagefile: usize,
        }
        extern "system" {
            fn GetCurrentProcess() -> isize;
            fn K32GetProcessMemoryInfo(process: isize, counters: *mut Counters, size: u32) -> i32;
        }
        let mut counters = Counters {
            size: std::mem::size_of::<Counters>() as u32,
            ..Counters::default()
        };
        // the call fills the struct we pass with its size; nothing is kept
        let ok =
            unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.size) };
        if ok != 0 {
            let megabytes = |bytes: usize| bytes as f64 / 1_048_576.0;
            return (
                megabytes(counters.working_set),
                megabytes(counters.peak_working_set),
            );
        }
    }
    (0.0, 0.0)
}

fn large_workbook() -> Workbook {
    let mut book = Workbook::new();
    let date = book
        .styles
        .derive(0, |s| s.number_format = "yyyy-mm-dd".into());
    let sheet = &mut book.sheets[0];
    for row in 0..ROWS {
        let r = row + 1;
        sheet.cells.set(
            row,
            0,
            Cell::value(Value::Number(f64::from(row % 1000) * 1.5)),
        );
        sheet.cells.set(
            row,
            1,
            Cell::value(Value::text(format!("item {}", row % 5000))),
        );
        sheet.cells.set(
            row,
            2,
            Cell {
                value: Value::Number(45000.0 + f64::from(row % 365)),
                style: date,
                ..Cell::default()
            },
        );
        sheet.cells.set(row, 3, Cell::formula(&format!("A{r}*2+1")));
        sheet.cells.set(
            row,
            4,
            Cell::formula(&format!("IF(A{r}>700,\"high\",\"low\")")),
        );
    }
    sheet.cells.set(0, 6, Cell::formula("SUM(A:A)"));
    sheet
        .cells
        .set(1, 6, Cell::formula("COUNTIF(E:E,\"high\")"));
    sheet
        .cells
        .set(2, 6, Cell::formula("VLOOKUP(\"item 4999\",B:D,3,FALSE)"));
    sheet
        .cells
        .set(3, 6, Cell::formula("SUMIFS(A:A,E:E,\"low\")"));
    book
}

fn timed<T>(label: &str, work: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let result = work();
    println!(
        "{label}: {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );
    result
}

#[test]
#[ignore = "timing; run in a release build"]
fn two_hundred_thousand_rows() {
    println!(
        "one cell takes {} bytes in the grid",
        std::mem::size_of::<Cell>() + 4
    );
    let bytes = {
        let mut book = timed(
            "build 200,000 rows x 5 columns (1,000,000 cells, 400,000 formulas)",
            large_workbook,
        );
        let mut engine = timed("parse every formula and index dependencies", || {
            Engine::new(&book)
        });
        let report = timed("full recalculation", || engine.recalculate_all(&mut book));
        assert_eq!(report.calculated, 2 * ROWS as usize + 4);
        timed("write xlsx", || write_xlsx(&book).unwrap())
    };
    println!("xlsx size: {:.1} MB", bytes.len() as f64 / 1_048_576.0);
    let (before_open, _) = memory_megabytes();
    let mut reopened = timed("read xlsx (streaming)", || read_xlsx(&bytes).unwrap());
    drop(bytes);
    let (after_read, _) = memory_megabytes();
    let mut engine = timed("engine for the reopened file", || Engine::new(&reopened));
    timed("full recalculation of the reopened file", || {
        engine.recalculate_all(&mut reopened)
    });
    let (after_calc, peak) = memory_megabytes();
    println!("memory: {before_open:.0} MB before opening, {after_read:.0} MB with the workbook open, {after_calc:.0} MB with the engine; peak {peak:.0} MB");
    reopened.set_value(0, 1000, 0, Value::Number(999.0));
    let report = timed("incremental recalculation after one edit", || {
        engine.cells_changed(&mut reopened, &[(0, 1000, 0)])
    });
    println!("  formulas recalculated: {}", report.calculated);
    assert!(
        report.calculated < 10,
        "only the row's formulas and the column totals"
    );
    reopened.set_value(0, 5, 1, Value::text("changed"));
    let report = timed(
        "incremental recalculation after a text edit (no formula reads it but the lookup)",
        || engine.cells_changed(&mut reopened, &[(0, 5, 1)]),
    );
    println!("  formulas recalculated: {}", report.calculated);
    let viewport = Range::parse("A150000:J150050").unwrap();
    let cells = timed("viewport of 50 rows x 10 columns", || {
        cells_in(&reopened, 0, viewport, &Locale::default())
    });
    assert_eq!(cells.len(), 51 * 5);
    let cache = timed("selection statistics cache", || {
        StatsCache::new(&reopened.sheets[0])
    });
    let stats = timed("statistics of A1:A200000", || {
        cache.stats(&reopened.sheets[0], &[Range::parse("A1:A200000").unwrap()])
    });
    assert_eq!(stats.numeric_count, u64::from(ROWS));
    timed("statistics of A1000:A2000", || {
        cache.stats(&reopened.sheets[0], &[Range::parse("A1000:A2000").unwrap()])
    });
}

/// one whole-column rule of each costly kind over column A of the large
/// workbook, timed for one block of the grid (64 rows by 32 columns)
#[test]
#[ignore = "timing; run in a release build"]
fn conditional_formats_over_a_whole_column() {
    let mut book = large_workbook();
    // the pass mark the "less than" rule compares with
    book.sheets[0]
        .cells
        .set(0, 7, Cell::value(Value::Number(500.0)));
    let red = book
        .styles
        .add_differential(Some("9C0006"), Some("FFC7CE"), false);
    let rule = |kind: &str, details: Option<&str>, attributes: &[(&str, &str)]| ConditionalRule {
        kind: kind.into(),
        format: Some(red),
        priority: 1,
        details_xml: details.map(str::to_string),
        attributes: attributes
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        operator: (kind == "cellIs").then(|| "lessThan".into()),
        formulas: match kind {
            "cellIs" => vec!["$H$1".into()],
            "expression" => vec!["A1<AVERAGE($A$1:$A$1000)".into()],
            _ => Vec::new(),
        },
        ..ConditionalRule::default()
    };
    // rows 150,961 to 151,024 hold both the column's highest and lowest
    // values, so every rule finds something to mark
    let block = Range::parse("A150961:AF151024").unwrap();
    let cases = [
        ("cellIs less than a pass mark", rule("cellIs", None, &[])),
        ("expression per cell", rule("expression", None, &[])),
        (
            "top 10%",
            rule("top10", None, &[("rank", "10"), ("percent", "1")]),
        ),
        ("above average", rule("aboveAverage", None, &[])),
        ("duplicate values", rule("duplicateValues", None, &[])),
        (
            "3-colour scale with a percentile",
            rule(
                "colorScale",
                Some(
                    r#"<colorScale><cfvo type="min"/><cfvo type="percentile" val="50"/><cfvo type="max"/><color rgb="FFF8696B"/><color rgb="FFFFEB84"/><color rgb="FF63BE7B"/></colorScale>"#,
                ),
                &[],
            ),
        ),
        (
            "data bar",
            rule(
                "dataBar",
                Some(
                    r#"<dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar>"#,
                ),
                &[],
            ),
        ),
    ];
    for (label, rule) in cases {
        book.sheets[0].conditional_formats = vec![ConditionalFormat {
            ranges: vec![Range::parse("A:A").unwrap()],
            rules: vec![rule],
        }];
        let cells = timed(&format!("one 64x32 block, whole-column {label}"), || {
            cells_in(&book, 0, block, &Locale::default())
        });
        assert!(
            cells.iter().any(|cell| cell.conditional.is_some()),
            "{label}"
        );
    }
    // a real file, when one is named (`LETTERS_LARGE_XLSX=...\big200000.xlsx`):
    // a colour scale over its whole first numeric column
    if let Ok(path) = std::env::var("LETTERS_LARGE_XLSX") {
        let bytes = std::fs::read(&path).expect("the named workbook");
        let mut file = timed("read the named workbook", || read_xlsx(&bytes).unwrap());
        let column = (0..16)
            .find(|col| matches!(file.sheets[0].cells.value(1, *col), Value::Number(_)))
            .expect("a numeric column");
        let letter = letters_sheets::address::column_name(column);
        file.sheets[0].conditional_formats = vec![ConditionalFormat {
            ranges: vec![Range::parse(&format!("{letter}:{letter}")).unwrap()],
            rules: vec![rule(
                "colorScale",
                Some(
                    r#"<colorScale><cfvo type="min"/><cfvo type="percentile" val="50"/><cfvo type="max"/><color rgb="FFF8696B"/><color rgb="FFFFEB84"/><color rgb="FF63BE7B"/></colorScale>"#,
                ),
                &[],
            )],
        }];
        let cells = timed(
            &format!("{path}: one block, whole-column {letter} colour scale"),
            || cells_in(&file, 0, block, &Locale::default()),
        );
        assert!(cells.iter().any(|cell| cell.conditional.is_some()));
    }
    let plain = timed("the same block with no rules", || {
        book.sheets[0].conditional_formats.clear();
        cells_in(&book, 0, block, &Locale::default())
    });
    assert!(plain.iter().all(|cell| cell.conditional.is_none()));
}
