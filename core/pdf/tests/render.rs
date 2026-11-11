//! end to end tests: a real pdf file, the real pdfium library.

mod support;

use std::fs;
use std::thread;

use letters_pdf::{DocumentId, PdfError, RenderWorker, TileKey};
use support::{pdfium_dir, temp_dir, write_test_pdf, Image, RED, WHITE};

const MEGABYTE: usize = 1024 * 1024;

fn start_worker(cache_budget_bytes: usize) -> RenderWorker {
    RenderWorker::start(&pdfium_dir(), cache_budget_bytes).unwrap()
}

fn tile_key(document: DocumentId, page: u32, scale_milli: u32, column: u32, row: u32) -> TileKey {
    TileKey {
        document,
        page,
        scale_milli,
        column,
        row,
    }
}

#[test]
fn opens_a_long_document_and_reports_every_page_size() {
    let dir = temp_dir("page-sizes");
    let path = dir.join("long.pdf");
    write_test_pdf(&path, 500);

    let info = start_worker(MEGABYTE).open(&path, None).unwrap();

    assert_eq!(info.page_sizes.len(), 500);
    assert_eq!(info.page_sizes[0].width_pt, 612.0);
    assert_eq!(info.page_sizes[499].height_pt, 792.0);
}

#[test]
fn draws_the_page_content_at_the_right_place() {
    let dir = temp_dir("first-tile");
    let path = dir.join("one.pdf");
    write_test_pdf(&path, 1);
    let worker = start_worker(MEGABYTE);
    let info = worker.open(&path, None).unwrap();

    // at 1 pixel per point the red square covers pixels 72..144
    let tile = Image::from_png(&worker.tile(tile_key(info.id, 0, 1000, 0, 0)).unwrap());

    assert_eq!((tile.width, tile.height), (512, 512));
    assert_eq!(tile.pixel(100, 100), RED);
    assert_eq!(tile.pixel(10, 10), WHITE);
    assert_eq!(tile.pixel(200, 100), WHITE);
}

#[test]
fn tiles_away_from_the_corner_show_their_own_part_of_the_page() {
    let dir = temp_dir("offset-tile");
    let path = dir.join("one.pdf");
    write_test_pdf(&path, 1);
    let worker = start_worker(MEGABYTE);
    let info = worker.open(&path, None).unwrap();

    // at 8 pixels per point the red square covers page pixels 576..1152,
    // and tile (1, 1) starts at page pixel 512
    let tile = Image::from_png(&worker.tile(tile_key(info.id, 0, 8000, 1, 1)).unwrap());

    assert_eq!(tile.pixel(0, 0), WHITE);
    assert_eq!(tile.pixel(63, 63), WHITE);
    assert_eq!(tile.pixel(65, 65), RED);
    assert_eq!(tile.pixel(511, 511), RED);
}

#[test]
fn edge_tiles_are_only_as_big_as_the_page() {
    let dir = temp_dir("edge-tile");
    let path = dir.join("one.pdf");
    write_test_pdf(&path, 1);
    let worker = start_worker(MEGABYTE);
    let info = worker.open(&path, None).unwrap();

    let tile = Image::from_png(&worker.tile(tile_key(info.id, 0, 1000, 1, 1)).unwrap());

    assert_eq!((tile.width, tile.height), (612 - 512, 792 - 512));
}

#[test]
fn the_cache_stays_within_budget_on_a_long_document() {
    let dir = temp_dir("budget");
    let path = dir.join("long.pdf");
    write_test_pdf(&path, 500);
    let budget = 64 * 1024;
    let worker = start_worker(budget);
    let info = worker.open(&path, None).unwrap();

    for page in 0..500 {
        worker.tile(tile_key(info.id, page, 1000, 0, 0)).unwrap();
        assert!(worker.stats().unwrap().cached_bytes <= budget);
    }

    let stats = worker.stats().unwrap();
    assert!(stats.cached_tiles > 0, "some tiles should still be cached");
    assert!(
        stats.cached_tiles < 500,
        "old tiles should have been dropped"
    );
}

#[test]
fn asking_twice_returns_the_cached_tile() {
    let dir = temp_dir("cache-hit");
    let path = dir.join("one.pdf");
    write_test_pdf(&path, 1);
    let worker = start_worker(MEGABYTE);
    let info = worker.open(&path, None).unwrap();
    let key = tile_key(info.id, 0, 1000, 0, 0);

    let first = worker.tile(key).unwrap();
    let second = worker.tile(key).unwrap();

    // the same allocation, not merely equal bytes
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert_eq!(worker.stats().unwrap().cached_tiles, 1);
}

#[test]
fn closing_a_document_frees_its_tiles() {
    let dir = temp_dir("close");
    let path = dir.join("one.pdf");
    write_test_pdf(&path, 3);
    let worker = start_worker(MEGABYTE);
    let info = worker.open(&path, None).unwrap();
    let key = tile_key(info.id, 0, 1000, 0, 0);
    worker.tile(key).unwrap();

    worker.close(info.id);

    let stats = worker.stats().unwrap();
    assert_eq!(stats.open_documents, 0);
    assert_eq!(stats.cached_bytes, 0);
    assert_eq!(worker.tile(key), Err(PdfError::UnknownDocument));
}

#[test]
fn many_windows_share_one_engine_and_one_cache() {
    let dir = temp_dir("shared");
    let path = dir.join("shared.pdf");
    write_test_pdf(&path, 20);
    let budget = 64 * 1024;
    let worker = start_worker(budget);

    // each thread plays the part of one window with its own document
    let windows: Vec<_> = (0..8)
        .map(|_| {
            let worker = worker.clone();
            let path = path.clone();
            thread::spawn(move || {
                let info = worker.open(&path, None).unwrap();
                for page in 0..20 {
                    worker.tile(tile_key(info.id, page, 1000, 0, 0)).unwrap();
                }
            })
        })
        .collect();
    for window in windows {
        window.join().unwrap();
    }

    let stats = worker.stats().unwrap();
    assert_eq!(stats.open_documents, 8);
    assert!(
        stats.cached_bytes <= budget,
        "eight windows still share one budget"
    );
}

#[test]
fn reports_pages_and_tiles_that_do_not_exist() {
    let dir = temp_dir("out-of-range");
    let path = dir.join("one.pdf");
    write_test_pdf(&path, 1);
    let worker = start_worker(MEGABYTE);
    let info = worker.open(&path, None).unwrap();

    assert_eq!(
        worker.tile(tile_key(info.id, 5, 1000, 0, 0)),
        Err(PdfError::UnknownPage(5))
    );
    assert_eq!(
        worker.tile(tile_key(info.id, 0, 1000, 9, 9)),
        Err(PdfError::UnknownTile)
    );
}

#[test]
fn explains_why_a_file_cannot_be_opened() {
    let dir = temp_dir("bad-files");
    let worker = start_worker(MEGABYTE);

    let missing = worker.open(&dir.join("missing.pdf"), None);
    assert!(matches!(missing, Err(PdfError::CannotOpen(_))));

    let not_a_pdf = dir.join("notes.pdf");
    fs::write(&not_a_pdf, "just some text").unwrap();
    let damaged = worker.open(&not_a_pdf, None);
    assert!(matches!(damaged, Err(PdfError::CannotOpen(_))));
}

#[test]
fn reports_a_missing_engine_instead_of_crashing() {
    let empty = temp_dir("no-engine");
    let result = RenderWorker::start(&empty, MEGABYTE);
    assert!(matches!(result, Err(PdfError::EngineUnavailable(_))));
}
