// tests of every open workbook (sessions.rs), through files on disk in
// a throwaway folder, as the app uses them.

use std::fs;
use std::path::{Path, PathBuf};

use letters_sheets::edit::Operation;
use letters_sheets::{Value, Workbook};

use super::*;
use crate::sheets::history::Edit;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("letters-sheets-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sessions(dir: &Path) -> SheetSessions {
    SheetSessions::new(&dir.join("cache"), &dir.join("data"))
}

/// a workbook file with 2 in A1 and =A1*3 in B1
fn two_and_triple(dir: &Path) -> PathBuf {
    let mut book = Workbook::new();
    book.set_value(0, 0, 0, Value::Number(2.0));
    book.set_formula(0, 0, 1, "A1*3");
    let path = dir.join("marks.xlsx");
    fs::write(&path, letters_sheets::xlsx::write_xlsx(&book).unwrap()).unwrap();
    path
}

fn typed(row: u32, col: u32, input: &str) -> Edit {
    Edit::Book(Operation::SetCell {
        sheet: 0,
        row,
        col,
        input: input.into(),
    })
}

fn value(sessions: &SheetSessions, id: u64, row: u32, col: u32) -> Value {
    sessions
        .with(id, |document| Ok(document.book.value(0, row, col)))
        .unwrap()
}

#[test]
fn typing_recalculates_what_depends_on_the_cell_and_undo_takes_it_back() {
    let dir = folder("undo");
    let sessions = sessions(&dir);
    let opened = sessions.open("main", &two_and_triple(&dir), false).unwrap();
    assert_eq!(value(&sessions, opened.id, 0, 1), Value::Number(6.0));
    let outcome = sessions
        .with(opened.id, |d| d.apply(&[typed(0, 0, "5")], "Typing"))
        .unwrap();
    assert!(outcome.dirty);
    assert_eq!(outcome.undo_label.as_deref(), Some("Typing"));
    assert_eq!(value(&sessions, opened.id, 0, 1), Value::Number(15.0));
    sessions.with(opened.id, |d| d.undo()).unwrap();
    assert_eq!(value(&sessions, opened.id, 0, 1), Value::Number(6.0));
    let redone = sessions.with(opened.id, |d| d.redo()).unwrap().unwrap();
    assert_eq!(redone.label.as_deref(), Some("Typing"));
    assert_eq!(value(&sessions, opened.id, 0, 1), Value::Number(15.0));
}

#[test]
fn inserting_a_row_moves_formulas_and_undo_puts_them_back() {
    let dir = folder("insert");
    let sessions = sessions(&dir);
    let opened = sessions.open("main", &two_and_triple(&dir), false).unwrap();
    let insert = Edit::Book(Operation::InsertRows {
        sheet: 0,
        at: 0,
        count: 1,
    });
    let outcome = sessions
        .with(opened.id, |d| d.apply(&[insert], "Insert Row"))
        .unwrap();
    assert!(outcome.structure);
    sessions
        .with(opened.id, |d| d.apply(&[typed(1, 0, "10")], "Typing"))
        .unwrap();
    assert_eq!(value(&sessions, opened.id, 1, 1), Value::Number(30.0));
    sessions.with(opened.id, |d| d.undo()).unwrap();
    sessions.with(opened.id, |d| d.undo()).unwrap();
    assert_eq!(value(&sessions, opened.id, 0, 1), Value::Number(6.0));
}

#[test]
fn an_action_that_fails_halfway_leaves_the_workbook_as_it_was() {
    let dir = folder("half");
    let sessions = sessions(&dir);
    let opened = sessions.open("main", &two_and_triple(&dir), false).unwrap();
    let bad_sheet = Edit::Book(Operation::RenameSheet {
        sheet: 7,
        name: "Data".into(),
    });
    let result = sessions.with(opened.id, |d| {
        d.apply(&[typed(0, 0, "9"), bad_sheet], "Paste")
    });
    assert!(result.is_err());
    assert_eq!(value(&sessions, opened.id, 0, 0), Value::Number(2.0));
}

