//! editing a ruled table's cell: the row grows, the rules and rows below
//! move, vertical rules lengthen, and nothing beside the table moves.

mod support;

use letters_edit::{edit_table_cell, page_objects, PageObjects, PathShape, ReflowOptions};
use support::{load, render, save, PageSetup, PdfBuilder};

/// a two-column, three-row table drawn with single line paths, a
/// paragraph below it and a note beside it.
const TABLE: &str = "\
0 G 1 w
72 700 m 372 700 l S
72 680 m 372 680 l S
72 660 m 372 660 l S
72 640 m 372 640 l S
72 640 m 72 700 l S
222 640 m 222 700 l S
372 640 m 372 700 l S
BT /F1 10 Tf 76 686 Td (Name) Tj ET
BT /F1 10 Tf 226 686 Td (Value) Tj ET
BT /F1 10 Tf 76 666 Td (Short) Tj ET
BT /F1 10 Tf 226 666 Td (One) Tj ET
BT /F1 10 Tf 76 646 Td (Last) Tj ET
BT /F1 10 Tf 226 646 Td (Two) Tj ET
BT /F1 10 Tf 72 600 Td (A paragraph below the table.) Tj ET
BT /F1 10 Tf 420 666 Td (A note beside it) Tj ET";

fn table_pdf() -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    pdf.page(&PageSetup::letter(), &[TABLE], "/Font << /F1 3 0 R >>");
    pdf.finish()
}

fn block<'a>(objects: &'a PageObjects, text: &str) -> &'a letters_edit::TextBlock {
    objects
        .text_blocks
        .iter()
        .find(|block| block.text.starts_with(text))
        .unwrap_or_else(|| {
            panic!(
                "no block {text}: {:?}",
                objects
                    .text_blocks
                    .iter()
                    .map(|b| &b.text)
                    .collect::<Vec<_>>()
            )
        })
}

#[test]
fn each_cell_is_its_own_block() {
    let doc = load(&table_pdf());
    let objects = page_objects(&doc, 0).unwrap();
    for text in ["Name", "Short", "Last", "Value", "One", "Two"] {
        assert_eq!(
            block(&objects, text).text,
            text,
            "the rule keeps rows apart"
        );
    }
}

#[test]
fn a_cell_that_needs_more_lines_grows_its_row() {
    let bytes = table_pdf();
    let before_pixels = render(&bytes, 0);
    let mut doc = load(&bytes);
    let before = page_objects(&doc, 0).unwrap();
    let cell = block(&before, "Short").id.clone();
    let text = "This cell now holds a much longer sentence than before";
    let result = edit_table_cell(&mut doc, 0, &cell, text, &ReflowOptions::default()).unwrap();
    assert!(result.applied, "{:?}", result.overflow);
    assert!(result.lines_after >= 2);
    let growth = result.height_change;
    assert!(growth > 0.0);
    let after = &result.objects;
    // the edited text stays inside its cell's width
    let edited = block(after, "This cell");
    assert!(edited.rect.right() <= 222.0, "{:?}", edited.rect);
    // rows below and the paragraph move down by the growth
    for text in ["Last", "Two", "A paragraph"] {
        let (old, new) = (block(&before, text).rect, block(after, text).rect);
        assert!(
            (new.y - old.y - growth).abs() < 0.01,
            "{text}: {old:?} -> {new:?}"
        );
    }
    // the row above and the note beside the table stay
    for text in ["Name", "Value", "A note"] {
        assert_eq!(block(&before, text).rect, block(after, text).rect, "{text}");
    }
    // the rules below the row move; the vertical rules get longer
    let horizontal_at = |objects: &PageObjects| -> Vec<f32> {
        let mut found: Vec<f32> = objects
            .paths
            .iter()
            .filter(|path| path.shape == PathShape::Line && path.rect.width > 100.0)
            .map(|path| path.points[0].y)
            .collect();
        found.sort_by(f32::total_cmp);
        found
    };
    let old_rules = horizontal_at(&before);
    let new_rules = horizontal_at(after);
    assert_eq!(&new_rules[..2], &old_rules[..2]);
    assert!((new_rules[2] - old_rules[2] - growth).abs() < 0.01);
    assert!((new_rules[3] - old_rules[3] - growth).abs() < 0.01);
    let verticals: Vec<f32> = after
        .paths
        .iter()
        .filter(|path| path.shape == PathShape::Line && path.rect.width < 5.0)
        .map(|path| {
            path.points
                .iter()
                .map(|point| point.y)
                .fold(f32::MIN, f32::max)
        })
        .collect();
    assert_eq!(verticals.len(), 3);
    for bottom in verticals {
        assert!((bottom - (152.0 + growth)).abs() < 0.01, "{bottom}");
    }
    assert_eq!(result.summary.paths_reshaped, 3);
    let after_pixels = render(&save(&mut doc), 0);
    // the rule widths stay one point, and the note's pixels are the same
    assert_eq!(
        before_pixels.differences_in(&after_pixels, (400, 0, 612, 792)),
        0
    );
    // a one point rule on a whole point straddles two pixel rows: grey, not white
    assert_ne!(after_pixels.pixel(150.0, 152.0 + growth), support::WHITE);
}

#[test]
fn text_outside_a_table_is_refused_with_a_sentence() {
    let mut doc = load(&table_pdf());
    let objects = page_objects(&doc, 0).unwrap();
    let outside = block(&objects, "A paragraph").id.clone();
    let error = edit_table_cell(&mut doc, 0, &outside, "x", &ReflowOptions::default()).unwrap_err();
    assert!(error
        .to_string()
        .contains("isn’t inside a ruled table cell"));
}
