//! end to end recognition: pages drawn by our own renderer from text we
//! know, read back by tesseract through the whole pipeline.

mod support;

use std::time::{Duration, Instant};

use letters_ocr::{
    recognize, recognize_pages, AppliedStep, CancelFlag, Engine, OcrError, OcrOptions, OcrPage,
    PageImage, PreprocessOptions,
};
use support::{
    add_noise, character_accuracy, engine, render_file, temp_dir, write_text_pdf, Font, TextLine,
    TextPage, PAGE_HEIGHT, PAGE_WIDTH,
};

/// clean pages must be read at least this well.
const CLEAN_ACCURACY: f32 = 0.98;
/// crooked or noisy pages at least this well.
const ROUGH_ACCURACY: f32 = 0.95;

const PARAGRAPH: [&str; 8] = [
    "Photosynthesis is the process by which green plants",
    "use sunlight to make food from carbon dioxide and",
    "water. It takes place mainly in the leaves, inside",
    "tiny structures called chloroplasts. The energy of",
    "light splits water molecules and releases oxygen,",
    "which the plant gives off into the air. The sugar it",
    "makes is used for growth or stored as starch for",
    "later, in roots, seeds and fruit.",
];

fn paragraph_lines(font_size: f32) -> Vec<TextLine> {
    PARAGRAPH
        .iter()
        .enumerate()
        .map(|(index, text)| {
            TextLine::new(
                72.0,
                150.0 + index as f32 * font_size * 1.6,
                font_size,
                text,
            )
        })
        .collect()
}

fn paragraph_text() -> String {
    PARAGRAPH.join(" ")
}

/// draws a page of known text at 300 dpi.
fn page_picture(test: &str, lines: Vec<TextLine>, font: Font, tilt_degrees: f32) -> PageImage {
    let dir = temp_dir(test);
    let path = dir.join("page.pdf");
    write_text_pdf(
        &path,
        &[TextPage {
            lines,
            font,
            tilt_degrees,
        }],
    );
    render_file(&path, 0, 300.0)
}

