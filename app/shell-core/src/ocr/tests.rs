//! recognition runs with a fake engine (progress, cancel, failures), and
//! the text layer written through an edit session the way the app does.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use letters_ocr::{
    has_text_layer, images_to_pdf, Baseline, CancelFlag, ImageSource, ImagesToPdfOptions, OcrError,
    OcrPage, OcrWord, PaperSize, Point, Rect,
};

use super::apply_layers;
use super::engine_folder::{candidates, first_installed, missing_message, plain_path};
use super::job::{run_job, JobOutcome, JobProgress, PageReader};
use super::jobs::{JobState, OcrJobs};
use super::overview::scan_overview;
use super::summary::{summarize, LOW_CONFIDENCE};
use super::words::{correct_word, ocr_page_from_objects, overlay_words, words_to_correct};
use crate::session::EditSession;
use crate::test_pdf::temp_dir;

fn word(text: &str, x: f32, y: f32, confidence: f32) -> OcrWord {
    let width = 8.0 * text.len() as f32;
    OcrWord {
        text: text.to_owned(),
        rect: Rect::new(x, y, width, 14.0),
        baseline: Baseline {
            start: Point::new(x, y + 11.0),
            end: Point::new(x + width, y + 11.0),
        },
        font_size: 12.0,
        confidence,
        line: 0,
        paragraph: 0,
        block: 0,
        language: Some("eng".into()),
    }
}

fn page_of(words: Vec<OcrWord>) -> OcrPage {
    let mean = if words.is_empty() {
        0.0
    } else {
        words.iter().map(|word| word.confidence).sum::<f32>() / words.len() as f32
    };
    OcrPage {
        width: 612.0,
        height: 792.0,
        words,
        language: "eng".into(),
        skew_angle: 0.0,
        orientation: 0,
        mean_confidence: mean,
        steps: Vec::new(),
    }
}

/// reads every page as "Page N", fails on `fail_on`, and pulls the cancel
/// switch after reading `cancel_after` pages, as a person pressing Cancel would.
struct FakeReader {
    fail_on: Option<u32>,
    cancel_after: Option<usize>,
    cancel: CancelFlag,
    read: Cell<usize>,
}

impl FakeReader {
    fn new(cancel: &CancelFlag) -> Self {
        Self {
            fail_on: None,
            cancel_after: None,
            cancel: cancel.clone(),
            read: Cell::new(0),
        }
    }
}

impl PageReader for FakeReader {
    fn read(&mut self, page: u32, cancel: &CancelFlag) -> Result<OcrPage, OcrError> {
        if cancel.is_cancelled() {
            return Err(OcrError::Cancelled);
        }
        if self.fail_on == Some(page) {
            return Err(OcrError::EngineFailed("tesseract stopped".into()));
        }
        self.read.set(self.read.get() + 1);
        if self.cancel_after == Some(self.read.get()) {
            self.cancel.cancel();
        }
        Ok(page_of(vec![
            word("Page", 72.0, 72.0, 90.0),
            word(&(page + 1).to_string(), 120.0, 72.0, 90.0),
        ]))
    }
}

fn show(path: &Path) -> Result<PathBuf, String> {
    Ok(path.to_path_buf())
}

/// a pdf of `count` scanned pages, each one plain white picture, opened in a session.
fn scanned_session(test_name: &str, count: usize) -> EditSession {
    let dir = temp_dir(test_name);
    let picture = white_png(300, 400);
    let sources: Vec<ImageSource> = (0..count)
        .map(|_| ImageSource::Bytes(picture.clone()))
        .collect();
    // letter pages, so the fake words (measured on a letter page) land where they say
    let options = ImagesToPdfOptions {
        paper: PaperSize::Letter,
        ..ImagesToPdfOptions::default()
    };
    let mut doc = images_to_pdf(&sources, &options, None, &mut |_| {}, &CancelFlag::new()).unwrap();
    let original = dir.join("scan.pdf");
    doc.save(&original).unwrap();
    EditSession::start(&original, &dir.join("sessions")).unwrap()
}

fn white_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![255; (width * height) as usize])
            .unwrap();
    }
    bytes
}

