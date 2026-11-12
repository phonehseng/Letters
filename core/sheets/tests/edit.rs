//! editing operations: each changes the workbook as excel would, and
//! applying its inverse gives back exactly the workbook before.

use letters_sheets::address::{CellRef, Range};
use letters_sheets::calc::recalculate;
use letters_sheets::edit::{apply, ClearKind, FindOptions, Operation, PasteKind, SortKey};
use letters_sheets::model::*;

fn range(text: &str) -> Range {
    Range::parse(text).unwrap()
}

fn type_into(book: &mut Workbook, sheet: usize, cells: &[(&str, &str)]) {
    for (address, input) in cells {
        let at = CellRef::parse(address).unwrap();
        apply(
            book,
            &Operation::SetCell {
                sheet,
                row: at.row,
                col: at.col,
                input: input.to_string(),
            },
        )
        .unwrap();
    }
}

fn formula(book: &Workbook, address: &str) -> String {
    let (sheet, cell) = match address.split_once('!') {
        Some((name, cell)) => (book.sheet_index(name).unwrap(), cell),
        None => (0, address),
    };
    let at = CellRef::parse(cell).unwrap();
    book.sheets[sheet]
        .cells
        .get(at.row, at.col)
        .and_then(|c| c.formula.clone())
        .map(|f| f.text)
        .unwrap_or_default()
}

/// applies an operation, checks its undo restores the workbook exactly,
/// and applies it again for the test to look at
fn apply_and_check_undo(book: &mut Workbook, operation: Operation) {
    let before = book.clone();
    let applied = apply(book, &operation).unwrap();
    let mut undone = book.clone();
    for inverse in &applied.inverse {
        apply(&mut undone, inverse).unwrap();
    }
    assert_eq!(undone, before, "undo of {operation:?}");
}

fn sample() -> Workbook {
    let mut book = Workbook::new();
    book.sheets.push(Sheet::new("Other"));
    type_into(
        &mut book,
        0,
        &[
            ("A1", "1"),
            ("A2", "2"),
            ("A3", "3"),
            ("A4", "=SUM(A1:A3)"),
            ("B2", "=A2*2"),
            ("C1", "=$A$3"),
        ],
    );
    type_into(
        &mut book,
        1,
        &[("A1", "=Sheet1!A3+1"), ("A2", "=SUM(Sheet1!A1:A4)")],
    );
    book.sheets[0].merged.push(range("D2:E3"));
    book
}

#[test]
fn inserting_rows_moves_cells_and_every_reference() {
    let mut book = sample();
    apply_and_check_undo(
        &mut book,
        Operation::InsertRows {
            sheet: 0,
            at: 1,
            count: 2,
        },
    );
    assert_eq!(book.value_at("A4"), Value::Number(2.0));
    assert_eq!(formula(&book, "A6"), "SUM(A1:A5)");
    assert_eq!(formula(&book, "B4"), "A4*2");
    assert_eq!(formula(&book, "C1"), "$A$5");
    assert_eq!(formula(&book, "Other!A1"), "Sheet1!A5+1");
    assert_eq!(book.sheets[0].merged, vec![range("D4:E5")]);
}

#[test]
fn deleting_rows_gives_ref_errors_for_deleted_cells() {
    let mut book = sample();
    apply_and_check_undo(
        &mut book,
        Operation::DeleteRows {
            sheet: 0,
            at: 2,
            count: 1,
        },
    );
    assert_eq!(formula(&book, "C1"), "#REF!");
    assert_eq!(formula(&book, "A3"), "SUM(A1:A2)");
    assert_eq!(formula(&book, "Other!A1"), "#REF!+1");
    recalculate(&mut book);
    assert_eq!(book.value_at("A3"), Value::Number(3.0));
}

#[test]
fn columns_insert_and_delete() {
    let mut book = sample();
    apply_and_check_undo(
        &mut book,
        Operation::InsertColumns {
            sheet: 0,
            at: 0,
            count: 1,
        },
    );
    assert_eq!(formula(&book, "B4"), "SUM(B1:B3)");
    assert_eq!(formula(&book, "D1"), "$B$3");
    apply_and_check_undo(
        &mut book,
        Operation::DeleteColumns {
            sheet: 0,
            at: 0,
            count: 1,
        },
    );
    assert_eq!(formula(&book, "A4"), "SUM(A1:A3)");
}