#[test]
fn saving_writes_the_edit_and_the_file_reads_back() {
    let dir = folder("save");
    let sessions = sessions(&dir);
    let path = two_and_triple(&dir);
    let opened = sessions.open("main", &path, false).unwrap();
    sessions
        .with(opened.id, |d| d.apply(&[typed(2, 0, "hello")], "Typing"))
        .unwrap();
    let saved = sessions.save(opened.id, None, 0).unwrap();
    assert!(!saved.state.dirty);
    let again = sessions.open("main", &path, false).unwrap();
    assert_eq!(value(&sessions, again.id, 2, 0), Value::text("hello"));
    // nothing but the file itself is left in the folder
    let names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(names
        .iter()
        .all(|n| n == "marks.xlsx" || n == "cache" || n == "data"));
}

#[test]
fn save_as_ods_changes_the_format_and_csv_of_two_sheets_stays_unsaved() {
    let dir = folder("formats");
    let sessions = sessions(&dir);
    let opened = sessions.open("main", &two_and_triple(&dir), false).unwrap();
    let saved = sessions
        .save(opened.id, Some(&dir.join("marks.ods")), 0)
        .unwrap();
    assert_eq!(saved.format, SheetFormat::Ods);
    let add = Edit::Book(Operation::AddSheet {
        index: 1,
        name: "Notes".into(),
    });
    sessions
        .with(opened.id, |d| d.apply(&[add], "Add Sheet"))
        .unwrap();
    let csv = sessions.save(opened.id, Some(&dir.join("marks")), 0);
    // a name without an extension keeps the current format (ods here)
    assert!(csv.unwrap().path.ends_with("marks.ods"));
    let losses = sessions
        .save_losses(opened.id, &dir.join("x.csv"))
        .unwrap()
        .unwrap();
    assert!(losses.contains("other sheet"), "{losses}");
    let as_csv = sessions
        .save(opened.id, Some(&dir.join("marks.csv")), 0)
        .unwrap();
    assert!(as_csv.state.dirty);
}

#[test]
fn a_new_workbook_asks_where_to_save_and_a_starter_one_is_calculated() {
    let dir = folder("new");
    let sessions = sessions(&dir);
    let blank = sessions.new_workbook("main", None).unwrap();
    assert!(blank.path.is_none() && !blank.state.dirty);
    assert!(sessions
        .save(blank.id, None, 0)
        .unwrap_err()
        .contains("Choose where"));
    for name in letters_sheets::help::presets::TEMPLATES {
        let starter = sessions.new_workbook("main", Some(name)).unwrap();
        assert!(starter.state.dirty, "{name}");
        let has_error = sessions
            .with(starter.id, |d| {
                Ok(d.book.sheets.iter().any(|s| {
                    s.cells
                        .iter()
                        .any(|(_, _, c)| matches!(c.value, Value::Error(_)))
                }))
            })
            .unwrap();
        assert!(!has_error, "{name} shows an error");
    }
}

#[test]
fn a_recovery_copy_is_offered_and_opens_as_unsaved() {
    let dir = folder("recovery");
    let sessions = sessions(&dir);
    let path = two_and_triple(&dir);
    let opened = sessions.open("main", &path, false).unwrap();
    sessions
        .with(opened.id, |d| d.apply(&[typed(0, 0, "7")], "Typing"))
        .unwrap();
    sessions.autosave(opened.id).unwrap();
    sessions.close(opened.id);
    let next = sessions.open("main", &path, false).unwrap();
    assert!(next.recovery.is_some());
    let recovered = sessions.open("main", &path, true).unwrap();
    assert!(recovered.state.dirty);
    assert_eq!(value(&sessions, recovered.id, 0, 1), Value::Number(21.0));
}

