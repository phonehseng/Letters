//! the page tools run through an edit session, the way the app runs them.
//!
//! the tools themselves are tested in core/pages. these tests check what
//! the app relies on: a tool's refusal leaves the working copy untouched,
//! a change can be undone, and the turns the old reference edit promised
//! (inherited rotation, turning left from upright) still hold.

use std::fs;
use std::path::{Path, PathBuf};

use letters_pages::{delete, rotate, DeleteParams, PagesError, RotateParams};

use crate::session::{ApplyError, EditSession};
use crate::test_pdf::{page_rotation, temp_dir, write_pdf, write_pdf_with_inherited_rotation};

fn show(path: &Path) -> Result<PathBuf, String> {
    Ok(path.to_path_buf())
}

fn session_for(test_name: &str, original: &Path) -> EditSession {
    EditSession::start(original, &temp_dir(test_name).join("sessions")).unwrap()
}

fn turn(
    pages: Vec<u32>,
    quarter_turns: i32,
) -> impl FnOnce(&mut lopdf::Document) -> Result<(), PagesError> {
    move |document| {
        rotate(
            document,
            &RotateParams {
                pages,
                quarter_turns,
            },
        )
    }
}

#[test]
fn turning_left_from_upright_gives_270() {
    let original = temp_dir("tools-left").join("one.pdf");
    write_pdf(&original, 1);
    let mut session = session_for("tools-left-session", &original);

    session.apply(turn(vec![0], -1), show).unwrap();

    assert_eq!(page_rotation(session.working_copy(), 1), 270);
}

#[test]
fn turning_adds_to_a_rotation_inherited_from_the_page_tree() {
    let original = temp_dir("tools-inherited").join("two.pdf");
    write_pdf_with_inherited_rotation(&original, 2, Some(90));
    let mut session = session_for("tools-inherited-session", &original);

    session.apply(turn(vec![0], 1), show).unwrap();

    assert_eq!(page_rotation(session.working_copy(), 1), 180);
    assert_eq!(page_rotation(session.working_copy(), 2), 90);
}

#[test]
fn a_missing_page_refuses_the_whole_turn_and_leaves_the_copy_alone() {
    let original = temp_dir("tools-missing").join("two.pdf");
    write_pdf(&original, 2);
    let mut session = session_for("tools-missing-session", &original);
    let before = fs::read(session.working_copy()).unwrap();

    let result = session.apply(turn(vec![0, 5], 1), show);

    assert_eq!(result, Err(ApplyError::Edit(PagesError::NoSuchPage(6))));
    assert_eq!(fs::read(session.working_copy()).unwrap(), before);
    assert!(!session.can_undo());
}

#[test]
fn deleting_every_page_is_refused_with_a_plain_sentence() {
    let original = temp_dir("tools-delete-all").join("two.pdf");
    write_pdf(&original, 2);
    let mut session = session_for("tools-delete-all-session", &original);

    let result = session.apply(
        |document| delete(document, &DeleteParams { pages: vec![0, 1] }),
        show,
    );

    let message = result.unwrap_err().to_string();
    assert_eq!(
        message,
        "A PDF needs at least one page, so not every page can be deleted."
    );
    assert!(!session.is_dirty());
}

#[test]
fn a_deleted_page_comes_back_with_undo() {
    let original = temp_dir("tools-delete-undo").join("three.pdf");
    write_pdf(&original, 3);
    let mut session = session_for("tools-delete-undo-session", &original);
    let count = |path: &Path| lopdf::Document::load(path).unwrap().get_pages().len();

    session
        .apply(
            |document| delete(document, &DeleteParams { pages: vec![1] }),
            show,
        )
        .unwrap();
    assert_eq!(count(session.working_copy()), 2);

    session.undo(show).unwrap();
    assert_eq!(count(session.working_copy()), 3);
}
