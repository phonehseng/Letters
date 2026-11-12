//! tests of every open presentation (sessions.rs): opening, editing with
//! undo, inserting, pictures, saving, recovery and export, through the
//! same calls the shell's commands make.

use super::*;
use crate::slides::pictures::tests::FakeViewer;
use crate::slides::test_decks::{deck_path, scratch_folder};

fn sessions(name: &str) -> SlidesSessions {
    SlidesSessions::new(&scratch_folder(name))
}

fn first_slide(sessions: &SlidesSessions, id: u64) -> String {
    sessions.deck(id).unwrap().slides[0].id.clone()
}

#[test]
fn opening_a_deck_shows_its_slides_and_nothing_to_undo() {
    let sessions = sessions("sessions-open");
    let opened = sessions
        .open("window-1", &deck_path("pp_two_content.pptx"), false)
        .unwrap();
    assert_eq!(opened.name, "pp_two_content.pptx");
    assert_eq!(opened.format, DeckFormat::Pptx);
    assert!(!opened.state.can_undo && !opened.state.dirty);
    assert!(!opened.deck.slides.is_empty());
    assert_eq!(sessions.open_count(), 1);
}

#[test]
fn an_odp_deck_opens_too() {
    let sessions = sessions("sessions-odp");
    let opened = sessions
        .open("window-1", &deck_path("lo_title_bullets.odp"), false)
        .unwrap();
    assert_eq!(opened.format, DeckFormat::Odp);
    assert!(!opened.deck.slides.is_empty());
}

#[test]
fn moving_a_shape_redraws_its_slide_and_undo_puts_it_back() {
    let sessions = sessions("sessions-move");
    let viewer = FakeViewer::default();
    let id = sessions
        .open("window-1", &deck_path("pp_shapes.pptx"), false)
        .unwrap()
        .id;
    let slide_id = first_slide(&sessions, id);
    let before = sessions.slide(id, &slide_id).unwrap();
    let shape = before.objects[0].id;
    let first = sessions
        .pictures(id, std::slice::from_ref(&slide_id), &viewer)
        .unwrap();
    let operation = Operation::Move {
        slide_id: slide_id.clone(),
        object_ids: vec![shape],
        dx: 12_700 * 10,
        dy: 0,
    };
    let edited = sessions.apply(id, &operation, &viewer).unwrap();
    assert!(edited.change.dirty && edited.change.changed_slides.contains(&slide_id));
    let moved = sessions.slide(id, &slide_id).unwrap();
    assert!((moved.objects[0].x - before.objects[0].x - 10.0).abs() < 0.001);
    // every other object stayed exactly where it was
    for (after, was) in moved.objects.iter().zip(&before.objects).skip(1) {
        assert_eq!(
            (after.x, after.y, after.width, after.height),
            (was.x, was.y, was.width, was.height)
        );
    }
    let redrawn = sessions
        .pictures(id, std::slice::from_ref(&slide_id), &viewer)
        .unwrap();
    assert!(redrawn[0].revision > first[0].revision);
    let undone = sessions.undo(id, &viewer).unwrap();
    assert!(!undone.change.dirty);
    assert_eq!(
        sessions.slide(id, &slide_id).unwrap().objects[0].x,
        before.objects[0].x
    );
}

#[test]
fn a_hidden_slide_stays_hidden_after_saving_and_undo_shows_it_again() {
    let folder = scratch_folder("sessions-hide-files");
    let sessions = sessions("sessions-hide");
    let viewer = FakeViewer::default();
    let id = sessions
        .open("window-1", &deck_path("pp_title_bullets.pptx"), false)
        .unwrap()
        .id;
    let slide_id = first_slide(&sessions, id);
    let edited = sessions.set_hidden(id, &slide_id, true, &viewer).unwrap();
    assert!(edited.deck.slides[0].hidden);
    sessions
        .save(id, Some(&folder.join("hidden.pptx")))
        .unwrap();
    let reopened = sessions
        .open("window-2", &folder.join("hidden.pptx"), false)
        .unwrap();
    assert!(reopened.deck.slides[0].hidden);
    let undone = sessions.undo(id, &viewer).unwrap();
    assert!(!undone.deck.slides[0].hidden);
}

