//! end to end tests of reading where a page's characters are: real pdf
//! files with real text, read by the real pdfium library.

mod support;

use std::path::{Path, PathBuf};

use letters_pdf::{DocumentId, PageText, PdfError, RenderWorker};
use support::{pdfium_dir, temp_dir, write_rotated_text_pdf, write_text_pdf, TextLine};

const MEGABYTE: usize = 1024 * 1024;

fn start_worker() -> RenderWorker {
    RenderWorker::start(&pdfium_dir(), MEGABYTE).unwrap()
}

/// page 0: two lines of helvetica at 24 points, baselines 100 and 140
/// points from the top. page 1 is empty.
fn write_notes(dir: &Path) -> PathBuf {
    let path = dir.join("notes.pdf");
    let pages = vec![
        vec![
            TextLine::new(72.0, 100.0, 24.0, "Photosynthesis turns light"),
            TextLine::new(72.0, 140.0, 24.0, "into energy."),
        ],
        vec![],
    ];
    write_text_pdf(&path, &pages, &[]);
    path
}

fn open_notes(test_name: &str) -> (RenderWorker, DocumentId) {
    let worker = start_worker();
    let id = worker
        .open(&write_notes(&temp_dir(test_name)), None)
        .unwrap()
        .id;
    (worker, id)
}

fn text_of(page: &PageText) -> String {
    page.chars.iter().map(|character| character.ch).collect()
}

/// the characters of one line, as text.
fn line_text(page: &PageText, line: u32) -> String {
    page.chars
        .iter()
        .filter(|character| character.line == line)
        .map(|character| character.ch)
        .collect()
}

#[test]
fn the_characters_come_back_in_reading_order_with_their_spaces() {
    let (worker, id) = open_notes("text-order");
    let page = worker.page_text(id, 0).unwrap();
    assert_eq!(text_of(&page), "Photosynthesis turns lightinto energy.");
    assert_eq!(line_text(&page, 0), "Photosynthesis turns light");
    assert_eq!(line_text(&page, 1), "into energy.");
    assert_eq!(page.lines.len(), 2);
}

#[test]
fn character_boxes_are_measured_from_the_top_left() {
    let (worker, id) = open_notes("text-boxes");
    let page = worker.page_text(id, 0).unwrap();
    let first = page.chars[0];
    assert_eq!(first.ch, 'P');
    assert!((first.rect.x - 72.0).abs() < 2.0, "x was {}", first.rect.x);
    // the loose box runs from about the font's ascent above the
    // baseline (100) to its descent below it
    let top = first.rect.y;
    let bottom = first.rect.y + first.rect.height;
    assert!((70.0..=84.0).contains(&top), "top was {top}");
    assert!((100.0..=110.0).contains(&bottom), "bottom was {bottom}");
}

#[test]
fn letters_further_along_a_line_are_further_right() {
    let (worker, id) = open_notes("text-advance");
    let page = worker.page_text(id, 0).unwrap();
    let on_first_line: Vec<_> = page.chars.iter().filter(|c| c.line == 0).collect();
    for pair in on_first_line.windows(2) {
        assert!(
            pair[1].rect.x >= pair[0].rect.x,
            "{} came before {}",
            pair[1].ch,
            pair[0].ch
        );
    }
}

#[test]
fn line_boxes_cover_their_characters_and_the_second_is_lower() {
    let (worker, id) = open_notes("text-lines");
    let page = worker.page_text(id, 0).unwrap();
    let [first, second] = [page.lines[0], page.lines[1]];
    assert!(second.y > first.y + first.height - 1.0);
    for character in &page.chars {
        let line = page.lines[character.line as usize];
        assert!(character.rect.x >= line.x - 0.01);
        assert!(character.rect.x + character.rect.width <= line.x + line.width + 0.01);
    }
    // "Photosynthesis turns light" is about 300 points wide at 24 points
    assert!(
        (270.0..=320.0).contains(&first.width),
        "width was {}",
        first.width
    );
}

#[test]
fn the_space_between_words_sits_between_them() {
    let (worker, id) = open_notes("text-space");
    let page = worker.page_text(id, 0).unwrap();
    let space = page.chars.iter().position(|c| c.ch == ' ').unwrap();
    let [before, gap, after] = [
        page.chars[space - 1],
        page.chars[space],
        page.chars[space + 1],
    ];
    assert!(gap.rect.x >= before.rect.x + before.rect.width - 0.5);
    assert!(gap.rect.x + gap.rect.width <= after.rect.x + 0.5);
    assert_eq!(gap.line, 0);
}

#[test]
fn a_page_without_text_has_no_characters() {
    let (worker, id) = open_notes("text-empty");
    let page = worker.page_text(id, 1).unwrap();
    assert!(page.chars.is_empty());
    assert!(page.lines.is_empty());
}

#[test]
fn text_on_a_turned_page_runs_down_the_screen() {
    let dir = temp_dir("text-rotated");
    let path = dir.join("sideways.pdf");
    let lines = vec![TextLine::new(72.0, 100.0, 24.0, "Photosynthesis")];
    write_rotated_text_pdf(&path, &[lines], &[], 90);
    let worker = start_worker();
    let id = worker.open(&path, None).unwrap().id;

    let page = worker.page_text(id, 0).unwrap();
    assert_eq!(text_of(&page), "Photosynthesis");
    assert_eq!(page.lines.len(), 1);
    let line = page.lines[0];
    // the same place find reports: about 692 points from the left, the
    // word running downward from y = 72
    assert!((680.0..=696.0).contains(&line.x), "x was {}", line.x);
    assert!((line.y - 72.0).abs() < 2.0, "y was {}", line.y);
    assert!(
        line.height > line.width * 4.0,
        "the line is tall and narrow"
    );
    for pair in page.chars.windows(2) {
        assert!(pair[1].rect.y >= pair[0].rect.y, "letters go down the page");
    }
}

#[test]
fn a_page_past_the_end_is_an_error() {
    let (worker, id) = open_notes("text-past-end");
    assert_eq!(worker.page_text(id, 5), Err(PdfError::UnknownPage(5)));
}

#[test]
fn the_text_of_a_closed_document_is_an_error() {
    let (worker, id) = open_notes("text-closed");
    worker.close(id);
    assert_eq!(worker.page_text(id, 0), Err(PdfError::UnknownDocument));
}

#[test]
fn after_a_reload_the_new_file_s_text_is_read() {
    let dir = temp_dir("text-reload");
    let worker = start_worker();
    let id = worker.open(&write_notes(&dir), None).unwrap().id;
    // ask once so the answer is cached
    worker.page_text(id, 0).unwrap();

    let changed = dir.join("changed.pdf");
    write_text_pdf(
        &changed,
        &[vec![TextLine::new(72.0, 100.0, 24.0, "Mitochondria")]],
        &[],
    );
    worker.reload(id, &changed).unwrap();
    assert_eq!(text_of(&worker.page_text(id, 0).unwrap()), "Mitochondria");
}
