//! an open document follows its file when Letters moves it: the edit
//! session keeps working and Save writes to the new place.

use std::fs;

use crate::organise::{move_into, Conflict, Outcome};
use crate::session::EditSession;
use crate::test_pdf::{temp_dir, write_pdf};

#[test]
fn an_open_document_saves_to_where_its_file_was_moved() {
    let folder = temp_dir("organise-follow");
    let sessions = folder.join("sessions");
    let courses = folder.join("Biology 101");
    fs::create_dir_all(&courses).unwrap();
    let original = folder.join("Reading.pdf");
    write_pdf(&original, 2);
    let mut session = EditSession::start(&original, &sessions).unwrap();
    // the engine shows the working copy, so the person's file is free to move
    let Outcome::Done { done } = move_into(&original, &courses, Conflict::Ask).unwrap() else {
        panic!("not moved");
    };
    let (_, to) = done.change().unwrap();
    session.follow_move(std::path::Path::new(to));
    session.save().unwrap();
    assert!(!original.exists(), "save must not bring the old file back");
    assert!(courses.join("Reading.pdf").is_file());
    assert_eq!(
        session.original_path(),
        courses.join("Reading.pdf").as_path()
    );
}
