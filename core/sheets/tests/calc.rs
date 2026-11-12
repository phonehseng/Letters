//! the calculation engine on whole workbooks: formulas reading formulas,
//! incremental recalculation, spills, circular references, functions
//! letters lacks, names, tables and cross-sheet references.

use letters_sheets::address::{CellRef, Range};
use letters_sheets::calc::{recalculate, Engine};
use letters_sheets::model::*;

fn book_with(cells: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    for (address, input) in cells {
        let at = CellRef::parse(address).unwrap();
        if let Some(formula) = input.strip_prefix('=') {
            book.set_formula(0, at.row, at.col, formula);
        } else if let Ok(n) = input.parse::<f64>() {
            book.set_value(0, at.row, at.col, Value::Number(n));
        } else {
            book.set_value(0, at.row, at.col, Value::text(*input));
        }
    }
    book
}

fn number(book: &Workbook, address: &str) -> f64 {
    match book.value_at(address) {
        Value::Number(n) => n,
        other => panic!("{address} is {other:?}"),
    }
}

#[test]
fn formulas_are_calculated_after_the_cells_they_read() {
    // written bottom-up so that the order of the sheet is wrong on purpose
    let mut book = book_with(&[
        ("A4", "=A3*2"),
        ("A3", "=A2+A1"),
        ("A2", "=A1*10"),
        ("A1", "1"),
    ]);
    recalculate(&mut book);
    assert_eq!(number(&book, "A2"), 10.0);
    assert_eq!(number(&book, "A3"), 11.0);
    assert_eq!(number(&book, "A4"), 22.0);
}

#[test]
fn an_edit_recalculates_only_what_depends_on_it() {
    let mut book = book_with(&[
        ("A1", "1"),
        ("B1", "=A1+1"),
        ("C1", "=B1*2"),
        ("A2", "5"),
        ("B2", "=A2*3"),
    ]);
    let mut engine = Engine::new(&book);
    engine.recalculate_all(&mut book);
    book.set_value(0, 0, 0, Value::Number(10.0));
    let report = engine.cells_changed(&mut book, &[(0, 0, 0)]);
    assert_eq!(report.calculated, 2, "B1 and C1, not B2");
    assert_eq!(number(&book, "C1"), 22.0);
    book.set_formula(0, 1, 1, "A2*4");
    engine.cells_changed(&mut book, &[(0, 1, 1)]);
    assert_eq!(number(&book, "B2"), 20.0);
}

#[test]
fn long_chains_do_not_overflow_the_stack() {
    let mut book = Workbook::new();
    book.set_value(0, 0, 0, Value::Number(1.0));
    for row in 1..50_000 {
        let above = CellRef::new(row - 1, 0).to_a1();
        book.set_formula(0, row, 0, &format!("{above}+1"));
    }
    recalculate(&mut book);
    assert_eq!(number(&book, "A50000"), 50_000.0);
}

#[test]
fn circular_references_are_reported_and_show_zero() {
    let mut book = book_with(&[("A1", "=B1+1"), ("B1", "=A1+1"), ("C1", "5")]);
    let report = recalculate(&mut book);
    assert!(!report.circular.is_empty());
    assert_eq!(number(&book, "C1"), 5.0);
}

#[test]
fn unknown_functions_keep_the_stored_value_and_are_reported() {
    let mut book = Workbook::new();
    // as a file stores it: an older-style formula with its last value
    book.sheets[0].cells.set(
        0,
        0,
        Cell {
            formula: Some(Formula::legacy("SOMEFUTUREFUNCTION(1)")),
            value: Value::Number(42.0),
            ..Cell::default()
        },
    );
    let report = recalculate(&mut book);
    assert_eq!(number(&book, "A1"), 42.0);
    assert!(book.sheets[0].cells.get(0, 0).unwrap().not_recalculated);
    assert_eq!(
        report.not_recalculated[0].functions,
        vec!["SOMEFUTUREFUNCTION".to_string()]
    );
}

#[test]
fn dynamic_arrays_spill_and_block() {
    let mut book = book_with(&[
        ("A1", "=SEQUENCE(3)"),
        ("C1", "=SEQUENCE(2,2)*10"),
        ("C2", "x"),
    ]);
    let mut engine = Engine::new(&book);
    engine.recalculate_all(&mut book);
    assert_eq!(number(&book, "A3"), 3.0);
    assert_eq!(book.value_at("C1"), Value::Error(ErrorValue::Spill));
    // clearing the blocking cell lets the formula spill
    book.sheets[0].cells.remove(1, 2);
    engine.cells_changed(&mut book, &[(0, 1, 2)]);
    assert_eq!(number(&book, "D2"), 40.0);
    // a smaller result clears the cells it no longer fills
    book.set_formula(0, 0, 0, "SEQUENCE(1)");
    engine.cells_changed(&mut book, &[(0, 0, 0)]);
    assert_eq!(book.value_at("A3"), Value::Blank);
    let mut sum = book_with(&[("A1", "=SEQUENCE(4)"), ("B1", "=SUM(A1#)")]);
    recalculate(&mut sum);
    assert_eq!(number(&sum, "B1"), 10.0);
}

#[test]
fn names_tables_and_other_sheets() {
    let mut book = book_with(&[
        ("A1", "Item"),
        ("B1", "Price"),
        ("A2", "Pen"),
        ("B2", "2"),
        ("A3", "Ink"),
        ("B3", "5"),
    ]);
    book.sheets[0].tables.push(Table {
        id: 1,
        name: "Shop".into(),
        range: Range::parse("A1:B3").unwrap(),
        header_row: true,
        columns: vec![
            TableColumn {
                name: "Item".into(),
                ..TableColumn::default()
            },
            TableColumn {
                name: "Price".into(),
                ..TableColumn::default()
            },
        ],
        ..Table::default()
    });
    book.defined_names.push(DefinedName {
        name: "Tax".into(),
        formula: "0.25".into(),
        ..DefinedName::default()
    });
    book.sheets.push(Sheet::new("Summary"));
    book.set_formula(1, 0, 0, "SUM(Shop[Price])*(1+Tax)");
    book.set_formula(1, 1, 0, "VLOOKUP(\"ink\",Sheet1!A2:B3,2,FALSE)");
    book.set_formula(1, 2, 0, "COUNTIF(Sheet1!B:B,\">2\")");
    recalculate(&mut book);
    assert_eq!(number(&book, "Summary!A1"), 8.75);
    assert_eq!(number(&book, "Summary!A2"), 5.0);
    assert_eq!(number(&book, "Summary!A3"), 1.0);
}

#[test]
fn excel_snaps_rounding_noise_only_in_the_last_step() {
    let mut book = book_with(&[
        ("A1", "=0.5-0.4-0.1"),
        ("A2", "=1*(0.5-0.4-0.1)"),
        ("A3", "=0.1+0.2=0.3"),
    ]);
    recalculate(&mut book);
    assert_eq!(number(&book, "A1"), 0.0);
    assert!(number(&book, "A2") != 0.0);
    assert_eq!(book.value_at("A3"), Value::Bool(true));
}

#[test]
fn legacy_formulas_take_the_cell_in_line() {
    let mut book = book_with(&[("A1", "1"), ("A2", "2"), ("A3", "3")]);
    book.sheets[0].cells.set(
        1,
        1,
        Cell {
            formula: Some(Formula::legacy("A1:A3*10")),
            ..Cell::default()
        },
    );
    recalculate(&mut book);
    assert_eq!(number(&book, "B2"), 20.0);
    assert_eq!(
        book.value_at("B3"),
        Value::Blank,
        "no spill from a legacy formula"
    );
}