#[test]
fn copy_shifts_relative_references_and_move_is_followed() {
    let mut book = sample();
    apply_and_check_undo(
        &mut book,
        Operation::Copy {
            sheet: 0,
            from: range("B2"),
            to_sheet: 0,
            to: CellRef::new(4, 3),
            paste: PasteKind::All,
        },
    );
    assert_eq!(formula(&book, "D5"), "C5*2");
    apply_and_check_undo(
        &mut book,
        Operation::Move {
            sheet: 0,
            from: range("A1:A3"),
            to_sheet: 0,
            to: CellRef::new(9, 5),
        },
    );
    assert_eq!(formula(&book, "A4"), "SUM(F10:F12)");
    assert_eq!(formula(&book, "Other!A1"), "Sheet1!F12+1");
    assert_eq!(book.value_at("F10"), Value::Number(1.0));
}

#[test]
fn fill_continues_series_and_copies_formulas() {
    let mut book = Workbook::new();
    type_into(
        &mut book,
        0,
        &[
            ("A1", "1"),
            ("A2", "2"),
            ("B1", "Mon"),
            ("C1", "=A1*10"),
            ("D1", "3/1/2024"),
        ],
    );
    apply_and_check_undo(
        &mut book,
        Operation::Fill {
            sheet: 0,
            source: range("A1:A2"),
            target: range("A1:A5"),
        },
    );
    assert_eq!(book.value_at("A5"), Value::Number(5.0));
    apply_and_check_undo(
        &mut book,
        Operation::Fill {
            sheet: 0,
            source: range("B1:D1"),
            target: range("B1:D3"),
        },
    );
    assert_eq!(book.value_at("B3"), Value::text("Wed"));
    assert_eq!(formula(&book, "C3"), "A3*10");
    assert_eq!(book.value_at("D2"), Value::Number(45353.0));
}

#[test]
fn sort_by_two_keys_with_a_header() {
    let mut book = Workbook::new();
    type_into(
        &mut book,
        0,
        &[
            ("A1", "Name"),
            ("B1", "Score"),
            ("A2", "b"),
            ("B2", "2"),
            ("A3", "a"),
            ("B3", "2"),
            ("A4", "c"),
            ("B4", "9"),
            ("C2", "=B2*2"),
        ],
    );
    let keys = vec![
        SortKey {
            column: 1,
            descending: true,
        },
        SortKey {
            column: 0,
            descending: false,
        },
    ];
    apply_and_check_undo(
        &mut book,
        Operation::Sort {
            sheet: 0,
            range: range("A1:C4"),
            keys,
            has_header: true,
        },
    );
    let names: Vec<Value> = (1..4).map(|r| book.value(0, r, 0)).collect();
    assert_eq!(
        names,
        vec![Value::text("c"), Value::text("a"), Value::text("b")]
    );
    assert_eq!(
        formula(&book, "C4"),
        "B4*2",
        "the formula moved with its row"
    );
}

#[test]
fn sheets_rename_duplicate_and_delete() {
    let mut book = sample();
    apply_and_check_undo(
        &mut book,
        Operation::RenameSheet {
            sheet: 0,
            name: "Budget 2024".into(),
        },
    );
    assert_eq!(formula(&book, "Other!A1"), "'Budget 2024'!A3+1");
    apply_and_check_undo(&mut book, Operation::DuplicateSheet { sheet: 0 });
    assert_eq!(book.sheets[1].name, "Budget 2024 (2)");
    apply_and_check_undo(&mut book, Operation::MoveSheet { from: 2, to: 0 });
    assert_eq!(book.sheets[0].name, "Other");
    apply_and_check_undo(&mut book, Operation::DeleteSheet { sheet: 1 });
    assert_eq!(formula(&book, "Other!A1"), "#REF!+1");
    assert!(apply(
        &mut book,
        &Operation::RenameSheet {
            sheet: 0,
            name: "bad:name".into()
        }
    )
    .is_err());
}