/// the operations exactly as the editor sends them (lib/slides/operations.ts):
/// camelCase names, snake_case fields
#[test]
fn the_editors_operations_are_understood() {
    let shapes = [
        r#"{"op":"addSlide","index":1,"layout_id":"x"}"#,
        r#"{"op":"move","slide_id":"s","object_ids":[2],"dx":12700,"dy":0}"#,
        r#"{"op":"resize","slide_id":"s","object_id":2,"x":0,"y":0,"width":1,"height":1}"#,
        r#"{"op":"rotate","slide_id":"s","object_id":2,"rotation":900000}"#,
        r#"{"op":"setText","slide_id":"s","object_id":2,"paragraphs":["a","b"]}"#,
        r#"{"op":"setFill","slide_id":"s","object_id":2,"fill":{"type":"solid","color":{"kind":{"type":"scheme","slot":"accent2"}}}}"#,
        r#"{"op":"reorder","slide_id":"s","object_id":2,"to":"front"}"#,
        r#"{"op":"setCellText","slide_id":"s","object_id":2,"row":0,"column":1,"text":"x"}"#,
        r#"{"op":"insertRow","slide_id":"s","object_id":2,"at":1}"#,
        r#"{"op":"setThemeColor","theme_id":"t","slot":"accent1","color":{"kind":{"type":"rgb","hex":"FF0000"}}}"#,
        r#"{"op":"setThemeFonts","theme_id":"t","major":"Georgia","minor":"Verdana"}"#,
        r#"{"op":"setNotes","slide_id":"s","text":"say this"}"#,
        r#"{"op":"findReplace","find":"a","replace":"b","match_case":false}"#,
        r#"{"op":"group","slide_id":"s","object_ids":[2,3]}"#,
        r#"{"op":"ungroup","slide_id":"s","group_id":4}"#,
    ];
    for shape in shapes {
        assert!(serde_json::from_str::<Operation>(shape).is_ok(), "{shape}");
    }
    let layout = serde_json::from_str::<PdfLayout>(r#"{"type":"handouts","perPage":6}"#).unwrap();
    assert_eq!(layout, PdfLayout::Handouts(6));
}

#[test]
fn a_duplicate_sits_a_quarter_inch_off_its_original() {
    let sessions = sessions("sessions-duplicate");
    let viewer = FakeViewer::default();
    let id = sessions
        .open("window-1", &deck_path("pp_title_bullets.pptx"), false)
        .unwrap()
        .id;
    let slide_id = first_slide(&sessions, id);
    let before = sessions.slide(id, &slide_id).unwrap();
    let original = &before.objects[0];
    let edited = sessions
        .duplicate(id, &slide_id, &[original.id], &viewer)
        .unwrap();
    let after = sessions.slide(id, &slide_id).unwrap();
    assert_eq!(after.objects.len(), before.objects.len() + 1);
    let copy = after
        .objects
        .iter()
        .find(|o| Some(o.id) == edited.added)
        .unwrap();
    assert!((copy.x - original.x - 18.0).abs() < 0.01 && (copy.y - original.y - 18.0).abs() < 0.01);
    assert!(sessions.duplicate(id, &slide_id, &[], &viewer).is_err());
}

#[test]
fn the_typing_field_gets_the_face_the_slide_is_drawn_with() {
    let sessions = sessions("sessions-face");
    let id = sessions
        .open("window-1", &deck_path("pp_title_bullets.pptx"), false)
        .unwrap()
        .id;
    // a computer with no fonts at all (a bare build machine) has nothing to hand over
    let Some(bytes) = sessions.face_bytes(id, "Arial", false, false).unwrap() else {
        return;
    };
    // a font file starts with one of these tags
    assert!(
        bytes.starts_with(&[0, 1, 0, 0])
            || bytes.starts_with(b"OTTO")
            || bytes.starts_with(b"true")
    );
}

#[test]
fn inserting_returns_the_new_object_to_select() {
    let sessions = sessions("sessions-insert");
    let viewer = FakeViewer::default();
    let id = sessions
        .open("window-1", &deck_path("pp_title_bullets.pptx"), false)
        .unwrap()
        .id;
    let slide_id = first_slide(&sessions, id);
    let at = PointBox {
        x: 50.0,
        y: 50.0,
        width: 100.0,
        height: 60.0,
    };
    let edited = sessions
        .insert(
            id,
            &slide_id,
            &NewObject::Shape {
                preset: "rect".into(),
            },
            at,
            &viewer,
        )
        .unwrap();
    let added = edited.added.unwrap();
    let view = sessions.slide(id, &slide_id).unwrap();
    let object = view.objects.iter().find(|o| o.id == added).unwrap();
    assert_eq!(object.kind, "shape");
    assert!((object.width - 100.0).abs() < 0.001);
}

#[test]
fn find_and_replace_goes_through_undo() {
    let sessions = sessions("sessions-find");
    let viewer = FakeViewer::default();
    let id = sessions
        .open("window-1", &deck_path("pp_title_bullets.pptx"), false)
        .unwrap()
        .id;
    let title = sessions.deck(id).unwrap().slides[0].title.clone();
    let word = title.split_whitespace().next().unwrap().to_string();
    let found = sessions.find(id, &word, true).unwrap();
    assert!(!found.is_empty());
    sessions
        .replace(id, &found[0], &word, "Swapped", true, &viewer)
        .unwrap();
    assert!(sessions.deck(id).unwrap().slides[0]
        .title
        .starts_with("Swapped"));
    sessions.undo(id, &viewer).unwrap();
    assert_eq!(sessions.deck(id).unwrap().slides[0].title, title);
}

#[test]
fn saving_then_reopening_keeps_the_edit_and_drops_the_recovery_copy() {
    let folder = scratch_folder("sessions-save-files");
    let copy = folder.join("talk.pptx");
    std::fs::copy(deck_path("pp_title_bullets.pptx"), &copy).unwrap();
    let sessions = sessions("sessions-save");
    let viewer = FakeViewer::default();
    let id = sessions.open("window-1", &copy, false).unwrap().id;
    sessions
        .apply(id, &Operation::DeleteSlide { index: 1 }, &viewer)
        .unwrap();
    sessions.autosave(id).unwrap();
    let again = sessions.open("window-2", &copy, false).unwrap();
    assert!(
        again.recovery.is_some(),
        "an unsaved change leaves a recovery copy"
    );
    let saved = sessions.save(id, None).unwrap();
    assert!(!saved.state.dirty);
    let reopened = sessions.open("window-3", &copy, false).unwrap();
    assert_eq!(reopened.deck.slides.len(), 1);
    assert!(reopened.recovery.is_none());
}

#[test]
fn a_recovered_deck_opens_with_unsaved_changes() {
    let folder = scratch_folder("sessions-recover-files");
    let copy = folder.join("talk.pptx");
    std::fs::copy(deck_path("pp_title_bullets.pptx"), &copy).unwrap();
    let sessions = sessions("sessions-recover");
    let viewer = FakeViewer::default();
    let id = sessions.open("window-1", &copy, false).unwrap().id;
    sessions
        .apply(id, &Operation::DeleteSlide { index: 0 }, &viewer)
        .unwrap();
    sessions.autosave(id).unwrap();
    let recovered = sessions.open("window-2", &copy, true).unwrap();
    assert!(recovered.state.dirty);
    assert_eq!(recovered.deck.slides.len(), 1);
}

#[test]
fn every_pdf_layout_exports() {
    let folder = scratch_folder("sessions-export-files");
    let sessions = sessions("sessions-export");
    let id = sessions
        .open("window-1", &deck_path("pp_autofit_notes.pptx"), false)
        .unwrap()
        .id;
    for (name, layout) in [
        ("slides.pdf", PdfLayout::Slides),
        ("notes.pdf", PdfLayout::NotesPages),
        ("handouts.pdf", PdfLayout::Handouts(6)),
    ] {
        sessions.export_pdf(id, layout, &folder.join(name)).unwrap();
        let bytes = std::fs::read(folder.join(name)).unwrap();
        assert!(bytes.starts_with(b"%PDF"), "{name}");
    }
    assert!(sessions
        .export_pdf(id, PdfLayout::Handouts(5), &folder.join("five.pdf"))
        .is_err());
}

#[test]
fn closing_a_window_closes_its_decks_and_their_pictures() {
    let sessions = sessions("sessions-close");
    let viewer = FakeViewer::default();
    let first = sessions
        .open("window-1", &deck_path("pp_shapes.pptx"), false)
        .unwrap()
        .id;
    sessions
        .open("window-2", &deck_path("pp_shapes.pptx"), false)
        .unwrap();
    let slide_id = first_slide(&sessions, first);
    sessions.pictures(first, &[slide_id], &viewer).unwrap();
    sessions.close_window("window-1", &viewer);
    assert_eq!(sessions.open_count(), 1);
    assert_eq!(viewer.closed.borrow().len(), 1);
    assert!(sessions.deck(first).is_err());
}
