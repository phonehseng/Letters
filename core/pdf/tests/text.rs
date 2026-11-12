//! end to end tests of the outline and text search: real pdf files with
//! real text, read by the real pdfium library.

mod support;

use std::path::PathBuf;

use letters_pdf::{
    DocumentId, OutlineItem, PageText, PdfError, RenderWorker, TextRect, MAX_SEARCH_MATCHES,
};
use support::{
    pdfium_dir, temp_dir, write_rotated_text_pdf, write_text_pdf, Bookmark, Target, TextLine,
};

const MEGABYTE: usize = 1024 * 1024;

fn start_worker() -> RenderWorker {
    RenderWorker::start(&pdfium_dir(), MEGABYTE).unwrap()
}

fn item(title: &str, page: u32, level: u32) -> OutlineItem {
    OutlineItem {
        title: title.to_owned(),
        page,
        level,
    }
}

/// a three page "biology notes" pdf with known words at known places.
///
/// page 0: "Photosynthesis turns light energy" with its baseline 100
///         points from the top, then "into chemical energy." below it
/// page 1: "Cells need LIGHT to grow."
/// page 2: "Nothing else here."
fn write_notes(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("notes.pdf");
    let pages = vec![
        vec![
            TextLine::new(72.0, 100.0, 24.0, "Photosynthesis turns light energy"),
            TextLine::new(72.0, 140.0, 24.0, "into chemical energy."),
        ],
        vec![TextLine::new(
            72.0,
            100.0,
            24.0,
            "Cells need LIGHT to grow.",
        )],
        vec![TextLine::new(72.0, 100.0, 24.0, "Nothing else here.")],
    ];
    write_text_pdf(&path, &pages, &[]);
    path
}

fn open_notes(test_name: &str) -> (RenderWorker, DocumentId) {
    let dir = temp_dir(test_name);
    let worker = start_worker();
    let id = worker.open(&write_notes(&dir), None).unwrap().id;
    (worker, id)
}

fn pages_of(worker: &RenderWorker, id: DocumentId, query: &str) -> Vec<u32> {
    let results = worker.search(id, query).unwrap();
    results.matches.iter().map(|found| found.page).collect()
}

/// pages each holding `lines` lines of "ab ab ab ..." with `per_line` words.
fn repeated_words(page_count: usize, lines: usize, per_line: usize) -> Vec<Vec<TextLine>> {
    let line_text = vec!["ab"; per_line].join(" ");
    (0..page_count)
        .map(|_| {
            (0..lines)
                .map(|line| TextLine::new(20.0, 40.0 + 12.0 * line as f32, 8.0, &line_text))
                .collect()
        })
        .collect()
}

// ---- outline ----

#[test]
fn outline_comes_back_in_reading_order_with_levels_and_pages() {
    let dir = temp_dir("outline");
    let path = dir.join("book.pdf");
    let outline = [
        Bookmark::new(
            "Chapter 1",
            Target::Page(0),
            vec![
                Bookmark::new("Section 1.1", Target::GoToAction(0), vec![]),
                Bookmark::new(
                    "Section 1.2",
                    Target::Page(1),
                    vec![Bookmark::new("Detail", Target::Page(1), vec![])],
                ),
            ],
        ),
        Bookmark::new("Chapter 2", Target::Page(2), vec![]),
    ];
    write_text_pdf(&path, &[vec![], vec![], vec![]], &outline);
    let worker = start_worker();
    let id = worker.open(&path, None).unwrap().id;

    assert_eq!(
        worker.outline(id).unwrap(),
        [
            item("Chapter 1", 0, 0),
            item("Section 1.1", 0, 1),
            item("Section 1.2", 1, 1),
            item("Detail", 1, 2),
            item("Chapter 2", 2, 0),
        ]
    );
}

