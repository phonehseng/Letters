//! the annotation engine run through an edit session, the way the app runs it.
//!
//! the engine itself is tested in core/annotate. these tests check what
//! the app relies on: each annotation is one undo step, a refused one
//! leaves the working copy untouched, deleting a note takes its replies
//! with it, flattening can still be undone, and everything is in the file
//! after saving.

use std::fs;
use std::path::{Path, PathBuf};

use letters_annotate::{
    add_note, add_reply, add_text_markup, delete, flatten, list, AnnotateError, AnnotationInfo,
    Color, FlattenScope, NoteParams, Point, Rect, ReplyParams, TextMarkupKind, TextMarkupParams,
};

use crate::session::{ApplyError, EditSession};
use crate::test_pdf::{temp_dir, write_pdf};

fn show(path: &Path) -> Result<PathBuf, String> {
    Ok(path.to_path_buf())
}

fn session_for(test_name: &str) -> EditSession {
    let dir = temp_dir(test_name);
    let original = dir.join("essay.pdf");
    write_pdf(&original, 2);
    EditSession::start(&original, &dir.join("sessions")).unwrap()
}

fn highlight(rects: Vec<Rect>) -> TextMarkupParams {
    TextMarkupParams {
        kind: TextMarkupKind::Highlight,
        rects,
        color: Color {
            r: 255,
            g: 214,
            b: 10,
        },
        opacity: 1.0,
        contents: None,
        author: Some("Sam".to_owned()),
    }
}

fn line_of_text() -> Rect {
    Rect {
        x: 72.0,
        y: 100.0,
        width: 200.0,
        height: 20.0,
    }
}

/// runs one annotation edit and hands back what the engine made, the
/// way the shell's commands do.
fn add(
    session: &mut EditSession,
    make: impl FnOnce(&mut lopdf::Document) -> Result<AnnotationInfo, AnnotateError>,
) -> Result<AnnotationInfo, ApplyError<AnnotateError>> {
    let mut made = None;
    session.apply(
        |document| {
            made = Some(make(document)?);
            Ok(())
        },
        show,
    )?;
    Ok(made.expect("the edit ran"))
}

fn annotations_on(session: &EditSession, page: usize) -> Vec<AnnotationInfo> {
    list(&session.read_document().unwrap(), page).unwrap()
}

#[test]
fn a_highlight_is_one_undo_step() {
    let mut session = session_for("annotate-undo");
    let made = add(&mut session, |document| {
        add_text_markup(document, 0, &highlight(vec![line_of_text()]))
    })
    .unwrap();

    let listed = annotations_on(&session, 0);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, made.id);
    assert_eq!(listed[0].author.as_deref(), Some("Sam"));
    assert!(session.is_dirty());

    session.undo(show).unwrap();
    assert!(annotations_on(&session, 0).is_empty());
    assert!(!session.is_dirty());
    session.redo(show).unwrap();
    assert_eq!(annotations_on(&session, 0).len(), 1);
}

#[test]
fn a_refused_annotation_leaves_the_working_copy_alone() {
    let mut session = session_for("annotate-refused");
    let before = fs::read(session.working_copy()).unwrap();
    let revision = session.revision();

    let result = add(&mut session, |document| {
        add_text_markup(document, 0, &highlight(vec![]))
    });

    match result {
        Err(ApplyError::Edit(AnnotateError::EmptyArea)) => {}
        other => panic!("expected the empty area refusal, got {other:?}"),
    }
    assert_eq!(fs::read(session.working_copy()).unwrap(), before);
    assert_eq!(session.revision(), revision);
    assert!(!session.can_undo());
}

#[test]
fn deleting_a_note_takes_its_replies_with_it() {
    let mut session = session_for("annotate-thread");
    let note = add(&mut session, |document| {
        add_note(
            document,
            1,
            &NoteParams {
                position: Point { x: 300.0, y: 300.0 },
                icon: Default::default(),
                color: Color {
                    r: 255,
                    g: 204,
                    b: 0,
                },
                contents: "Check this date".to_owned(),
                author: Some("Sam".to_owned()),
                opacity: 1.0,
                open: false,
            },
        )
    })
    .unwrap();
    let reply = add(&mut session, |document| {
        add_reply(
            document,
            1,
            &note.id,
            &ReplyParams {
                contents: "It is 1848".to_owned(),
                author: Some("Ana".to_owned()),
            },
        )
    })
    .unwrap();
    assert_eq!(reply.reply_to.as_deref(), Some(note.id.as_str()));
    assert_eq!(annotations_on(&session, 1).len(), 2);

    let note_id = note.id.clone();
    session
        .apply(|document| delete(document, 1, &note_id).map(|_| ()), show)
        .unwrap();
    assert!(annotations_on(&session, 1).is_empty());
}

#[test]
fn flattening_can_be_undone() {
    let mut session = session_for("annotate-flatten");
    add(&mut session, |document| {
        add_text_markup(document, 0, &highlight(vec![line_of_text()]))
    })
    .unwrap();

    session
        .apply(
            |document| flatten(document, &FlattenScope::AllPages).map(|_| ()),
            show,
        )
        .unwrap();
    assert!(annotations_on(&session, 0).is_empty());

    session.undo(show).unwrap();
    assert_eq!(annotations_on(&session, 0).len(), 1);
}

#[test]
fn annotations_are_in_the_file_after_saving() {
    let mut session = session_for("annotate-save");
    let made = add(&mut session, |document| {
        add_text_markup(document, 0, &highlight(vec![line_of_text()]))
    })
    .unwrap();
    let copy = session.original_path().with_file_name("essay copy.pdf");
    session.save_as(&copy).unwrap();

    let saved = lopdf::Document::load(&copy).unwrap();
    let listed = list(&saved, 0).unwrap();
    assert_eq!(listed.len(), 1);
    // the id is the annotation's /NM, so it survives saving
    assert_eq!(listed[0].id, made.id);
}
