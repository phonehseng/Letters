//! whole pages: added, removed, reordered, turned and cropped.

mod support;

use letters_compare::*;
use support::*;

fn kinds(result: &ComparisonResult) -> Vec<ChangeKind> {
    result
        .page_pairs
        .iter()
        .flat_map(|pair| pair.changes.iter().map(|change| change.kind))
        .collect()
}

fn pairs(result: &ComparisonResult) -> Vec<(Option<u32>, Option<u32>)> {
    result
        .page_pairs
        .iter()
        .map(|pair| (pair.left_page, pair.right_page))
        .collect()
}

#[test]
fn a_page_inserted_in_the_middle_does_not_shift_the_rest() {
    let old = pdf((0..4).map(|seed| numbered_page(seed, 6)).collect());
    let mut new_pages: Vec<PageSpec> = (0..4).map(|seed| numbered_page(seed, 6)).collect();
    new_pages.insert(2, numbered_page(99, 6));
    let (old, new) = write_pair("page-inserted", &old, &pdf(new_pages));
    let result = compare_documents(&old.source(), &new.source()).unwrap();

    assert_eq!(
        pairs(&result),
        [
            (Some(0), Some(0)),
            (Some(1), Some(1)),
            (None, Some(2)),
            (Some(2), Some(3)),
            (Some(3), Some(4))
        ]
    );
    assert_eq!(kinds(&result), [ChangeKind::PageAdded]);
    let added = &result.page_pairs[2].changes[0];
    assert_eq!(added.category, Category::Page);
    assert!(
        added
            .detail
            .starts_with("Page 3 was added (it begins “w99x0y0"),
        "{}",
        added.detail
    );
    assert_eq!(result.summary.page, 1);
    assert_eq!(result.summary.total, 1);
}

#[test]
fn a_deleted_page_is_listed_where_it_was() {
    let old = pdf((0..4).map(|seed| numbered_page(seed, 6)).collect());
    let new = pdf([0, 2, 3]
        .iter()
        .map(|&seed| numbered_page(seed, 6))
        .collect());
    let (old, new) = write_pair("page-deleted", &old, &new);
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    assert_eq!(
        pairs(&result),
        [
            (Some(0), Some(0)),
            (Some(1), None),
            (Some(2), Some(1)),
            (Some(3), Some(2))
        ]
    );
    assert_eq!(kinds(&result), [ChangeKind::PageRemoved]);
    assert!(result.page_pairs[1].changes[0]
        .detail
        .starts_with("Old page 2 was removed"));
}

#[test]
fn reordered_pages_are_matched_and_reported_as_moved() {
    let old = pdf((0..5).map(|seed| numbered_page(seed, 6)).collect());
    let new = pdf([0, 3, 1, 2, 4]
        .iter()
        .map(|&seed| numbered_page(seed, 6))
        .collect());
    let (old, new) = write_pair("pages-reordered", &old, &new);
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    // every page found its partner
    for pair in &result.page_pairs {
        assert!(
            pair.left_page.is_some() && pair.right_page.is_some(),
            "{:?}",
            pairs(&result)
        );
        assert_eq!(pair.similarity, 1.0);
    }
    assert_eq!(kinds(&result), [ChangeKind::PageMoved]);
    let moved = result
        .page_pairs
        .iter()
        .find(|pair| pair.has_changes())
        .unwrap();
    assert_eq!((moved.left_page, moved.right_page), (Some(3), Some(1)));
    assert_eq!(moved.changes[0].detail, "Old page 4 is now page 2");
}

#[test]
fn a_page_turned_sideways_is_one_layout_change_and_its_words_still_match() {
    let items = lines(
        72.0,
        100.0,
        12.0,
        16.0,
        &[
            "The fox runs across the open field",
            "and hides under the hedge.",
        ],
    );
    let mut turned = page(lines(
        72.0,
        100.0,
        12.0,
        16.0,
        &[
            "The cat runs across the open field",
            "and hides under the hedge.",
        ],
    ));
    turned.rotate = 90;
    let (old, new) = write_pair("rotated", &pdf(vec![page(items)]), &pdf(vec![turned]));
    let new_source = new.source();
    assert_eq!(
        new_source.page_size(0).unwrap(),
        PageSize::new(792.0, 612.0)
    );
    let result = compare_documents(&old.source(), &new_source).unwrap();

    let pair = &result.page_pairs[0];
    let found: Vec<ChangeKind> = pair.changes.iter().map(|change| change.kind).collect();
    assert_eq!(
        found,
        [ChangeKind::Resized, ChangeKind::Replaced],
        "{:#?}",
        pair.changes
    );
    assert_eq!(
        pair.changes[0].detail,
        "Page turned from portrait to landscape (612 × 792 pt to 792 × 612 pt)"
    );
    let replaced = &pair.changes[1];
    assert_eq!(replaced.new_text.as_deref(), Some("cat"));
    // on the turned page the word runs down the page, where pdfium sees it too
    assert!(
        lands_on(&replaced.right_rects, new.word_rect(0, "cat"), 3.0),
        "{:?} vs {:?}",
        replaced.right_rects,
        new.word_rect(0, "cat")
    );
}

#[test]
fn a_cropped_page_reports_its_new_size_and_not_every_line_as_moved() {
    let texts = [
        "The fox runs across the open field",
        "and hides under the hedge.",
    ];
    let mut cropped = page(lines(
        72.0,
        100.0,
        12.0,
        16.0,
        &[
            "The cat runs across the open field",
            "and hides under the hedge.",
        ],
    ));
    cropped.crop = Some([36.0, 36.0, 576.0, 756.0]);
    let (old, new) = write_pair(
        "cropped",
        &pdf(vec![page(lines(72.0, 100.0, 12.0, 16.0, &texts))]),
        &pdf(vec![cropped]),
    );
    let result = compare_documents(&old.source(), &new.source()).unwrap();
    let pair = &result.page_pairs[0];
    let found: Vec<ChangeKind> = pair.changes.iter().map(|change| change.kind).collect();
    assert_eq!(
        found,
        [ChangeKind::Resized, ChangeKind::Replaced],
        "{:#?}",
        pair.changes
    );
    assert_eq!(
        pair.changes[0].detail,
        "Page size changed from 612 × 792 pt to 540 × 720 pt"
    );
    // the crop moves everything 36 points left and up on the visible page
    let cat = new.word_rect(0, "cat");
    assert!(
        lands_on(&pair.changes[1].right_rects, cat, 3.0),
        "{:?} vs {cat:?}",
        pair.changes[1].right_rects
    );
    assert!(
        (pair.offset.dx + 36.0).abs() < 1.0 && (pair.offset.dy + 36.0).abs() < 1.0,
        "{:?}",
        pair.offset
    );
}