#[test]
fn a_run_reads_every_page_in_order_and_says_how_far_it_got() {
    let cancel = CancelFlag::new();
    let mut reader = FakeReader::new(&cancel);
    let mut heard = Vec::new();
    let outcome = run_job(&[0, 2, 5], &mut reader, &cancel, &mut |progress| {
        heard.push(progress)
    });
    let JobOutcome::Finished(pages) = outcome else {
        panic!("the run should finish");
    };
    assert_eq!(
        pages.iter().map(|(page, _)| *page).collect::<Vec<_>>(),
        vec![0, 2, 5]
    );
    assert_eq!(pages[1].1.words[1].text, "3");
    assert_eq!(
        heard.first(),
        Some(&JobProgress {
            done: 0,
            total: 3,
            current: Some(0)
        })
    );
    assert_eq!(
        heard.last(),
        Some(&JobProgress {
            done: 3,
            total: 3,
            current: None
        })
    );
}

#[test]
fn cancelling_stops_before_the_next_page_and_keeps_nothing() {
    let cancel = CancelFlag::new();
    let mut reader = FakeReader::new(&cancel);
    reader.cancel_after = Some(2);
    let outcome = run_job(&[0, 1, 2, 3, 4], &mut reader, &cancel, &mut |_| {});
    assert_eq!(outcome, JobOutcome::Cancelled { done: 2 });
    assert_eq!(reader.read.get(), 2, "no page after the cancel was read");
}

#[test]
fn a_failed_page_fails_the_run_and_leaves_the_document_unchanged() {
    let mut session = scanned_session("ocr-partial-failure", 4);
    let jobs = OcrJobs::new();
    let (id, cancel) = jobs.start(7, 4).unwrap();
    let mut reader = FakeReader::new(&cancel);
    reader.fail_on = Some(2);
    jobs.run(id, &[0, 1, 2, 3], &mut reader, &cancel);

    let status = jobs.status(id).unwrap();
    assert_eq!(status.state, JobState::Failed);
    assert!(
        status
            .error
            .as_deref()
            .unwrap()
            .starts_with("Page 3 couldn’t be read"),
        "{status:?}"
    );
    assert!(
        jobs.take_pages(id).is_none(),
        "a failed run has nothing to add"
    );
    // nothing was applied, so the document is as it was
    assert_eq!(session.revision(), 0);
    assert!(!session.can_undo());
    let doc = session.read_document().unwrap();
    assert!((0..4).all(|page| !has_text_layer(&doc, page).unwrap()));
    // and an edit that fails half way through adds nothing either
    let pages = vec![
        (0, page_of(vec![word("Hello", 72.0, 72.0, 95.0)])),
        (9, page_of(vec![])),
    ];
    assert!(session
        .apply(|doc| apply_layers(doc, &pages), show)
        .is_err());
    assert_eq!(session.revision(), 0);
    let doc = session.read_document().unwrap();
    assert!(
        !has_text_layer(&doc, 0).unwrap(),
        "page 1 was not left half done"
    );
}

#[test]
fn a_finished_run_is_added_as_one_undo_step() {
    let mut session = scanned_session("ocr-one-step", 3);
    let jobs = OcrJobs::new();
    let (id, cancel) = jobs.start(3, 3).unwrap();
    jobs.run(id, &[0, 1, 2], &mut FakeReader::new(&cancel), &cancel);
    let status = jobs.status(id).unwrap();
    assert_eq!(status.state, JobState::Finished);
    assert_eq!(status.summary.as_ref().unwrap().pages_with_text, 3);

    let (document, pages) = jobs.take_pages(id).unwrap();
    assert_eq!(document, 3);
    assert!(jobs.status(id).is_none(), "pages are collected once");
    session
        .apply(|doc| apply_layers(doc, &pages), show)
        .unwrap();
    assert_eq!(session.revision(), 1);
    let doc = session.read_document().unwrap();
    assert!((0..3).all(|page| has_text_layer(&doc, page).unwrap()));
    let overview = scan_overview(&doc);
    assert_eq!(overview.scanned, vec![0, 1, 2]);
    assert_eq!(overview.with_layer, vec![0, 1, 2]);
    assert!(overview.unread_scans().is_empty());

    // one undo takes every page's text away again
    session.undo(show).unwrap();
    let doc = session.read_document().unwrap();
    assert!((0..3).all(|page| !has_text_layer(&doc, page).unwrap()));
    assert_eq!(scan_overview(&doc).unread_scans(), vec![0, 1, 2]);
}

