//! the invisible text layer, checked through our own pdf engine: after
//! recognition, searching the pdf must find the words where they are on
//! the page, copying must give the right characters, and the page must
//! look exactly as it did.

mod support;

use std::path::Path;

use letters_ocr::{
    add_text_layer, has_text_layer, images_to_pdf, is_scanned, page_makeup, recognize, Baseline,
    CancelFlag, ImageSource, ImagesToPdfOptions, OcrOptions, OcrPage, OcrWord, PageImage, Point,
    Rect,
};
use letters_pdf::{SearchResults, TextRect};
use lopdf::{Document, Object};
use support::{
    engine, page_tiles, png_bytes, render_file, start_renderer, temp_dir, write_text_pdf, Font,
    TextLine, TextPage,
};

const LINES: [&str; 4] = [
    "Photosynthesis is the process by which green plants",
    "use sunlight to make food. It takes place inside tiny",
    "structures called chloroplasts. The energy of light",
    "splits water molecules and releases oxygen.",
];

fn text_page(lines: &[&str], size: f32) -> TextPage {
    TextPage {
        lines: lines
            .iter()
            .enumerate()
            .map(|(index, text)| TextLine::new(72.0, 150.0 + index as f32 * size * 1.6, size, text))
            .collect(),
        font: Font::Helvetica,
        tilt_degrees: 0.0,
    }
}

/// a scan of some text: the text drawn by our renderer at 300 dpi, then
/// made into a pdf whose page is just that picture.
struct Scan {
    picture: PageImage,
    pdf: Document,
}

fn scan_of(dir: &Path, page: TextPage) -> Scan {
    let source = dir.join("source.pdf");
    write_text_pdf(&source, &[page]);
    let picture = render_file(&source, 0, 300.0);
    let png = png_bytes(&picture.pixels, picture.dpi);
    let pdf = images_to_pdf(
        &[ImageSource::Bytes(png)],
        &ImagesToPdfOptions::default(),
        None,
        &mut |_| {},
        &CancelFlag::new(),
    )
    .unwrap();
    Scan { picture, pdf }
}

fn save(doc: &mut Document, path: &Path) {
    doc.save(path).unwrap();
}

fn search(path: &Path, text: &str) -> SearchResults {
    let worker = start_renderer();
    let info = worker.open(path, None).unwrap();
    let results = worker.search(info.id, text).unwrap();
    worker.close(info.id);
    results
}

fn word<'a>(page: &'a OcrPage, text: &str) -> &'a OcrWord {
    page.words
        .iter()
        .find(|word| word.text.trim_end_matches(['.', ',']) == text)
        .unwrap_or_else(|| panic!("{text} was not recognised"))
}

/// searches for a recognised word and checks the match lies on it. the
/// word must have been read as `text` exactly, without punctuation, so the
/// match covers all of it.
fn assert_found_on_word(path: &Path, page: &OcrPage, text: &str) {
    let found = search(path, text);
    assert_eq!(found.matches.len(), 1, "{text}");
    let word = word(page, text);
    assert_eq!(word.text, text);
    assert_lands_on(&found.matches[0].rects[0], &word.rect, 1.5);
}

/// the found rectangle must cover the recognised word's box.
fn assert_lands_on(found: &TextRect, expected: &Rect, tolerance: f32) {
    let close = |a: f32, b: f32| (a - b).abs() <= tolerance;
    assert!(
        close(found.x, expected.x) && close(found.x + found.width, expected.right()),
        "found across {}..{}, word is across {}..{}",
        found.x,
        found.x + found.width,
        expected.x,
        expected.right()
    );
    // the found box is the font's full height, the word's box only its
    // ink, so only ask that they overlap on the same line
    let middle = expected.center().y;
    assert!(
        found.y < middle && found.y + found.height > middle,
        "found down {}..{}, word's middle at {middle}",
        found.y,
        found.y + found.height
    );
}

