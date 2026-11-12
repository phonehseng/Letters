//! every tip, shown where it should fire and where it must not.

use super::*;
use crate::edit::apply;

fn book_with(cells: &[(&str, &str)]) -> Workbook {
    let mut book = Workbook::new();
    for (address, input) in cells {
        let at = CellRef::parse(address).unwrap();
        apply(
            &mut book,
            &Operation::SetCell {
                sheet: 0,
                row: at.row,
                col: at.col,
                input: input.to_string(),
            },
        )
        .unwrap();
    }
    crate::calc::recalculate(&mut book);
    book
}

fn ids(book: &Workbook, selection: &str, recent: &[RecentAction]) -> Vec<String> {
    let range = Range::parse(selection).unwrap();
    suggest(book, 0, range, recent, &[])
        .into_iter()
        .map(|s| s.id.split(':').next().unwrap().to_string())
        .collect()
}

fn typed(address: &str) -> Vec<RecentAction> {
    let at = CellRef::parse(address).unwrap();
    vec![RecentAction::Typed {
        row: at.row,
        col: at.col,
    }]
}

/// a header and `rows` rows of data
fn table_book(rows: u32) -> Workbook {
    let mut cells = vec![
        ("A1".to_string(), "Fruit".to_string()),
        ("B1".to_string(), "Sold".to_string()),
    ];
    for row in 0..rows {
        let fruit = ["apple", "pear", "plum"][row as usize % 3];
        cells.push((format!("A{}", row + 2), fruit.to_string()));
        cells.push((format!("B{}", row + 2), (row * 3 % 7).to_string()));
    }
    let refs: Vec<(&str, &str)> = cells
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    book_with(&refs)
}

#[test]
fn add_total_fires_below_numbers_only() {
    let book = book_with(&[
        ("A1", "3"),
        ("A2", "4"),
        ("A3", "5"),
        ("B1", "x"),
        ("B2", "y"),
    ]);
    let found = suggest(&book, 0, Range::parse("A4").unwrap(), &[], &[]);
    assert_eq!(found[0].preview, "=SUM(A1:A3)");
    assert!(
        !ids(&book, "B3", &[]).contains(&"add-total".to_string()),
        "text above"
    );
    assert!(
        !ids(&book, "A3", &[]).contains(&"add-total".to_string()),
        "the cell isn't empty"
    );
    assert!(
        !ids(&book, "C4", &[]).contains(&"add-total".to_string()),
        "nothing above"
    );
}

#[test]
fn fill_down_follows_the_neighbouring_column() {
    let book = book_with(&[("A1", "1"), ("A2", "2"), ("A3", "3"), ("B1", "=A1*2")]);
    let found: Vec<Suggestion> = suggest(&book, 0, Range::parse("B1").unwrap(), &typed("B1"), &[])
        .into_iter()
        .filter(|s| s.id.starts_with("fill-down"))
        .collect();
    assert_eq!(found[0].preview, "2 rows");
    let plain = book_with(&[("A1", "1"), ("A2", "2"), ("B1", "5")]);
    assert!(
        !ids(&plain, "B1", &typed("B1")).contains(&"fill-down".to_string()),
        "a value, not a formula"
    );
}

#[test]
fn flash_fill_needs_a_clear_pattern() {
    let book = book_with(&[
        ("A1", "Jane Doe"),
        ("A2", "John Smith"),
        ("A3", "Ada Lovelace"),
        ("B1", "Jane"),
        ("B2", "John"),
    ]);
    let found: Vec<Suggestion> = suggest(&book, 0, Range::parse("B2").unwrap(), &typed("B2"), &[])
        .into_iter()
        .filter(|s| s.id.starts_with("flash-fill"))
        .collect();
    assert_eq!(found[0].preview, "Ada");
    let random = book_with(&[
        ("A1", "Jane Doe"),
        ("A2", "John Smith"),
        ("A3", "Ada Lovelace"),
        ("B1", "red"),
        ("B2", "blue"),
    ]);
    assert!(!ids(&random, "B2", &typed("B2")).contains(&"flash-fill".to_string()));
}

#[test]
fn header_tips_need_a_header_and_enough_rows() {
    let big = table_book(25);
    let found = ids(&big, "A1", &[]);
    for tip in ["freeze-header", "make-table", "sort", "summarise"] {
        assert!(found.contains(&tip.to_string()), "{tip} in {found:?}");
    }
    let small = table_book(2);
    let found = ids(&small, "A1", &[]);
    assert!(!found.contains(&"freeze-header".to_string()));
    assert!(!found.contains(&"make-table".to_string()));
    let no_header = book_with(&[
        ("A1", "1"),
        ("A2", "2"),
        ("A3", "3"),
        ("A4", "4"),
        ("B1", "5"),
        ("B2", "6"),
        ("B3", "7"),
        ("B4", "8"),
    ]);
    assert!(!ids(&no_header, "A1", &[]).contains(&"make-table".to_string()));
}

