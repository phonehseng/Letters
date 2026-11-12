//! reloading: an open document is pointed at a new file under the same id.
//! this is what every edit relies on to show its result.

mod support;

use std::sync::Arc;

use letters_pdf::{DocumentId, PdfError, RenderWorker, TileKey};
use support::{
    pdfium_dir, temp_dir, write_rotated_text_pdf, write_test_pdf, Image, TextLine, RED, WHITE,
};

const MEGABYTE: usize = 1024 * 1024;

fn start_worker() -> RenderWorker {
    RenderWorker::start(&pdfium_dir(), MEGABYTE).unwrap()
}

fn first_tile(document: DocumentId) -> TileKey {
    TileKey {
        document,
        page: 0,
        scale_milli: 1000,
        column: 0,
        row: 0,
    }
}

/// three pages of text turned a quarter turn, so they are wider than tall
/// and have no red square: easy to tell apart from `write_test_pdf`.
fn write_turned_text_pdf(path: &std::path::Path) {
    let page = || vec![TextLine::new(300.0, 400.0, 12.0, "turned")];
    write_rotated_text_pdf(path, &[page(), page(), page()], &[], 90);
}

#[test]
fn reload_shows_the_new_file_under_the_same_id() {
    let dir = temp_dir("reload-new-content");
    let before = dir.join("before.pdf");
    let after = dir.join("after.pdf");
    write_test_pdf(&before, 1);
    write_turned_text_pdf(&after);
    let worker = start_worker();
    let opened = worker.open(&before, None).unwrap();
    let old_tile = Image::from_png(&worker.tile(first_tile(opened.id)).unwrap());
    assert_eq!(old_tile.pixel(100, 100), RED);

    let reloaded = worker.reload(opened.id, &after).unwrap();

    assert_eq!(reloaded.id, opened.id);
    assert_eq!(reloaded.page_sizes.len(), 3);
    assert_eq!(reloaded.page_sizes[0].width_pt, 792.0);
    assert_eq!(reloaded.page_sizes[0].height_pt, 612.0);
    let new_tile = Image::from_png(&worker.tile(first_tile(opened.id)).unwrap());
    assert_eq!(new_tile.pixel(100, 100), WHITE);
}

#[test]
fn reload_drops_the_old_tiles_of_that_document_only() {
    let dir = temp_dir("reload-purge");
    let path = dir.join("one.pdf");
    let other = dir.join("other.pdf");
    let turned = dir.join("turned.pdf");
    write_test_pdf(&path, 1);
    write_test_pdf(&other, 1);
    write_turned_text_pdf(&turned);
    let worker = start_worker();
    let edited = worker.open(&path, None).unwrap();
    let untouched = worker.open(&other, None).unwrap();
    let old_edited_tile = worker.tile(first_tile(edited.id)).unwrap();
    let untouched_tile = worker.tile(first_tile(untouched.id)).unwrap();
    assert_eq!(worker.stats().unwrap().cached_tiles, 2);

    worker.reload(edited.id, &turned).unwrap();

    let stats = worker.stats().unwrap();
    assert_eq!(
        stats.cached_tiles, 1,
        "only the other document's tile is left"
    );
    assert_eq!(stats.open_documents, 2);
    // the other document's tile is still the very same cached one
    let again = worker.tile(first_tile(untouched.id)).unwrap();
    assert!(Arc::ptr_eq(&untouched_tile, &again));
    // and the reloaded document's tile is drawn afresh
    let fresh = worker.tile(first_tile(edited.id)).unwrap();
    assert!(!Arc::ptr_eq(&old_edited_tile, &fresh));
}

#[test]
fn reload_of_an_unknown_document_is_an_error() {
    let dir = temp_dir("reload-unknown");
    let path = dir.join("one.pdf");
    write_test_pdf(&path, 1);
    let worker = start_worker();

    assert_eq!(
        worker.reload(DocumentId(999), &path),
        Err(PdfError::UnknownDocument)
    );
}

#[test]
fn a_failed_reload_keeps_the_old_file_open() {
    let dir = temp_dir("reload-failed");
    let path = dir.join("one.pdf");
    write_test_pdf(&path, 2);
    let worker = start_worker();
    let opened = worker.open(&path, None).unwrap();

    let result = worker.reload(opened.id, &dir.join("missing.pdf"));

    assert!(matches!(result, Err(PdfError::CannotOpen(_))));
    let tile = Image::from_png(&worker.tile(first_tile(opened.id)).unwrap());
    assert_eq!(tile.pixel(100, 100), RED);
}

#[test]
fn closing_waits_until_the_file_can_be_deleted() {
    let dir = temp_dir("close-then-delete");
    let path = dir.join("one.pdf");
    write_test_pdf(&path, 1);
    let worker = start_worker();
    let opened = worker.open(&path, None).unwrap();
    worker.tile(first_tile(opened.id)).unwrap();

    worker.close(opened.id);

    std::fs::remove_file(&path).unwrap();
    assert!(!path.exists());
}