#[test]
fn a_scan_becomes_searchable_and_looks_the_same() {
    let dir = temp_dir("searchable");
    let mut scan = scan_of(&dir, text_page(&LINES, 14.0));
    let plain = dir.join("plain.pdf");
    save(&mut scan.pdf, &plain);
    assert!(is_scanned(&scan.pdf, 0).unwrap());

    let words = recognize(
        &engine(),
        &scan.picture,
        &OcrOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap();
    add_text_layer(&mut scan.pdf, 0, &words).unwrap();
    let searchable = dir.join("searchable.pdf");
    save(&mut scan.pdf, &searchable);

    // words can be found, at the place they are in the picture
    for text in ["Photosynthesis", "sunlight", "structures", "molecules"] {
        assert_found_on_word(&searchable, &words, text);
    }

    // phrases across words and lines are found, so spaces and line
    // breaks come out where they should when text is copied
    assert_eq!(search(&searchable, "the energy of light").matches.len(), 1);
    assert_eq!(search(&searchable, "light splits water").matches.len(), 1);
    let snippet = &search(&searchable, "chloroplasts").matches[0].snippet;
    assert!(
        snippet.contains("structures called chloroplasts. The energy"),
        "{snippet}"
    );

    // the layer is invisible: every pixel is as it was
    assert_eq!(
        page_tiles(&plain, 0, 2000),
        page_tiles(&searchable, 0, 2000)
    );

    // the page still counts as a scan, one that has been read
    let makeup = page_makeup(&scan.pdf, 0).unwrap();
    assert!(makeup.is_scanned() && makeup.has_ocr_layer && makeup.invisible_characters > 100);
}

#[test]
fn reading_again_replaces_the_layer() {
    let dir = temp_dir("again");
    let mut scan = scan_of(&dir, text_page(&LINES, 14.0));
    let words = recognize(
        &engine(),
        &scan.picture,
        &OcrOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap();
    add_text_layer(&mut scan.pdf, 0, &words).unwrap();
    add_text_layer(&mut scan.pdf, 0, &words).unwrap();
    let path = dir.join("twice.pdf");
    save(&mut scan.pdf, &path);
    // once, not twice
    assert_eq!(search(&path, "chloroplasts").matches.len(), 1);
    // and it survives saving and opening again
    let mut reopened = Document::load(&path).unwrap();
    assert!(has_text_layer(&reopened, 0).unwrap());
    add_text_layer(&mut reopened, 0, &words).unwrap();
    let path = dir.join("three-times.pdf");
    save(&mut reopened, &path);
    assert_eq!(search(&path, "chloroplasts").matches.len(), 1);
}

#[test]
fn images_to_pdf_can_read_each_page() {
    let dir = temp_dir("images-to-pdf");
    let first = scan_of(&dir, text_page(&LINES[..2], 14.0));
    let second = scan_of(&dir, text_page(&LINES[2..], 14.0));
    let images = [
        ImageSource::Bytes(png_bytes(&first.picture.pixels, 300.0)),
        ImageSource::Bytes(png_bytes(&second.picture.pixels, 300.0)),
    ];
    let mut done = Vec::new();
    let mut doc = images_to_pdf(
        &images,
        &ImagesToPdfOptions::default(),
        Some((&engine(), &OcrOptions::default())),
        &mut |progress| done.push(progress.done),
        &CancelFlag::new(),
    )
    .unwrap();
    assert_eq!(done, [1, 2]);
    let path = dir.join("made.pdf");
    save(&mut doc, &path);
    let results = search(&path, "chloroplasts");
    assert_eq!(results.matches.len(), 1);
    assert_eq!(results.matches[0].page, 1);
    assert_eq!(search(&path, "photosynthesis").matches[0].page, 0);
}

/// one page each of spanish, french and german, each read in its own
/// language as a person would choose it.
#[test]
fn accented_text_copies_correctly() {
    let samples = [
        (
            "spa",
            "El niño comió una manzana el año pasado.",
            &["niño", "comió", "año"][..],
        ),
        (
            "fra",
            "Le garçon a vu un château près de la forêt.",
            &["garçon", "château", "près"][..],
        ),
        (
            "deu",
            "Die Straße führt über die Brücke zum Fluss.",
            &["Straße", "führt", "über", "Brücke"][..],
        ),
    ];
    for (language, line, accented) in samples {
        let dir = temp_dir(&format!("accents-{language}"));
        let mut scan = scan_of(&dir, text_page(&[line], 16.0));
        let options = OcrOptions {
            languages: vec![language.into()],
            ..OcrOptions::default()
        };
        let words = recognize(&engine(), &scan.picture, &options, &CancelFlag::new()).unwrap();
        let read = words.plain_text();
        assert_eq!(read, line, "{language} was misread");
        add_text_layer(&mut scan.pdf, 0, &words).unwrap();
        let path = dir.join("accents.pdf");
        save(&mut scan.pdf, &path);
        for wanted in accented {
            assert_found_on_word(&path, &words, wanted);
        }
        // copying the line gives it back exactly, spaces and all
        let without_stop = line.trim_end_matches('.');
        let found = search(&path, without_stop);
        assert_eq!(
            found.matches.len(),
            1,
            "{language}: the whole line wasn't found"
        );
        assert_eq!(found.matches[0].snippet, line);
    }
}

fn hand_made_word(text: &str, x: f32, baseline: f32, width: f32, size: f32) -> OcrWord {
    OcrWord {
        text: text.into(),
        rect: Rect::new(x, baseline - size * 0.7, width, size * 0.7),
        baseline: Baseline {
            start: Point::new(x, baseline),
            end: Point::new(x + width, baseline),
        },
        font_size: size,
        confidence: 90.0,
        line: 0,
        paragraph: 0,
        block: 0,
        language: None,
    }
}

#[test]
fn any_script_round_trips_through_the_glyphless_font() {
    // no recognition here: the words are made up, to test the writing
    let dir = temp_dir("any-script");
    let blank = dir.join("blank.pdf");
    write_text_pdf(&blank, &[text_page(&[], 12.0)]);
    let mut doc = Document::load(&blank).unwrap();
    let words = OcrPage {
        width: 612.0,
        height: 792.0,
        words: vec![
            hand_made_word("łódź", 72.0, 100.0, 60.0, 14.0),
            hand_made_word("Ωμέγα", 200.0, 100.0, 80.0, 14.0),
            hand_made_word("Привет", 72.0, 200.0, 90.0, 14.0),
            hand_made_word("café", 300.0, 200.0, 50.0, 14.0),
        ],
        language: "pol".into(),
        skew_angle: 0.0,
        orientation: 0,
        mean_confidence: 90.0,
        steps: Vec::new(),
    };
    add_text_layer(&mut doc, 0, &words).unwrap();
    let path = dir.join("any-script.pdf");
    save(&mut doc, &path);
    for word in &words.words {
        let found = search(&path, &word.text);
        assert_eq!(found.matches.len(), 1, "{} not found", word.text);
        // helvetica letters sit a little inside their width, so "café"
        // is found up to a point inside its box
        assert_lands_on(&found.matches[0].rects[0], &word.rect, 1.5);
    }
}

#[test]
fn turned_and_cropped_pages_get_the_layer_in_the_right_place() {
    let dir = temp_dir("turned-cropped");
    let source = dir.join("source.pdf");
    write_text_pdf(&source, &[text_page(&LINES, 14.0)]);
    let upright = render_file(&source, 0, 300.0);
    // the scanner stored the page sideways (turned anticlockwise) and the
    // pdf turns it back for showing with /Rotate 90
    let stored = image::imageops::rotate270(&upright.pixels);
    let mut doc = images_to_pdf(
        &[ImageSource::Bytes(png_bytes(&stored, 300.0))],
        &ImagesToPdfOptions::default(),
        None,
        &mut |_| {},
        &CancelFlag::new(),
    )
    .unwrap();
    let page_id = *doc.get_pages().values().next().unwrap();
    let page = doc.get_dictionary_mut(page_id).unwrap();
    page.set("Rotate", 90);
    // and someone cropped half an inch off every side
    let media: Vec<f32> = page
        .get(b"MediaBox")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_float().unwrap())
        .collect();
    let crop: Vec<Object> = [
        media[0] + 36.0,
        media[1] + 36.0,
        media[2] - 36.0,
        media[3] - 36.0,
    ]
    .into_iter()
    .map(Object::from)
    .collect();
    page.set("CropBox", crop);
    let prepared = dir.join("prepared.pdf");
    save(&mut doc, &prepared);

    // read the page as shown: upright and cropped
    let shown = render_file(&prepared, 0, 300.0);
    let words = recognize(
        &engine(),
        &shown,
        &OcrOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap();
    assert_eq!(words.orientation, 0, "the shown page is upright");
    add_text_layer(&mut doc, 0, &words).unwrap();
    let path = dir.join("turned-cropped.pdf");
    save(&mut doc, &path);

    for text in ["structures", "sunlight", "molecules"] {
        assert_found_on_word(&path, &words, text);
    }
    assert_eq!(page_tiles(&prepared, 0, 1500), page_tiles(&path, 0, 1500));
}

#[test]
fn a_page_of_real_text_is_not_a_scan() {
    let dir = temp_dir("not-scan");
    let path = dir.join("text.pdf");
    write_text_pdf(&path, &[text_page(&LINES, 12.0)]);
    let doc = Document::load(&path).unwrap();
    assert!(!is_scanned(&doc, 0).unwrap());
}