#[test]
fn one_run_per_document_and_cancel_moves_it_on() {
    let jobs = OcrJobs::new();
    let (first, _) = jobs.start(1, 5).unwrap();
    assert!(
        jobs.start(1, 5).is_err(),
        "a second run of the same document is refused"
    );
    let (other, _) = jobs.start(2, 1).unwrap();
    assert_ne!(first, other);

    assert!(jobs.cancel(first));
    assert_eq!(jobs.status(first).unwrap().state, JobState::Cancelling);
    assert!(!jobs.cancel(first), "cancelling twice does nothing more");
    jobs.finish(first, JobOutcome::Cancelled { done: 1 });
    assert_eq!(jobs.status(first).unwrap().state, JobState::Cancelled);
    // once it ended, the document can be read again
    assert!(jobs.start(1, 5).is_ok());
    jobs.forget_document(2);
    assert!(jobs.status(other).is_none());
}

#[test]
fn the_summary_counts_text_blank_pages_and_doubtful_ones() {
    let results = vec![
        (
            0,
            page_of(vec![
                word("Clear", 72.0, 72.0, 96.0),
                word("text", 130.0, 72.0, 94.0),
            ]),
        ),
        (1, page_of(vec![word(".", 72.0, 72.0, 30.0)])),
        (
            2,
            page_of(vec![
                word("Smudged", 72.0, 72.0, 40.0),
                word("words", 150.0, 72.0, 50.0),
            ]),
        ),
        (3, page_of(vec![])),
    ];
    let summary = summarize(&results);
    assert_eq!(summary.pages, vec![0, 1, 2, 3]);
    assert_eq!(summary.pages_with_text, 2);
    assert_eq!(summary.blank_pages, vec![1, 3], "a lone speck is not text");
    assert_eq!(summary.word_count, 4);
    assert!((summary.mean_confidence - 70.0).abs() < 1e-3);
    assert_eq!(summary.low_confidence_pages.len(), 1);
    assert_eq!(summary.low_confidence_pages[0].page, 2);
    assert!(summary.low_confidence_pages[0].confidence < LOW_CONFIDENCE);
}

#[test]
fn the_engine_is_looked_for_in_the_usual_places() {
    let dir = temp_dir("ocr-engine-folder");
    let installed = dir.join("resources").join("tesseract");
    std::fs::create_dir_all(installed.join("bin")).unwrap();
    std::fs::create_dir_all(installed.join("tessdata")).unwrap();
    let program = if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    };
    std::fs::write(installed.join("bin").join(program), b"").unwrap();
    let list = candidates(
        None,
        Some(dir.join("resources")),
        Some(dir.join("app")),
        dir.join("vendor"),
    );
    assert_eq!(list.len(), 3);
    assert_eq!(first_installed(&list), Some(installed));
    assert_eq!(first_installed(&list[1..]), None);
    assert!(missing_message(true).contains("fetch-tesseract.ps1"));
    assert!(!missing_message(false).contains("fetch"));
    // tauri's resource folders come as \\?\ paths, which tesseract can't use
    assert_eq!(
        plain_path(Path::new(r"\\?\C:\Letters\debug\tesseract")),
        PathBuf::from(r"C:\Letters\debug\tesseract")
    );
    assert_eq!(
        plain_path(Path::new(r"\\?\UNC\server\share\tesseract")),
        PathBuf::from(r"\\server\share\tesseract")
    );
}

#[test]
fn the_overlay_shows_tesseracts_words_while_they_match_the_page() {
    let mut session = scanned_session("ocr-overlay", 1);
    let read = page_of(vec![
        word("Hello", 72.0, 100.0, 92.0),
        word("wurld", 130.0, 100.0, 41.0),
    ]);
    session
        .apply(|doc| apply_layers(doc, &[(0, read.clone())]), show)
        .unwrap();
    let doc = session.read_document().unwrap();
    let objects = letters_edit::page_objects(&doc, 0).unwrap();
    let on_page = ocr_page_from_objects(&objects);
    let texts: Vec<_> = on_page
        .words
        .iter()
        .map(|word| word.text.as_str())
        .collect();
    assert_eq!(
        texts,
        vec!["Hello", "wurld"],
        "the invisible words read back from the page"
    );

    let overlay = overlay_words(0, Some(&read), &objects);
    assert!(overlay.with_confidence);
    assert_eq!(overlay.words[1].confidence, Some(41.0));
    // without tesseract's words (a file opened later) the page's own are shown
    let plain = overlay_words(0, None, &objects);
    assert!(!plain.with_confidence);
    assert_eq!(plain.words.len(), 2);
    assert!(
        (plain.words[0].rect.x - 72.0).abs() < 3.0,
        "{:?}",
        plain.words[0].rect
    );
    // "Hello" was read 40 points wide; the layer stretches it to that width
    assert!(
        (plain.words[0].rect.width - 40.0).abs() < 2.0,
        "{:?}",
        plain.words[0].rect
    );

    // correcting a word writes the page's layer again, as one more step
    let corrected = correct_word(&words_to_correct(Some(&read), &objects), 1, "world").unwrap();
    session
        .apply(|doc| apply_layers(doc, &[(0, corrected.clone())]), show)
        .unwrap();
    let doc = session.read_document().unwrap();
    let objects = letters_edit::page_objects(&doc, 0).unwrap();
    let again = overlay_words(0, Some(&corrected), &objects);
    assert_eq!(again.words[1].text, "world");
    assert_eq!(again.words[1].confidence, Some(100.0));
    // tesseract's old words no longer match, so they aren't shown
    assert!(!overlay_words(0, Some(&read), &objects).with_confidence);
}

