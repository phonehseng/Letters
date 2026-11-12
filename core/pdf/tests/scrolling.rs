//! end to end tests of fast scrolling: after a flick through a long
//! document, the page the reader stopped on is drawn before the pages they
//! passed, and tiles for pages well behind them are never drawn at all.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use letters_pdf::{DocumentId, PdfError, RenderWorker, TileKey, Viewport};
use support::{pdfium_dir, temp_dir, write_text_pdf, TextLine};

const MEGABYTE: usize = 1024 * 1024;
const PAGES: u32 = 100;
/// the scale the "viewer" in these tests draws at
const SCALE_MILLI: u32 = 1000;

/// how one tile request ended: which page, in which place it came back,
/// and whether it was drawn or given up on.
struct Outcome {
    page: u32,
    finished_as: usize,
    drawn: bool,
}

fn long_document(test_name: &str) -> (RenderWorker, DocumentId) {
    let path = temp_dir(test_name).join("long.pdf");
    let pages: Vec<Vec<TextLine>> = (0..PAGES)
        .map(|page| vec![TextLine::new(72.0, 100.0, 24.0, &format!("Page {page}"))])
        .collect();
    write_text_pdf(&path, &pages, &[]);
    let worker = RenderWorker::start(&pdfium_dir(), 64 * MEGABYTE).unwrap();
    let document = worker.open(&path, None).unwrap().id;
    (worker, document)
}

fn first_tile(document: DocumentId, page: u32) -> TileKey {
    TileKey {
        document,
        page,
        scale_milli: SCALE_MILLI,
        column: 0,
        row: 0,
    }
}

fn showing(page: u32) -> Viewport {
    Viewport {
        generation: 1,
        first_page: page,
        last_page: page,
        scale_milli: SCALE_MILLI,
    }
}

/// asks for the first tile of every page, in page order, as a flick from
/// the top of the document to the bottom would, each from its own thread
/// the way each tile image asks on its own.
fn flick_through(worker: &RenderWorker, document: DocumentId) -> Vec<thread::JoinHandle<Outcome>> {
    let finished = Arc::new(AtomicUsize::new(0));
    (0..PAGES)
        .map(|page| {
            let worker = worker.clone();
            let finished = finished.clone();
            thread::spawn(move || {
                let result = worker.tile(first_tile(document, page));
                let finished_as = finished.fetch_add(1, Ordering::SeqCst);
                match result {
                    Ok(_) => Outcome {
                        page,
                        finished_as,
                        drawn: true,
                    },
                    Err(PdfError::Superseded) => Outcome {
                        page,
                        finished_as,
                        drawn: false,
                    },
                    Err(other) => panic!("page {page} failed: {other}"),
                }
            })
        })
        .collect()
}

fn outcomes(requests: Vec<thread::JoinHandle<Outcome>>) -> Vec<Outcome> {
    requests
        .into_iter()
        .map(|request| request.join().unwrap())
        .collect()
}

#[test]
fn after_a_flick_to_the_end_the_last_page_is_drawn_before_most_others() {
    let (worker, document) = long_document("scroll-to-end");
    let requests = flick_through(&worker, document);
    worker.set_viewport(document, showing(PAGES - 1));
    let outcomes = outcomes(requests);

    let last = outcomes.iter().find(|o| o.page == PAGES - 1).unwrap();
    assert!(last.drawn, "the page on screen was not drawn");
    // given-up tiles are answered at once, so what counts is how much
    // drawing happened before the page on screen, and how much at all
    let drawn_before = outcomes
        .iter()
        .filter(|o| o.drawn && o.finished_as < last.finished_as)
        .count();
    let drawn = outcomes.iter().filter(|o| o.drawn).count();
    assert!(
        drawn_before < PAGES as usize / 10,
        "{drawn_before} other pages were drawn before the page on screen"
    );
    assert!(
        drawn < PAGES as usize / 2,
        "{drawn} of {PAGES} pages were drawn; most of the pages passed should be dropped"
    );
}

#[test]
fn stopping_part_way_draws_that_page_and_drops_the_pages_far_from_it() {
    let (worker, document) = long_document("scroll-to-middle");
    // the viewer says where it stopped while the tiles of the pages it
    // passed are still on their way: newest first alone would draw pages
    // 99 down to 51 before page 50
    worker.set_viewport(document, showing(50));
    let outcomes = outcomes(flick_through(&worker, document));

    let middle = outcomes.iter().find(|o| o.page == 50).unwrap();
    assert!(middle.drawn, "the page on screen was not drawn");
    // only the page on screen and its near neighbours are drawn
    let far_and_drawn: Vec<u32> = outcomes
        .iter()
        .filter(|o| o.drawn && !(48..=52).contains(&o.page))
        .map(|o| o.page)
        .collect();
    assert!(
        far_and_drawn.is_empty(),
        "pages far from the screen were drawn: {far_and_drawn:?}"
    );
}

#[test]
fn thumbnails_at_another_scale_are_still_drawn() {
    let (worker, document) = long_document("scroll-thumbnails");
    worker.set_viewport(document, showing(50));
    // a sidebar thumbnail of the first page, at a fifth of the size
    let thumbnail = TileKey {
        scale_milli: 200,
        ..first_tile(document, 0)
    };
    assert!(worker.tile(thumbnail).is_ok());
}