#[test]
fn tables_and_back() {
    let mut book = Workbook::new();
    type_into(
        &mut book,
        0,
        &[
            ("A1", "Item"),
            ("B1", "Price"),
            ("A2", "Pen"),
            ("B2", "2"),
            ("A3", "Ink"),
            ("B3", "5"),
        ],
    );
    apply_and_check_undo(
        &mut book,
        Operation::ToTable {
            sheet: 0,
            range: range("A1:B3"),
            has_header: true,
        },
    );
    type_into(&mut book, 0, &[("D1", "=SUM(Table1[Price])")]);
    recalculate(&mut book);
    assert_eq!(book.value_at("D1"), Value::Number(7.0));
    apply_and_check_undo(
        &mut book,
        Operation::TableToRange {
            sheet: 0,
            table: "Table1".into(),
        },
    );
    assert_eq!(formula(&book, "D1"), "SUM($B$2:$B$3)");
}

#[test]
fn find_replace_duplicates_split_merge_and_clear() {
    let mut book = Workbook::new();
    type_into(
        &mut book,
        0,
        &[
            ("A1", "apple pie"),
            ("A2", "Apple"),
            ("A3", "Apple"),
            ("B1", "=A1&\"!\""),
        ],
    );
    assert_eq!(
        letters_sheets::edit::find(&book, None, "apple", &FindOptions::default()).len(),
        3
    );
    apply_and_check_undo(
        &mut book,
        Operation::Replace {
            sheet: None,
            find: "apple".into(),
            with: "pear".into(),
            options: FindOptions::default(),
        },
    );
    assert_eq!(book.value_at("A2"), Value::text("pear"));
    apply_and_check_undo(
        &mut book,
        Operation::RemoveDuplicates {
            sheet: 0,
            range: range("A1:A3"),
            columns: vec![],
            has_header: false,
        },
    );
    assert_eq!(book.value_at("A3"), Value::Blank);
    type_into(&mut book, 0, &[("D1", "1,2,three")]);
    apply_and_check_undo(
        &mut book,
        Operation::TextToColumns {
            sheet: 0,
            range: range("D1"),
            delimiter: ',',
        },
    );
    assert_eq!(book.value_at("E1"), Value::Number(2.0));
    apply_and_check_undo(
        &mut book,
        Operation::Merge {
            sheet: 0,
            range: range("D1:F1"),
        },
    );
    assert_eq!(book.value_at("E1"), Value::Blank);
    apply_and_check_undo(
        &mut book,
        Operation::Clear {
            sheet: 0,
            range: range("A1:B3"),
            what: ClearKind::All,
        },
    );
    assert_eq!(book.value_at("B1"), Value::Blank);
    apply_and_check_undo(
        &mut book,
        Operation::Freeze {
            sheet: 0,
            rows: 1,
            cols: 0,
        },
    );
}

/// a deterministic random source for the property test
struct Random(u64);
impl Random {
    fn next(&mut self, below: u32) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % u64::from(below)) as u32
    }
}

#[test]
fn random_structural_edits_always_undo_exactly_and_keep_values() {
    let mut random = Random(7);
    for _ in 0..60 {
        let mut book = sample();
        recalculate(&mut book);
        let total_before = book.value_at("Other!A2");
        let at = random.next(8);
        let count = random.next(3) + 1;
        let operation = match random.next(4) {
            0 => Operation::InsertRows {
                sheet: 0,
                at,
                count,
            },
            1 => Operation::InsertColumns {
                sheet: 0,
                at,
                count,
            },
            2 => Operation::DeleteRows {
                sheet: 0,
                at: at + 5,
                count,
            },
            _ => Operation::DeleteColumns {
                sheet: 0,
                at: at + 4,
                count,
            },
        };
        apply_and_check_undo(&mut book, operation);
        recalculate(&mut book);
        // edits away from the data never change what the formulas show
        assert_eq!(book.value_at("Other!A2"), total_before);
    }
}