#[test]
fn closing_a_window_closes_its_workbooks_only() {
    let dir = folder("windows");
    let sessions = sessions(&dir);
    let path = two_and_triple(&dir);
    sessions.open("one", &path, false).unwrap();
    sessions.open("two", &path, false).unwrap();
    sessions.close_window("one");
    assert_eq!(sessions.open_count(), 1);
}

/// 200,000 rows of five columns, the last a formula of the row: the size
/// the grid is measured with. ignored: it takes seconds and prints times.
#[test]
#[ignore]
fn a_200000_row_workbook_opens_edits_and_answers_the_grid_in_time() {
    use std::time::Instant;
    let dir = folder("large");
    let sessions = sessions(&dir);
    let mut book = Workbook::new();
    for row in 0..200_000u32 {
        for col in 0..4 {
            book.set_value(0, row, col, Value::Number(f64::from(row * 4 + col)));
        }
        book.set_formula(0, row, 4, &format!("A{0}+B{0}", row + 1));
    }
    let path = dir.join("large.xlsx");
    fs::write(&path, letters_sheets::xlsx::write_xlsx(&book).unwrap()).unwrap();
    let started = Instant::now();
    let opened = sessions.open("main", &path, false).unwrap();
    println!("open: {:?}", started.elapsed());
    let started = Instant::now();
    sessions
        .with(opened.id, |d| d.apply(&[typed(5, 0, "1")], "Typing"))
        .unwrap();
    println!("edit and recalculate: {:?}", started.elapsed());
    let started = Instant::now();
    let range = letters_sheets::Range::from_bounds(100_000, 0, 100_060, 25);
    let shown = sessions
        .with(opened.id, |d| {
            Ok(crate::sheets::look::cells(&d.book, 0, range))
        })
        .unwrap();
    println!(
        "viewport of 61 rows: {:?} ({} cells)",
        started.elapsed(),
        shown.len()
    );
    let started = Instant::now();
    let column = letters_sheets::Range::from_bounds(0, 0, 199_999, 0);
    sessions
        .with(opened.id, |d| Ok(d.stats(0, &[column])))
        .unwrap();
    println!("stats of a whole column: {:?}", started.elapsed());
    let started = Instant::now();
    let insert = Edit::Book(Operation::InsertRows {
        sheet: 0,
        at: 10,
        count: 1,
    });
    sessions
        .with(opened.id, |d| d.apply(&[insert], "Insert Row"))
        .unwrap();
    println!("insert a row (engine rebuilt): {:?}", started.elapsed());
}

#[test]
fn a_fault_inside_the_engine_takes_the_change_back_with_a_sentence() {
    let dir = folder("fault");
    let sessions = sessions(&dir);
    let opened = sessions.open("main", &two_and_triple(&dir), false).unwrap();
    // a function given too few arguments used to be such a fault; it now
    // gives #VALUE! (core/sheets checks the count against the catalogue)
    sessions
        .with(opened.id, |d| {
            d.apply(&[typed(6, 1, "=SUBSTITUTE(A1:A3)")], "Typing")
        })
        .unwrap();
    assert_eq!(
        value(&sessions, opened.id, 6, 1),
        Value::Error(letters_sheets::ErrorValue::Value)
    );
    // so a fault is planted for the next calculation
    crate::sheets::document::PLANTED_FAULT.with(|planted| planted.set(true));
    let error = sessions
        .with(opened.id, |d| d.apply(&[typed(5, 1, "=A1*2")], "Typing"))
        .unwrap_err();
    assert!(error.contains("wasn’t made"), "{error}");
    assert_eq!(value(&sessions, opened.id, 5, 1), Value::Blank);
    let outcome = sessions
        .with(opened.id, |d| d.apply(&[typed(0, 0, "4")], "Typing"))
        .unwrap();
    assert_eq!(outcome.undo_label.as_deref(), Some("Typing"));
    assert_eq!(value(&sessions, opened.id, 0, 1), Value::Number(12.0));
}