fn words_text(page: &OcrPage) -> String {
    page.words
        .iter()
        .map(|word| word.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn clean_text_is_read_accurately() {
    let picture = page_picture("clean", paragraph_lines(12.0), Font::Helvetica, 0.0);
    let started = Instant::now();
    let page = recognize(
        &engine(),
        &picture,
        &OcrOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap();
    let elapsed = started.elapsed();
    let accuracy = character_accuracy(&paragraph_text(), &words_text(&page));
    eprintln!(
        "clean 300 dpi page: accuracy {:.4}, mean confidence {:.1}, {} words, {:?}",
        accuracy,
        page.mean_confidence,
        page.words.len(),
        elapsed
    );
    assert!(
        accuracy >= CLEAN_ACCURACY,
        "accuracy {accuracy}: {}",
        words_text(&page)
    );
    assert!(page.mean_confidence > 80.0);
    // the renderer rounds the page up to whole pixels`n    assert!((page.width - PAGE_WIDTH).abs() < 0.5 && (page.height - PAGE_HEIGHT).abs() < 0.5);
    assert_eq!(page.orientation, 0);
    assert!(page.skew_angle.abs() < 0.1, "skew {}", page.skew_angle);
    // eight lines, read in order (tesseract may see one paragraph or two)
    let text = page.plain_text();
    assert_eq!(
        text.lines().filter(|line| !line.is_empty()).count(),
        8,
        "{text}"
    );
    assert_eq!(page.words.last().unwrap().line, 7);
}

/// a full page of body text: 36 lines of 11 point type. prints how long
/// each stage takes, for the numbers in README.md (run with --nocapture,
/// one test at a time, for undisturbed timings).
#[test]
fn a_full_page_is_read_in_reasonable_time() {
    let lines: Vec<TextLine> = (0..36)
        .map(|index| TextLine::new(72.0, 90.0 + index as f32 * 17.6, 11.0, PARAGRAPH[index % 8]))
        .collect();
    let expected: Vec<&str> = (0..36).map(|index| PARAGRAPH[index % 8]).collect();
    let picture = page_picture("full-page", lines, Font::Helvetica, 0.0);
    let engine = engine();

    let started = Instant::now();
    let prepared = letters_ocr::preprocess::prepare(
        &picture.pixels,
        picture.dpi,
        &PreprocessOptions::default(),
        0,
    );
    let cleanup = started.elapsed();
    drop(prepared);

    let without_orientation = OcrOptions {
        preprocess: PreprocessOptions {
            orientation: false,
            ..PreprocessOptions::default()
        },
        ..OcrOptions::default()
    };
    let started = Instant::now();
    let page = recognize(&engine, &picture, &without_orientation, &CancelFlag::new()).unwrap();
    let without = started.elapsed();
    let started = Instant::now();
    let with_page = recognize(
        &engine,
        &picture,
        &OcrOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap();
    let with = started.elapsed();

    let accuracy = character_accuracy(&expected.join(" "), &words_text(&page));
    eprintln!(
        "full page ({} words, {}x{} pixels): cleanup {cleanup:?}, read without orientation {without:?}, \
         with orientation {with:?}, accuracy {accuracy:.4}",
        page.words.len(),
        picture.pixels.width(),
        picture.pixels.height()
    );
    assert!(accuracy >= CLEAN_ACCURACY, "accuracy {accuracy}");
    assert_eq!(with_page.words.len(), page.words.len());
    assert!(with < Duration::from_secs(60), "took {with:?}");
}

#[test]
fn word_boxes_land_on_the_words() {
    // courier: every letter is 0.6 of the font size wide, so at 20 points
    // each letter takes 12 points and the word places are known exactly
    let lines = vec![
        TextLine::new(72.0, 200.0, 20.0, "alpha beta gamma"),
        TextLine::new(144.0, 400.0, 20.0, "delta epsilon"),
    ];
    let picture = page_picture("boxes", lines, Font::Courier, 0.0);
    let page = recognize(
        &engine(),
        &picture,
        &OcrOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap();
    let expected = [
        ("alpha", 72.0, 200.0),
        ("beta", 72.0 + 6.0 * 12.0, 200.0),
        ("gamma", 72.0 + 11.0 * 12.0, 200.0),
        ("delta", 144.0, 400.0),
        ("epsilon", 144.0 + 6.0 * 12.0, 400.0),
    ];
    for (text, left, baseline) in expected {
        let word = page
            .words
            .iter()
            .find(|word| word.text == text)
            .unwrap_or_else(|| panic!("{text} not found in {}", words_text(&page)));
        let right = left + text.len() as f32 * 12.0;
        // letters sit a little inside their advance width
        assert!(
            (word.rect.x - left).abs() < 3.0,
            "{text} starts at {}",
            word.rect.x
        );
        assert!(
            (word.rect.right() - right).abs() < 3.0,
            "{text} ends at {}",
            word.rect.right()
        );
        assert!(
            (word.baseline.start.y - baseline).abs() < 1.5,
            "{text} baseline {}",
            word.baseline.start.y
        );
        assert!(
            word.rect.y > baseline - 20.0 && word.rect.y < baseline - 8.0,
            "{text} top {}",
            word.rect.y
        );
        // "epsilon" has a descender (p), so its box goes below the baseline
        if text == "epsilon" {
            assert!(word.rect.bottom() > baseline + 2.0);
        }
        assert!(
            (word.font_size - 20.0).abs() < 5.0,
            "{text} size {}",
            word.font_size
        );
    }
}

#[test]
fn crooked_pages_are_straightened_and_still_read() {
    for tilt in [-7.0f32, -3.0, -1.0, 1.0, 3.0, 7.0] {
        let picture = page_picture(
            &format!("tilt{tilt}"),
            paragraph_lines(12.0),
            Font::Helvetica,
            tilt,
        );
        let page = recognize(
            &engine(),
            &picture,
            &OcrOptions::default(),
            &CancelFlag::new(),
        )
        .unwrap();
        let accuracy = character_accuracy(&paragraph_text(), &words_text(&page));
        eprintln!(
            "tilt {tilt}: found {:.2} degrees, accuracy {accuracy:.4}",
            page.skew_angle
        );
        assert!(
            (page.skew_angle - tilt).abs() <= 0.2,
            "made {tilt}, found {}",
            page.skew_angle
        );
        assert!(page
            .steps
            .iter()
            .any(|step| matches!(step, AppliedStep::Deskew { .. })));
        assert!(
            accuracy >= ROUGH_ACCURACY,
            "tilt {tilt}: accuracy {accuracy}"
        );

        // the first word's box must be where the tilted word really is:
        // its unturned start (72, 150) turned around the page centre
        let first = &page.words[0];
        let (sin, cos) = tilt.to_radians().sin_cos();
        let (dx, dy) = (72.0 - PAGE_WIDTH / 2.0, 150.0 - PAGE_HEIGHT / 2.0);
        // anticlockwise on a screen, where y grows downward
        let expected_x = PAGE_WIDTH / 2.0 + dx * cos + dy * sin;
        let expected_y = PAGE_HEIGHT / 2.0 - dx * sin + dy * cos;
        let start = first.baseline.start;
        assert!(
            (start.x - expected_x).abs() < 4.0 && (start.y - expected_y).abs() < 4.0,
            "tilt {tilt}: baseline starts at {start:?}, expected ({expected_x}, {expected_y})"
        );
        // the baseline slopes with the text
        let end = first.baseline.end;
        let slope = (-(end.y - start.y)).atan2(end.x - start.x).to_degrees();
        assert!(
            (slope - tilt).abs() < 1.0,
            "tilt {tilt}: baseline slope {slope}"
        );
    }
}

#[test]
fn noise_does_not_break_recognition() {
    let mut picture = page_picture("noise", paragraph_lines(12.0), Font::Helvetica, 0.0);
    // grain of up to 40 gray levels, and 2 percent of pixels turned to dust
    add_noise(&mut picture.pixels, 40.0, 0.02, 7);
    let page = recognize(
        &engine(),
        &picture,
        &OcrOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap();
    let accuracy = character_accuracy(&paragraph_text(), &words_text(&page));
    eprintln!("noisy page: accuracy {accuracy:.4}, steps {:?}", page.steps);
    assert!(
        page.steps.contains(&AppliedStep::Median),
        "{:?}",
        page.steps
    );
    assert!(
        accuracy >= ROUGH_ACCURACY,
        "accuracy {accuracy}: {}",
        words_text(&page)
    );
}

#[test]
fn without_cleanup_noise_hurts_more() {
    // shows the cleanup earns its keep: the same dusty page read raw
    let mut picture = page_picture("noise-raw", paragraph_lines(12.0), Font::Helvetica, 0.0);
    add_noise(&mut picture.pixels, 40.0, 0.02, 7);
    let options = OcrOptions {
        preprocess: PreprocessOptions::none(),
        ..OcrOptions::default()
    };
    let raw = recognize(&engine(), &picture, &options, &CancelFlag::new()).unwrap();
    let cleaned = recognize(
        &engine(),
        &picture,
        &OcrOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap();
    let raw_accuracy = character_accuracy(&paragraph_text(), &words_text(&raw));
    let cleaned_accuracy = character_accuracy(&paragraph_text(), &words_text(&cleaned));
    eprintln!("noisy page: raw {raw_accuracy:.4}, cleaned {cleaned_accuracy:.4}");
    assert!(cleaned_accuracy >= raw_accuracy);
}

#[test]
fn a_sideways_page_is_turned_upright() {
    let picture = page_picture("sideways", paragraph_lines(12.0), Font::Helvetica, 0.0);
    // the scan came out turned a quarter anticlockwise, so it needs one
    // quarter turn clockwise to be read
    let sideways = PageImage::from_gray(image::imageops::rotate270(&picture.pixels), picture.dpi);
    let page = recognize(
        &engine(),
        &sideways,
        &OcrOptions::default(),
        &CancelFlag::new(),
    )
    .unwrap();
    let accuracy = character_accuracy(&paragraph_text(), &words_text(&page));
    eprintln!(
        "sideways page: orientation {}, accuracy {accuracy:.4}",
        page.orientation
    );
    assert_eq!(page.orientation, 90);
    assert!(accuracy >= CLEAN_ACCURACY, "accuracy {accuracy}");
    // positions are on the sideways picture: the first line runs up its
    // left side, so the first word is near the bottom left
    assert!((page.width - PAGE_HEIGHT).abs() < 0.5 && (page.height - PAGE_WIDTH).abs() < 0.5);
    let first = &page.words[0];
    assert!(
        first.rect.x < 200.0 && first.rect.y > 400.0,
        "{:?}",
        first.rect
    );
    assert!(
        first.rect.height > first.rect.width,
        "a sideways word is tall: {:?}",
        first.rect
    );
}

#[test]
fn a_missing_language_is_named() {
    let picture = PageImage::from_gray(
        image::GrayImage::from_pixel(100, 100, image::Luma([255])),
        300.0,
    );
    let options = OcrOptions {
        languages: vec!["eng".into(), "kor".into()],
        ..OcrOptions::default()
    };
    let error = recognize(&engine(), &picture, &options, &CancelFlag::new()).unwrap_err();
    assert_eq!(
        error,
        OcrError::MissingLanguage {
            code: "kor".into(),
            name: "Korean".into()
        }
    );
    let message = error.to_string();
    assert!(message.contains("Korean"), "{message}");
    assert!(message.contains("Install Letters again"), "{message}");
}

#[test]
fn the_bundled_languages_are_installed() {
    let engine = engine();
    let installed = engine.installed_languages();
    for code in letters_ocr::BUNDLED_LANGUAGES {
        assert!(
            installed.contains(&code.to_owned()),
            "{code} missing from {installed:?}"
        );
    }
    assert!(engine.can_detect_orientation());
    assert!(engine.version().unwrap().starts_with("tesseract 5"));
}

#[test]
fn a_missing_engine_says_so() {
    let dir = temp_dir("no-engine");
    let error = Engine::locate(&dir).unwrap_err();
    assert!(matches!(error, OcrError::EngineMissing(_)));
    assert!(error.to_string().contains("Install Letters again"));
}

#[test]
fn many_pages_report_progress_and_stop_when_cancelled() {
    let picture = page_picture("pages", paragraph_lines(12.0), Font::Helvetica, 0.0);
    let cancel = CancelFlag::new();
    let mut loaded = Vec::new();
    let mut reports = Vec::new();
    let result = recognize_pages(
        &engine(),
        3,
        |index| {
            loaded.push(index);
            Ok(picture.clone())
        },
        &OcrOptions::default(),
        &mut |progress| {
            reports.push(progress);
            // the person presses cancel after the first page
            cancel.cancel();
        },
        &cancel,
    );
    assert_eq!(result.unwrap_err(), OcrError::Cancelled);
    assert_eq!(loaded, [0], "no page is drawn after cancelling");
    assert_eq!(reports.len(), 1);
    assert_eq!((reports[0].done, reports[0].total), (1, 3));
}

#[test]
fn cancelling_stops_a_page_in_the_middle() {
    let picture = page_picture("mid-page", paragraph_lines(12.0), Font::Helvetica, 0.0);
    let cancel = CancelFlag::new();
    let flag = cancel.clone();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        flag.cancel();
    });
    let started = Instant::now();
    let result = recognize(&engine(), &picture, &OcrOptions::default(), &cancel);
    canceller.join().unwrap();
    assert_eq!(result.unwrap_err(), OcrError::Cancelled);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn all_pages_are_read_with_progress() {
    let picture = page_picture("two-pages", paragraph_lines(12.0), Font::Helvetica, 0.0);
    let mut reports = Vec::new();
    let pages = recognize_pages(
        &engine(),
        2,
        |_| Ok(picture.clone()),
        &OcrOptions::default(),
        &mut |progress| reports.push(progress.done),
        &CancelFlag::new(),
    )
    .unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!(reports, [1, 2]);
}