#[test]
fn outline_entries_that_lead_nowhere_are_left_out() {
    let dir = temp_dir("outline-nowhere");
    let path = dir.join("book.pdf");
    let outline = [
        Bookmark::new("Introduction", Target::Page(0), vec![]),
        // a heading with no page of its own: its child moves up a level
        Bookmark::new(
            "Appendices",
            Target::Nowhere,
            vec![Bookmark::new("Appendix A", Target::Page(1), vec![])],
        ),
    ];
    write_text_pdf(&path, &[vec![], vec![]], &outline);
    let worker = start_worker();
    let id = worker.open(&path, None).unwrap().id;

    assert_eq!(
        worker.outline(id).unwrap(),
        [item("Introduction", 0, 0), item("Appendix A", 1, 0)]
    );
}

#[test]
fn a_pdf_without_an_outline_gives_an_empty_list() {
    let (worker, id) = open_notes("no-outline");
    assert!(worker.outline(id).unwrap().is_empty());
}

#[test]
fn the_outline_of_a_closed_document_is_an_error() {
    let (worker, id) = open_notes("outline-closed");
    worker.close(id);
    assert_eq!(worker.outline(id), Err(PdfError::UnknownDocument));
}

// ---- search ----

#[test]
fn search_finds_the_right_pages() {
    let (worker, id) = open_notes("search-pages");
    assert_eq!(pages_of(&worker, id, "energy"), [0, 0]);
    assert_eq!(pages_of(&worker, id, "Nothing"), [2]);
    assert!(pages_of(&worker, id, "mitochondria").is_empty());
}

#[test]
fn search_ignores_case() {
    let (worker, id) = open_notes("search-case");
    assert_eq!(pages_of(&worker, id, "light"), [0, 1]);
    assert_eq!(pages_of(&worker, id, "LIGHT"), [0, 1]);
    assert_eq!(pages_of(&worker, id, "pHoToSyNtHeSiS"), [0]);
}

/// the size every test line here is set at.
const LINE_SIZE: f32 = 24.0;

