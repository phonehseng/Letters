//! colour contrast, measured on pages drawn by pdfium.

mod support;

use letters_a11y::{check, contrast_ratio, FixKind, Rgb, RuleStatus};
use lopdf::Document;
use support::*;

/// a tagged page with one line of text in `grey` (0 to 255) at `size`
/// points, on white or on a filled background.
fn grey_text(grey: u8, size: f32, background: Option<u8>) -> Document {
    let level = grey as f32 / 255.0;
    let mut content = String::new();
    if let Some(back) = background {
        content.push_str(&artifact(&format!(
            "{} g 50 600 500 100 re f\n",
            back as f32 / 255.0
        )));
    }
    content.push_str(&marked(
        "H1",
        0,
        &format!("{level} g {}", text("F1", size, 72.0, 160.0, "Hello there")),
    ));
    content.push_str(&marked(
        "P",
        1,
        &format!("0 g {}", text("F1", 12.0, 72.0, 400.0, "Black text")),
    ));
    let mut builder = Builder::new();
    builder.page(&content);
    let pages = builder.pages.clone();
    let mut doc = builder.build();
    add_tags(
        &mut doc,
        &pages,
        vec![leaf("H1", 0, &[0]), leaf("P", 0, &[1])],
    );
    add_metadata(&mut doc, "en", "Contrast");
    doc
}

fn measured(doc: &Document, name: &str) -> letters_a11y::Report {
    let mut pdfium = Pdfium::open(doc, name);
    check(doc, Some(&mut pdfium)).unwrap()
}

#[test]
fn black_on_white_passes_and_is_counted_as_checked() {
    let report = measured(&grey_text(0, 12.0, None), "black");
    assert!(!report.has("low-contrast"));
    let rule = report
        .rules
        .iter()
        .find(|r| r.id == "low-contrast")
        .unwrap();
    assert_eq!(rule.status, RuleStatus::Passed);
}

#[test]
fn grey_777_on_white_fails_with_its_measured_ratio() {
    let report = measured(&grey_text(0x77, 12.0, None), "grey-777");
    let finding = report
        .with_id("low-contrast")
        .next()
        .expect("#777 on white should fail");
    let contrast = finding.contrast.unwrap();
    assert!((contrast.ratio - 4.48).abs() < 0.01, "{contrast:?}");
    assert_eq!(contrast.required, 4.5);
    assert_eq!(contrast.text_colour, Rgb::new(0x77, 0x77, 0x77));
    assert_eq!(contrast.background_colour, Rgb::WHITE);
    assert!(contrast_ratio(contrast.suggested_colour, Rgb::WHITE) >= 4.5);
    assert!(finding.details.contains("4.48 to 1"), "{}", finding.details);
    // only the grey text is reported, not the black paragraph
    assert_eq!(report.with_id("low-contrast").count(), 1);
}

#[test]
fn grey_767676_on_white_passes() {
    assert!(!measured(&grey_text(0x76, 12.0, None), "grey-767676").has("low-contrast"));
}

#[test]
fn large_text_needs_only_three_to_one() {
    // #949494 is about 3.03:1 on white: enough at 24 points, not at 12
    assert!(!measured(&grey_text(0x94, 24.0, None), "large-pass").has("low-contrast"));
    assert!(measured(&grey_text(0x94, 12.0, None), "small-fail").has("low-contrast"));
}

#[test]
fn text_on_a_dark_background_is_measured_against_that_background() {
    let report = measured(&grey_text(0x55, 12.0, Some(0x33)), "dark");
    let contrast = report
        .with_id("low-contrast")
        .next()
        .unwrap()
        .contrast
        .unwrap();
    assert_eq!(contrast.background_colour, Rgb::new(0x33, 0x33, 0x33));
    // the fix makes the text lighter, not darker
    assert!(contrast.suggested_colour.r > 0x55);
}

#[test]
fn recolouring_the_failing_text_fixes_it_and_nothing_else() {
    let mut doc = grey_text(0x99, 12.0, None);
    let report = measured(&doc, "recolour-before");
    let before = problems(&report);
    let Some(FixKind::ChangeTextColour { runs, suggested }) =
        report.with_id("low-contrast").next().unwrap().fix.clone()
    else {
        panic!("a colour fix was expected");
    };
    letters_a11y::apply_text_colour(&mut doc, &runs, suggested).unwrap();
    let doc = reload(&doc);
    let after = measured(&doc, "recolour-after");
    let mut expected = before;
    expected.remove("low-contrast");
    assert_eq!(problems(&after), expected);
    // the black paragraph is still black: its pixels did not change
    let mut pdfium = Pdfium::open(&doc, "recolour-black");
    let region = letters_a11y::Rect::new(72.0, 390.0, 60.0, 12.0);
    let image = letters_a11y::RegionRenderer::render_region(&mut pdfium, 0, region, 1.0).unwrap();
    let darkest = (0..image.width)
        .flat_map(|x| (0..image.height).map(move |y| (x, y)))
        .filter_map(|(x, y)| image.pixel(x, y))
        .map(|[r, _, _]| r)
        .min()
        .unwrap();
    assert_eq!(darkest, 0);
}