#[test]
fn a_correction_must_be_a_word() {
    let page = page_of(vec![word("Hello", 72.0, 72.0, 90.0)]);
    assert!(correct_word(&page, 0, "  ").is_err());
    assert!(correct_word(&page, 0, "two\nlines").is_err());
    assert!(correct_word(&page, 3, "Hi").is_err());
    let fixed = correct_word(&page, 0, " Hallo ").unwrap();
    assert_eq!(fixed.words[0].text, "Hallo");
}

#[test]
fn making_text_editable_replaces_the_hidden_layer_with_visible_text() {
    use letters_edit::{page_objects, ScanMode, ScanOptions};

    let mut session = scanned_session("ocr-make-editable", 2);
    let read = |page: u32| {
        page_of(vec![
            word("Hello", 72.0, 72.0, 95.0),
            word(&format!("page{page}"), 130.0, 72.0, 40.0),
        ])
    };
    let pages = vec![(0, read(0)), (1, read(1))];
    // first the hidden layer of Recognize Text, then Make Editable Text over it
    session
        .apply(|doc| apply_layers(doc, &pages), show)
        .unwrap();
    let options = ScanOptions {
        mode: ScanMode::TextOverCleanedImage,
        ..ScanOptions::default()
    };
    let mut summary = None;
    session
        .apply(
            |doc| {
                summary = Some(super::editable::make_pages_editable(doc, &pages, &options)?);
                Ok::<(), String>(())
            },
            show,
        )
        .unwrap();
    let summary = summary.unwrap();
    assert_eq!((summary.pages, summary.lines), (2, 2));
    assert_eq!(
        summary.doubtful_lines, 2,
        "a 40% word makes its line doubtful"
    );
    let doc = session.read_document().unwrap();
    let objects = page_objects(&doc, 1).unwrap();
    let runs: Vec<_> = objects
        .text_blocks
        .iter()
        .flat_map(|block| &block.lines)
        .flat_map(|line| &line.runs)
        .collect();
    assert!(
        runs.iter().all(|run| !run.invisible),
        "no hidden layer is left"
    );
    let text: String = runs.iter().map(|run| run.text.as_str()).collect();
    assert_eq!(
        text.matches("Hello").count(),
        1,
        "the words are there once: {text}"
    );
    assert!(session.can_undo());

    // a second Make Editable Text on the same pages is refused with a
    // sentence and changes nothing, rather than writing the lines twice
    let revision = session.revision();
    let again = session.apply(
        |doc| super::editable::make_pages_editable(doc, &pages, &options).map(|_| ()),
        show,
    );
    let message = again.unwrap_err().to_string();
    assert!(
        message.contains("Page 1 already has editable text"),
        "{message}"
    );
    assert_eq!(session.revision(), revision);
}

#[test]
fn a_page_that_cannot_be_made_editable_changes_nothing() {
    use letters_edit::ScanOptions;

    let mut session = scanned_session("ocr-make-editable-refused", 2);
    let revision = session.revision();
    let pages = vec![
        (0, page_of(vec![word("Fine", 72.0, 72.0, 90.0)])),
        (1, page_of(Vec::new())),
    ];
    let result = session.apply(
        |doc| {
            super::editable::make_pages_editable(doc, &pages, &ScanOptions::default()).map(|_| ())
        },
        show,
    );
    let error = result.unwrap_err().to_string();
    assert!(error.contains("Page 2"), "{error}");
    assert_eq!(session.revision(), revision, "nothing changed");
}
