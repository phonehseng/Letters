//! pdf to plain text, in reading order and in layout mode, from pdfs
//! with known text.

mod support;

use letters_convert::*;
use support::*;

fn text_of(pages: &[TestPage], options: &TextExportOptions, name: &str) -> String {
    let dir = temp_dir(name);
    let path = dir.join("text.pdf");
    write_pdf(&path, pages);
    let source = PdfSource::open(&path).unwrap();
    pdf_to_text(&source, options, &mut |_| {}, &CancelFlag::new()).unwrap()
}

/// lines of 11 point helvetica, 13.2 points apart, starting at `top`.
fn lines(page: &mut TestPage, x: f32, top: f32, texts: &[&str]) -> f32 {
    let mut baseline = top;
    for text in texts {
        page.text(Font::Helvetica, 11.0, x, baseline, text);
        baseline += 13.2;
    }
    baseline
}

#[test]
fn single_column_paragraphs_in_reading_order() {
    let mut page = TestPage::letter();
    page.text(Font::HelveticaBold, 18.0, 72.0, 80.0, "Photosynthesis");
    let end = lines(
        &mut page,
        72.0,
        110.0,
        &[
            "Plants turn light into chemical energy that they store",
            "in sugars. The process takes place in the chloroplasts",
            "of the leaf.",
        ],
    );
    lines(
        &mut page,
        72.0,
        end + 10.0,
        &[
            "Water and carbon dioxide go in, and oxygen comes out",
            "as a by-product of the reaction.",
        ],
    );
    let text = text_of(&[page], &TextExportOptions::default(), "single");
    assert_eq!(
        text,
        "Photosynthesis\n\n\
         Plants turn light into chemical energy that they store in sugars. The process takes place in the chloroplasts of the leaf.\n\n\
         Water and carbon dioxide go in, and oxygen comes out as a by-product of the reaction.\n"
    );
}

fn two_column_page() -> TestPage {
    let mut page = TestPage::letter();
    page.text(Font::HelveticaBold, 16.0, 72.0, 80.0, "Two Column Report");
    lines(
        &mut page,
        72.0,
        110.0,
        &[
            "The left column starts here",
            "and carries on down the page",
            "until it reaches its end.",
        ],
    );
    lines(
        &mut page,
        324.0,
        110.0,
        &[
            "The right column is read",
            "only after the left column",
            "has been read to the end.",
        ],
    );
    page
}

#[test]
fn two_columns_are_read_one_after_the_other() {
    let text = text_of(
        &[two_column_page()],
        &TextExportOptions::default(),
        "columns",
    );
    assert_eq!(
        text,
        "Two Column Report\n\n\
         The left column starts here and carries on down the page until it reaches its end.\n\n\
         The right column is read only after the left column has been read to the end.\n"
    );
}

fn hyphenated_page() -> TestPage {
    let mut page = TestPage::letter();
    lines(
        &mut page,
        72.0,
        100.0,
        &[
            "Every document goes through a conver-",
            "sion step before it is shared with the",
            "class, which keeps the work well organised.",
        ],
    );
    page
}

#[test]
fn hyphens_at_line_ends_are_joined_when_asked() {
    let joined = text_of(
        &[hyphenated_page()],
        &TextExportOptions::default(),
        "hyphen-on",
    );
    assert_eq!(
        joined,
        "Every document goes through a conversion step before it is shared with the class, which keeps the work well organised.\n"
    );
    let options = TextExportOptions {
        dehyphenate: false,
        ..Default::default()
    };
    let kept = text_of(&[hyphenated_page()], &options, "hyphen-off");
    assert!(kept.contains("a conver-sion step"), "{kept}");
}

#[test]
fn pages_can_be_separated() {
    let mut first = TestPage::letter();
    first.text(Font::Helvetica, 11.0, 72.0, 100.0, "First page.");
    let mut second = TestPage::letter();
    second.text(Font::Helvetica, 11.0, 72.0, 100.0, "Second page.");
    let options = TextExportOptions {
        page_separator: PageSeparator::FormFeed,
        ..Default::default()
    };
    let text = text_of(&[first, second], &options, "separators");
    assert_eq!(text, "First page.\n\u{c}\nSecond page.\n");
}

#[test]
fn layout_mode_keeps_positions() {
    let mut page = TestPage::letter();
    page.text(Font::Courier, 10.0, 72.0, 100.0, "Name");
    page.text(Font::Courier, 10.0, 192.0, 100.0, "Score");
    page.text(Font::Courier, 10.0, 72.0, 112.0, "Ana");
    page.text(Font::Courier, 10.0, 192.0, 112.0, "9");
    page.text(Font::Courier, 10.0, 102.0, 148.0, "indented note");
    let options = TextExportOptions {
        mode: TextMode::Layout,
        ..Default::default()
    };
    // courier letters are 6 points wide: 120 points is 20 columns, 30 is 5
    let text = text_of(&[page], &options, "layout");
    assert_eq!(
        text,
        "Name                Score\nAna                 9\n\n\n     indented note\n"
    );
}

#[test]
fn headers_and_page_numbers_are_left_out_unless_asked() {
    let pages: Vec<TestPage> = (1..=3)
        .map(|number| {
            let mut page = TestPage::letter();
            page.text(Font::Helvetica, 9.0, 72.0, 40.0, "Lab Notebook");
            page.text(
                Font::Helvetica,
                11.0,
                72.0,
                200.0,
                &format!("Entry number {number}."),
            );
            page.text(Font::Helvetica, 9.0, 300.0, 760.0, &number.to_string());
            page
        })
        .collect();
    let without = text_of(&pages, &TextExportOptions::default(), "repeat-off");
    assert_eq!(
        without,
        "Entry number 1.\n\nEntry number 2.\n\nEntry number 3.\n"
    );
    let options = TextExportOptions {
        include_headers_footers: true,
        ..Default::default()
    };
    let with = text_of(&pages, &options, "repeat-on");
    assert!(
        with.starts_with("Lab Notebook\n\nEntry number 1.\n\n1"),
        "{with}"
    );
}

#[test]
fn cancelling_stops_text_export() {
    let dir = temp_dir("text-cancel");
    let path = dir.join("text.pdf");
    write_pdf(&path, &[hyphenated_page(), hyphenated_page()]);
    let source = PdfSource::open(&path).unwrap();
    let cancel = CancelFlag::new();
    cancel.cancel();
    assert_eq!(
        pdf_to_text(&source, &TextExportOptions::default(), &mut |_| {}, &cancel),
        Err(ConvertError::Cancelled)
    );
}
