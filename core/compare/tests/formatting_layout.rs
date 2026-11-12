//! changes to how unchanged text looks and where it sits.

mod support;

use letters_compare::*;
use support::*;

fn only_change(result: &ComparisonResult) -> &Change {
    let changes: Vec<&Change> = result
        .page_pairs
        .iter()
        .flat_map(|pair| pair.changes.iter())
        .collect();
    assert_eq!(changes.len(), 1, "{changes:#?}");
    changes[0]
}

fn heading_page(size: f32) -> PageSpec {
    let mut items = vec![text(72.0, 80.0, size, "Introduction")];
    items.extend(lines(
        72.0,
        110.0,
        11.0,
        15.0,
        &[
            "Plain body text follows the heading and",
            "does not change at all between versions.",
        ],
    ));
    page(items)
}

#[test]
fn a_font_size_change_is_one_formatting_change() {
    let (old, new) = write_pair(
        "font-size",
        &pdf(vec![heading_page(14.0)]),
        &pdf(vec![heading_page(16.0)]),
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let change = only_change(&result);
    assert_eq!(change.kind, ChangeKind::Restyled);
    assert_eq!(change.category, Category::Formatting);
    assert_eq!(
        change.detail,
        "Font size changed from 14 to 16 in “Introduction”"
    );
    let style = change.style.as_ref().unwrap();
    assert_eq!((style.old.size, style.new.size), (14.0, 16.0));
    assert_eq!(style.old.font, "Helvetica");
    assert!(lands_on(
        &change.left_rects,
        old.word_rect(0, "Introduction"),
        3.0
    ));
    assert!(lands_on(
        &change.right_rects,
        new.word_rect(0, "Introduction"),
        3.0
    ));
    assert_eq!(result.summary.formatting, 1);
}

#[test]
fn a_colour_change_is_one_formatting_change() {
    let old = page(vec![
        text(72.0, 100.0, 12.0, "This is"),
        text(116.0, 100.0, 12.0, "important"),
        text(172.0, 100.0, 12.0, "to know."),
    ]);
    let mut new = old.clone();
    new.items[1] = text(116.0, 100.0, 12.0, "important").colour([1.0, 0.0, 0.0]);
    let (old, new) = write_pair("colour", &pdf(vec![old]), &pdf(vec![new]));
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let change = only_change(&result);
    assert_eq!(
        change.detail,
        "Colour changed from black to red in “important”"
    );
    assert_eq!(
        change.style.as_ref().unwrap().new.colour,
        Colour::new(255, 0, 0)
    );
}

#[test]
fn bold_and_font_family_changes_are_named() {
    let old = page(vec![
        text(72.0, 100.0, 12.0, "Strong words"),
        text(72.0, 130.0, 12.0, "Serif words"),
    ]);
    let new = page(vec![
        text(72.0, 100.0, 12.0, "Strong words").face(Face::HelveticaBold),
        text(72.0, 130.0, 12.0, "Serif words").face(Face::Times),
    ]);
    let (old, new) = write_pair("bold-family", &pdf(vec![old]), &pdf(vec![new]));
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let details: Vec<&str> = result.page_pairs[0]
        .changes
        .iter()
        .map(|change| change.detail.as_str())
        .collect();
    assert_eq!(
        details,
        [
            "Made bold in “Strong words”",
            "Font changed from Helvetica to Times in “Serif words”"
        ]
    );
}

const FIVE_LINES: [&str; 5] = [
    "First line of the page stays put.",
    "Second line of the page stays put.",
    "Third line is the one that moves.",
    "Fourth line of the page stays put.",
    "Fifth line of the page stays put.",
];

#[test]
fn a_line_moved_down_is_one_layout_change() {
    let old = page(lines(72.0, 100.0, 12.0, 30.0, &FIVE_LINES));
    let mut new = old.clone();
    if let Item::Text(line) = &mut new.items[2] {
        line.baseline += 12.0;
    }
    let (old, new) = write_pair("line-shifted", &pdf(vec![old]), &pdf(vec![new]));
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let change = only_change(&result);
    assert_eq!(change.kind, ChangeKind::Shifted);
    assert_eq!(change.category, Category::Layout);
    assert_eq!(
        change.detail,
        "Line moved down 12 pt (“Third line is the one that moves.”)"
    );
    assert!(covers(&change.left_rects, old.word_rect(0, "Third")));
    assert!(covers(&change.right_rects, new.word_rect(0, "Third")));

    // scrolling: the third line's height on the old page maps to its new height
    let pair = &result.page_pairs[0];
    let third_old = old.word_rect(0, "Third").y;
    let third_new = new.word_rect(0, "Third").y;
    assert!(
        (map_scroll(&pair.scroll_map, third_old) - third_new).abs() < 2.0,
        "{:?}",
        pair.scroll_map
    );
    let first = old.word_rect(0, "First").y;
    assert!((map_scroll(&pair.scroll_map, first) - first).abs() < 2.0);
}

#[test]
fn a_wider_left_margin_is_named_as_a_margin_change() {
    let old = page(lines(72.0, 100.0, 12.0, 16.0, &FIVE_LINES));
    let new = page(lines(108.0, 100.0, 12.0, 16.0, &FIVE_LINES));
    let (old, new) = write_pair("margin", &pdf(vec![old]), &pdf(vec![new]));
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let change = only_change(&result);
    assert_eq!(change.kind, ChangeKind::Shifted);
    assert_eq!(change.detail, "Left margin changed from 72 to 108 pt");
    assert_eq!(result.page_pairs[0].offset.dx.round(), 36.0);
}

#[test]
fn a_paragraph_broken_into_different_lines_is_reported_as_reflowed() {
    let old = page(lines(
        72.0,
        100.0,
        12.0,
        15.0,
        &[
            "Words of this paragraph are the same in",
            "both versions but the lines break at",
            "different places.",
        ],
    ));
    let new = page(lines(
        72.0,
        100.0,
        12.0,
        15.0,
        &[
            "Words of this paragraph are the same",
            "in both versions but the lines break",
            "at different places.",
        ],
    ));
    let (old, new) = write_pair("reflowed", &pdf(vec![old]), &pdf(vec![new]));
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let change = only_change(&result);
    assert_eq!(change.kind, ChangeKind::Reflowed);
    assert_eq!(
        change.detail,
        "Paragraph reflowed with different line breaks (“Words of this paragraph are the”)"
    );
}

#[test]
fn turning_off_formatting_and_layout_leaves_only_text() {
    let (old, new) = write_pair(
        "options",
        &pdf(vec![heading_page(14.0)]),
        &pdf(vec![heading_page(18.0)]),
    );
    let options = CompareOptions {
        compare_formatting: false,
        compare_layout: false,
        ..CompareOptions::default()
    };
    let result = compare(
        &old.source(),
        &new.source(),
        &options,
        &mut |_| {},
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        result.summary.total, 0,
        "{:#?}",
        result.page_pairs[0].changes
    );
}