/// the box covering the letters of `word` where it first appears on a page,
/// from pdfium's own box for each letter.
///
/// these boxes are loose: each starts at the letter's pen position and spans
/// the font's full height, so they don't depend on which font draws the text.
/// that matters here because the test pdfs use helvetica without embedding
/// it, and pdfium draws it with whatever the system offers (arial on windows,
/// another sans on linux and macos).
fn letter_boxes(page: &PageText, word: &str) -> TextRect {
    let letters: Vec<char> = word.chars().collect();
    let chars: Vec<char> = page.chars.iter().map(|c| c.ch).collect();
    let start = chars
        .windows(letters.len())
        .position(|window| window == letters.as_slice())
        .expect("the word is on the page");
    let boxes = page.chars[start..start + letters.len()]
        .iter()
        .map(|c| c.rect);
    let left = boxes.clone().map(|r| r.x).fold(f32::MAX, f32::min);
    let top = boxes.clone().map(|r| r.y).fold(f32::MAX, f32::min);
    let right = boxes
        .clone()
        .map(|r| r.x + r.width)
        .fold(f32::MIN, f32::max);
    let bottom = boxes.map(|r| r.y + r.height).fold(f32::MIN, f32::max);
    TextRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

/// a match is the letters' tight ink box: inside their loose boxes, and
/// most of their length, so it covers every letter and not just one.
fn assert_covers_the_letters(found: TextRect, letters: TextRect) {
    // half a point for rounding where ink touches the edge of its box
    let slack = 0.5;
    let inside = found.x >= letters.x - slack
        && found.y >= letters.y - slack
        && found.x + found.width <= letters.x + letters.width + slack
        && found.y + found.height <= letters.y + letters.height + slack;
    let long_side = |r: TextRect| r.width.max(r.height);
    let covers = long_side(found) >= 0.9 * long_side(letters);
    assert!(
        inside && covers,
        "the match {found:?} doesn't cover the letters {letters:?}"
    );
}

#[test]
fn match_rectangles_are_measured_from_the_top_left_and_cover_the_text() {
    let (worker, id) = open_notes("search-rects");
    let results = worker.search(id, "Photosynthesis").unwrap();
    assert_eq!(results.matches.len(), 1);
    let rects = &results.matches[0].rects;
    assert_eq!(rects.len(), 1);
    let rect = rects[0];
    let letters = letter_boxes(&worker.page_text(id, 0).unwrap(), "Photosynthesis");
    assert_covers_the_letters(rect, letters);

    // and it is where the pdf put the word, measured from the top left: the
    // letters start at the pen, x = 72, whatever the font, and the baseline
    // is 100 points from the top. a letter's side bearing is a small part of
    // its size, the ink rises above the baseline by less than the size, and
    // the "y" hangs below it
    assert!(
        (letters.x - 72.0).abs() < 0.5,
        "the letters start at {}",
        letters.x
    );
    let baseline = 100.0;
    assert!(
        (71.0..72.0 + 0.25 * LINE_SIZE).contains(&rect.x),
        "x was {}",
        rect.x
    );
    assert!(
        (baseline - LINE_SIZE..baseline).contains(&rect.y),
        "top was {}",
        rect.y
    );
    let bottom = rect.y + rect.height;
    assert!(
        (baseline..baseline + 0.5 * LINE_SIZE).contains(&bottom),
        "bottom was {bottom}"
    );
    // fourteen letters of a sans serif are between 0.4 and 0.8 of the size wide each
    assert!(
        (14.0 * 0.4 * LINE_SIZE..14.0 * 0.8 * LINE_SIZE).contains(&rect.width),
        "width was {}",
        rect.width
    );
}

#[test]
fn match_rectangles_follow_a_rotated_page() {
    let dir = temp_dir("search-rotated");
    let path = dir.join("sideways.pdf");
    let lines = vec![TextLine::new(72.0, 100.0, LINE_SIZE, "Photosynthesis")];
    write_rotated_text_pdf(&path, &[lines], &[], 90);
    let worker = start_worker();
    let info = worker.open(&path, None).unwrap();
    // turned a quarter clockwise, the page is shown landscape
    assert_eq!(info.page_sizes[0].width_pt, 792.0);

    let rect = worker.search(info.id, "Photosynthesis").unwrap().matches[0].rects[0];
    let letters = letter_boxes(&worker.page_text(info.id, 0).unwrap(), "Photosynthesis");
    assert_covers_the_letters(rect, letters);

    // the baseline, 100 points from the top of the unturned page, is now
    // 692 points from the left. the letters rise to the right of it and the
    // "y" hangs to its left, and the word runs downward from the pen at y = 72
    assert!(
        (letters.y - 72.0).abs() < 0.5,
        "the letters start at {}",
        letters.y
    );
    let baseline = 692.0;
    let right = rect.x + rect.width;
    assert!(
        (baseline - 0.5 * LINE_SIZE..baseline).contains(&rect.x),
        "x was {}",
        rect.x
    );
    assert!(
        (baseline..baseline + LINE_SIZE).contains(&right),
        "right was {right}"
    );
    assert!(
        (71.0..72.0 + 0.25 * LINE_SIZE).contains(&rect.y),
        "y was {}",
        rect.y
    );
    assert!(
        (14.0 * 0.4 * LINE_SIZE..14.0 * 0.8 * LINE_SIZE).contains(&rect.height),
        "height was {}",
        rect.height
    );
}

#[test]
fn a_match_further_along_the_line_starts_further_right() {
    let (worker, id) = open_notes("search-rects-offset");
    let start = worker.search(id, "Photosynthesis").unwrap().matches[0].rects[0];
    let later = worker.search(id, "light").unwrap().matches[0].rects[0];
    assert!(later.x > start.x + start.width, "light was at {}", later.x);
    assert!((later.y - start.y).abs() < 6.0, "same line, same height");
}

#[test]
fn a_match_on_the_second_line_is_lower_down() {
    let (worker, id) = open_notes("search-second-line");
    let results = worker.search(id, "chemical").unwrap();
    let rect = results.matches[0].rects[0];
    // the second line's baseline is 140 points from the top
    assert!(
        (130.0..=148.0).contains(&(rect.y + rect.height)),
        "bottom was {}",
        rect.y + rect.height
    );
}

#[test]
fn a_phrase_broken_over_two_lines_is_found_with_one_rectangle_per_line() {
    let (worker, id) = open_notes("search-two-lines");
    let results = worker.search(id, "energy into").unwrap();
    assert_eq!(results.matches.len(), 1);
    let rects = &results.matches[0].rects;
    assert_eq!(rects.len(), 2, "rects were {rects:?}");
    assert!(rects[1].y > rects[0].y, "the second rectangle is lower");
}

#[test]
fn snippets_show_the_words_around_the_match() {
    let (worker, id) = open_notes("search-snippet");
    let results = worker.search(id, "need").unwrap();
    assert_eq!(results.matches[0].snippet, "Cells need LIGHT to grow.");
}

#[test]
fn an_empty_query_finds_nothing() {
    let (worker, id) = open_notes("search-empty");
    for query in ["", "   ", "\t\n"] {
        let results = worker.search(id, query).unwrap();
        assert!(results.matches.is_empty());
        assert!(!results.truncated);
    }
}

#[test]
fn search_stops_at_the_limit_and_says_so() {
    let dir = temp_dir("search-truncated");
    let path = dir.join("many.pdf");
    // 2 pages of 10 lines of 30 words: 600 matches
    write_text_pdf(&path, &repeated_words(2, 10, 30), &[]);
    let worker = start_worker();
    let id = worker.open(&path, None).unwrap().id;

    let results = worker.search(id, "ab").unwrap();
    assert_eq!(results.matches.len(), MAX_SEARCH_MATCHES);
    assert!(results.truncated);
}

#[test]
fn exactly_the_limit_is_not_truncated() {
    let dir = temp_dir("search-exactly-limit");
    let path = dir.join("many.pdf");
    // 2 pages of 10 lines of 25 words: exactly 500 matches
    write_text_pdf(&path, &repeated_words(2, 10, 25), &[]);
    let worker = start_worker();
    let id = worker.open(&path, None).unwrap().id;

    let results = worker.search(id, "ab").unwrap();
    assert_eq!(results.matches.len(), MAX_SEARCH_MATCHES);
    assert!(!results.truncated);
}

#[test]
fn search_covers_every_page_of_a_long_document() {
    let dir = temp_dir("search-long");
    let path = dir.join("long.pdf");
    // more pages than one search slice holds, with the word on the first
    // page, one page into the second slice and the very last page
    let pages: Vec<Vec<TextLine>> = (0..45)
        .map(|page| {
            let word = if [0, 21, 44].contains(&page) {
                "needle"
            } else {
                "hay"
            };
            vec![TextLine::new(72.0, 100.0, 12.0, word)]
        })
        .collect();
    write_text_pdf(&path, &pages, &[]);
    let worker = start_worker();
    let id = worker.open(&path, None).unwrap().id;

    assert_eq!(pages_of(&worker, id, "needle"), [0, 21, 44]);
    assert_eq!(pages_of(&worker, id, "hay").len(), 42);
}

#[test]
fn searching_a_closed_document_is_an_error() {
    let (worker, id) = open_notes("search-closed");
    worker.close(id);
    assert_eq!(worker.search(id, "light"), Err(PdfError::UnknownDocument));
}

#[test]
fn tiles_can_still_be_drawn_after_a_search() {
    let (worker, id) = open_notes("search-then-tiles");
    worker.search(id, "energy").unwrap();
    let key = letters_pdf::TileKey {
        document: id,
        page: 0,
        scale_milli: 1000,
        column: 0,
        row: 0,
    };
    assert!(worker.tile(key).is_ok());
}