#[test]
fn duplicates_and_text_numbers_and_dates() {
    let repeated = book_with(&[
        ("A1", "a"),
        ("B1", "1"),
        ("A2", "a"),
        ("B2", "1"),
        ("A3", "b"),
        ("B3", "2"),
    ]);
    assert!(ids(&repeated, "A1", &[]).contains(&"remove-duplicates".to_string()));
    let distinct = book_with(&[("A1", "a"), ("B1", "1"), ("A2", "b"), ("B2", "1")]);
    assert!(!ids(&distinct, "A1", &[]).contains(&"remove-duplicates".to_string()));
    let texts = book_with(&[
        ("A1", "'12"),
        ("A2", "'7.5"),
        ("A3", "'3/1/2024"),
        ("A4", "'4/1/2024"),
    ]);
    let found = ids(&texts, "A1:A4", &[]);
    assert!(
        found.contains(&"text-to-numbers".to_string())
            && found.contains(&"text-to-dates".to_string())
    );
    let words = book_with(&[("A1", "apple"), ("A2", "'12")]);
    assert!(
        !ids(&words, "A1:A2", &[]).contains(&"text-to-numbers".to_string()),
        "one number is not enough"
    );
}

#[test]
fn formula_tips() {
    let unsorted = book_with(&[
        ("A1", "30"),
        ("A2", "10"),
        ("A3", "20"),
        ("C1", "=VLOOKUP(10,A1:B3,2)"),
    ]);
    let found = suggest(&unsorted, 0, Range::parse("C1").unwrap(), &[], &[]);
    assert!(found
        .iter()
        .any(|s| s.preview == "=VLOOKUP(10,A1:B3,2,FALSE)"));
    let sorted = book_with(&[("A1", "10"), ("A2", "20"), ("C1", "=VLOOKUP(10,A1:B2,2)")]);
    assert!(!ids(&sorted, "C1", &[]).contains(&"exact-lookup".to_string()));
    let nested = book_with(&[(
        "A1",
        "=IF(B1>90,\"A\",IF(B1>80,\"B\",IF(B1>70,\"C\",\"D\")))",
    )]);
    let found = suggest(&nested, 0, Range::parse("A1").unwrap(), &[], &[]);
    assert!(found
        .iter()
        .any(|s| s.preview == "=IFS(B1>90,\"A\",B1>80,\"B\",B1>70,\"C\",TRUE,\"D\")"));
    let two = book_with(&[("A1", "=IF(B1>90,\"A\",IF(B1>80,\"B\",\"C\"))")]);
    assert!(!ids(&two, "A1", &[]).contains(&"use-ifs".to_string()));
    let rates = book_with(&[
        ("A1", "=B1*0.0825"),
        ("A2", "=B2*0.0825"),
        ("A3", "=B3*0.0825"),
    ]);
    assert!(ids(&rates, "A1", &[]).contains(&"name-value".to_string()));
    let common = book_with(&[("A1", "=B1*100"), ("A2", "=B2*100"), ("A3", "=B3*100")]);
    assert!(!ids(&common, "A1", &[]).contains(&"name-value".to_string()));
}

#[test]
fn volatile_whole_columns_only_on_large_sheets() {
    let mut big = book_with(&[("C1", "=SUM(OFFSET(A1,0,0,10,1))+SUM(A:A)")]);
    big.set_value(0, 20_000, 0, Value::Number(1.0));
    assert!(ids(&big, "C1", &[]).contains(&"bounded-ranges".to_string()));
    let small = book_with(&[("C1", "=SUM(OFFSET(A1,0,0,10,1))+SUM(A:A)")]);
    assert!(!ids(&small, "C1", &[]).contains(&"bounded-ranges".to_string()));
}

#[test]
fn dismissed_tips_stay_away() {
    let book = book_with(&[("A1", "3"), ("A2", "4")]);
    let all = suggest(&book, 0, Range::parse("A3").unwrap(), &[], &[]);
    let dismissed: Vec<String> = all.iter().map(|s| s.id.clone()).collect();
    assert!(suggest(&book, 0, Range::parse("A3").unwrap(), &[], &dismissed).is_empty());
}
