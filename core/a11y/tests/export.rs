//! the report as plain text and as accessible html.

mod support;

use letters_a11y::{report_html, report_text};
use support::*;

fn report_with_problems() -> letters_a11y::Report {
    let mut builder = Builder::new();
    builder.page(&format!(
        "{}{}",
        text("F2", 12.0, 72.0, 100.0, "Hello <world>"),
        picture(72.0, 200.0, 100.0, 100.0)
    ));
    let doc = builder.build();
    check(&doc)
}

#[test]
fn the_text_summary_lists_problems_by_urgency() {
    let text = report_text(&report_with_problems());
    assert!(text.starts_with("Accessibility check"));
    assert!(text.contains("Must fix ("));
    assert!(text.contains("The document has no tags"));
    assert!(text.contains("Why it matters:"));
    assert!(text.contains("Not checked"));
    let must = text.find("Must fix").unwrap();
    let hand = text.find("Check by hand").unwrap_or(usize::MAX);
    assert!(must < hand);
}

#[test]
fn the_html_report_is_accessible_itself() {
    let html = report_html(&report_with_problems());
    assert!(html.starts_with("<!doctype html>"));
    assert!(html.contains("<html lang=\"en\">"));
    assert!(html.contains("<title>Accessibility report</title>"));
    assert_eq!(html.matches("<h1>").count(), 1);
    // severity is said in words in the headings, not only shown by colour
    assert!(html.contains("<h2>Must fix ("));
    assert!(html.contains("<ol>") && html.contains("<h3>"));
    assert!(!html.contains("color: red"));
}

#[test]
fn the_html_report_escapes_text_from_the_document() {
    let mut doc = good_document();
    doc.catalog_mut().unwrap().set("Lang", string("<script>"));
    let html = report_html(&check(&doc));
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>"));
}

#[test]
fn a_clean_report_says_what_is_left_for_a_person() {
    let text = report_text(&check(&good_document()));
    assert!(text.contains("No problems a program can find"));
    assert!(text.contains("Check by hand"));
}
