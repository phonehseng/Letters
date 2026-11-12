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
