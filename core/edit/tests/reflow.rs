//! paragraph reflow that moves the content below it down the column,
//! stops at the column's bottom, and reports overflow with choices.

mod support;

use letters_edit::{
    page_objects, reflow_paragraph, LimitReason, OverflowChoice, PageObjects, Rect, ReflowOptions,
};
use lopdf::Document;
use support::{load, render, save, Image, PageSetup, PdfBuilder};

/// two columns of 10 point helvetica, 14 points apart. the left column
/// holds paragraph A, paragraph B, a picture, a green box and a link;
/// the right column paragraph C. each page has a footer "Page n".
fn two_column_page(page_number: usize) -> String {
    format!(
        "BT /F1 10 Tf 14 TL 72 700 Td (Letters keeps the fonts and spacing of) Tj T* \
         (every line while a paragraph is edited) Tj T* (so the page still looks right.) Tj ET
BT /F1 10 Tf 14 TL 72 640 Td (A second paragraph sits below the) Tj T* (first one in the same column.) Tj ET
q 100 0 0 40 72 560 cm /Im1 Do Q
0 0.5 0 rg 72 500 128 40 re f
0 g BT /F1 10 Tf 14 TL 324 700 Td (The right column has its own text) Tj T* \
         (which must not move at all when the) Tj T* (left column grows or shrinks.) Tj ET
BT /F1 9 Tf 290 40 Td (Page {page_number}) Tj ET"
    )
}

fn two_column_pdf(pages: usize) -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    let image = pdf.stream(
        "/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8",
        &[255, 0, 0, 0, 0, 255, 0, 0, 255, 255, 0, 0],
    );
    for number in 1..=pages {
        let link =
            pdf.add("<< /Type /Annot /Subtype /Link /Rect [72 470 150 490] /Border [0 0 0] >>");
        pdf.page_with_extra(
            &PageSetup::letter(),
            &[&two_column_page(number)],
            &format!("/Font << /F1 3 0 R >> /XObject << /Im1 {image} 0 R >>"),
            &format!("/Annots [{link} 0 R]"),
        );
    }
    pdf.finish()
}

const LONGER: &str = "Letters keeps the fonts and spacing of every line while a paragraph is edited so the page still looks right, even when a whole new sentence is added at the end.";

fn annotation_rect(doc: &Document, page: usize) -> [f32; 4] {
    let page_id = doc.get_pages()[&(page as u32 + 1)];
    let annots = doc
        .get_dictionary(page_id)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    let link = doc
        .get_dictionary(annots[0].as_reference().unwrap())
        .unwrap();
    let rect = link.get(b"Rect").unwrap().as_array().unwrap();
    [0, 1, 2, 3].map(|index| rect[index].as_float().unwrap())
}

fn moved_by(before: &Rect, after: &Rect, dy: f32) -> bool {
    (after.x - before.x).abs() < 0.01 && (after.y - before.y - dy).abs() < 0.01
}

/// how many pixels of a region differ from the same region `dy` lower.
fn shifted_differences(
    before: &Image,
    after: &Image,
    region: (u32, u32, u32, u32),
    dy: u32,
) -> usize {
    let (left, top, right, bottom) = region;
    let mut count = 0;
    for y in top..bottom {
        for x in left..right {
            if before.pixel(x as f32, y as f32) != after.pixel(x as f32, (y + dy) as f32) {
                count += 1;
            }
        }
    }
    count
}

fn options() -> ReflowOptions {
    ReflowOptions::default()
}

/// everything below paragraph a in the left column is the same picture
/// exactly dy pixels lower, and the right column and footer are the same.
fn assert_only_the_column_moved(before: &Image, after: &Image, dy: u32) {
    let below_a = (60, 140, 285, 300);
    assert_eq!(shifted_differences(before, after, below_a, dy), 0);
    assert_eq!(before.differences_in(after, (300, 0, 612, 792)), 0);
    assert_eq!(before.differences_in(after, (0, 330, 612, 792)), 0);
}

#[test]
fn a_growing_paragraph_pushes_its_column_down_by_whole_lines() {
    let bytes = two_column_pdf(2);
    let before_pixels = render(&bytes, 0);
    let mut doc = load(&bytes);
    let before = page_objects(&doc, 0).unwrap();
    let link_before = annotation_rect(&doc, 0);
    let a = before.text_blocks[0].id.clone();
    let result = reflow_paragraph(&mut doc, 0, &a, LONGER, &options()).unwrap();
    assert!(result.applied);
    assert_eq!(result.lines_before, 3);
    assert!(result.lines_after > 3);
    let dy = result.height_change;
    assert!(
        (dy - 14.0 * (result.lines_after - 3) as f32).abs() < 0.01,
        "{dy}"
    );
    let after = &result.objects;
    assert!(
        moved_by(&before.text_blocks[1].rect, &after.text_blocks[1].rect, dy),
        "B moves"
    );
    assert!(
        moved_by(&before.images[0].rect, &after.images[0].rect, dy),
        "the picture moves"
    );
    assert!(
        moved_by(&before.paths[0].rect, &after.paths[0].rect, dy),
        "the box moves"
    );
    assert_eq!(
        before.text_blocks[2].rect, after.text_blocks[2].rect,
        "the right column stays"
    );
    assert_eq!(
        before.text_blocks[3].rect, after.text_blocks[3].rect,
        "the footer stays"
    );
    let link_after = annotation_rect(&doc, 0);
    assert!(
        (link_before[1] - link_after[1] - dy).abs() < 0.01,
        "the link stays with its content"
    );
    assert_eq!(result.summary.text_lines_moved, 2);
    assert_eq!(
        (
            result.summary.images_moved,
            result.summary.paths_moved,
            result.summary.annotations_moved
        ),
        (1, 1, 1)
    );

    let saved = save(&mut doc);
    assert_only_the_column_moved(&before_pixels, &render(&saved, 0), dy as u32);

    let text = support::page_text(&load(&saved), 0);
    assert!(
        text[0].replace('\n', " ").ends_with("added at the end."),
        "{text:?}"
    );
}

#[test]
fn other_pages_are_not_rewritten() {
    let bytes = two_column_pdf(2);
    let mut doc = load(&bytes);
    let untouched = page_object_bytes(&doc, 1);
    let a = page_objects(&doc, 0).unwrap().text_blocks[0].id.clone();
    reflow_paragraph(&mut doc, 0, &a, LONGER, &options()).unwrap();
    assert_eq!(page_object_bytes(&doc, 1), untouched);
}

/// every object page `page` uses directly (its dictionary and content
/// streams), written out, to compare before and after.
pub fn page_object_bytes(doc: &Document, page: usize) -> Vec<String> {
    let page_id = doc.get_pages()[&(page as u32 + 1)];
    let mut ids = vec![page_id];
    ids.extend(doc.get_page_contents(page_id));
    ids.iter()
        .map(|id| format!("{:?}", doc.get_object(*id).unwrap()))
        .collect()
}

#[test]
fn shrinking_leaves_the_gap_unless_asked_to_close_it() {
    let bytes = two_column_pdf(2);
    let mut doc = load(&bytes);
    let before = page_objects(&doc, 0).unwrap();
    let a = before.text_blocks[0].id.clone();
    let short = "A short paragraph now.";
    let result = reflow_paragraph(&mut doc, 0, &a, short, &options()).unwrap();
    assert_eq!(result.lines_after, 1);
    assert_eq!(
        result.objects.text_blocks[1].rect, before.text_blocks[1].rect,
        "the gap stays"
    );

    let mut doc = load(&bytes);
    let closing = ReflowOptions {
        close_gap: true,
        ..options()
    };
    let result = reflow_paragraph(&mut doc, 0, &a, short, &closing).unwrap();
    assert!(moved_by(
        &before.text_blocks[1].rect,
        &result.objects.text_blocks[1].rect,
        -28.0
    ));
    assert!(moved_by(
        &before.paths[0].rect,
        &result.objects.paths[0].rect,
        -28.0
    ));
}

/// options with the column's bottom right at the lowest content, so any
/// growth overflows.
fn full_column(objects: &PageObjects) -> ReflowOptions {
    let lowest = objects.paths[0].rect.bottom();
    ReflowOptions {
        bottom_limit: Some(lowest),
        ..options()
    }
}

#[test]
fn overflow_is_reported_with_choices_and_nothing_changes() {
    let bytes = two_column_pdf(2);
    let mut doc = load(&bytes);
    let objects = page_objects(&doc, 0).unwrap();
    let a = objects.text_blocks[0].id.clone();
    let result = reflow_paragraph(&mut doc, 0, &a, LONGER, &full_column(&objects)).unwrap();
    assert!(!result.applied);
    let report = result.overflow.unwrap();
    assert_eq!(report.limit_reason, LimitReason::Caller);
    assert!((report.amount - 14.0 * (result.lines_after - 3) as f32).abs() < 0.01);
    let choices: Vec<OverflowChoice> = report.choices.iter().map(|option| option.choice).collect();
    assert_eq!(
        choices,
        [
            OverflowChoice::AllowIntoMargin,
            OverflowChoice::Tighten,
            OverflowChoice::CarryToNextPage,
            OverflowChoice::Leave
        ]
    );
    let carry = &report.choices[2];
    assert!(
        !carry.possible,
        "content below the paragraph can't be carried: {}",
        carry.detail
    );
    assert_eq!(
        save(&mut doc),
        save(&mut load(&bytes)),
        "the file is unchanged"
    );
}

#[test]
fn the_footer_of_neighbouring_pages_limits_the_column() {
    let bytes = two_column_pdf(2);
    let mut doc = load(&bytes);
    let a = page_objects(&doc, 0).unwrap().text_blocks[0].id.clone();
    let huge = LONGER.repeat(12);
    let result = reflow_paragraph(&mut doc, 0, &a, &huge, &options()).unwrap();
    assert!(!result.applied);
    let report = result.overflow.unwrap();
    assert_eq!(report.limit_reason, LimitReason::Footer);
    // the footer's text starts about 745 points down; the limit sits above it
    assert!(
        report.bottom_limit > 730.0 && report.bottom_limit < 745.0,
        "{}",
        report.bottom_limit
    );
}

#[test]
fn a_margin_allowance_lets_content_run_a_little_lower() {
    let bytes = two_column_pdf(2);
    let mut doc = load(&bytes);
    let objects = page_objects(&doc, 0).unwrap();
    let a = objects.text_blocks[0].id.clone();
    let allowing = ReflowOptions {
        overflow: OverflowChoice::AllowIntoMargin,
        ..full_column(&objects)
    };
    let result = reflow_paragraph(&mut doc, 0, &a, LONGER, &allowing).unwrap();
    assert!(result.applied);
    assert!(result.overflow.is_some(), "the overflow is still reported");
    let tiny = ReflowOptions {
        margin_allowance: 1.0,
        ..allowing
    };
    let mut doc = load(&bytes);
    let refused = reflow_paragraph(&mut doc, 0, &a, LONGER, &tiny).unwrap();
    assert!(!refused.applied);
}

#[test]
fn tightening_fits_a_few_more_letters_and_says_how_much() {
    let bytes = two_column_pdf(2);
    let mut doc = load(&bytes);
    let objects = page_objects(&doc, 0).unwrap();
    let a = &objects.text_blocks[0];
    // the shortest addition that no longer fits with normal spacing: one
    // more letter than the last line can hold
    let full = ReflowOptions {
        bottom_limit: Some(objects.paths[0].rect.bottom()),
        ..options()
    };
    let base = a.text.replace('\n', " ");
    let overflowing = (1..40)
        .map(|letters| format!("{base} {}", "m".repeat(letters)))
        .find(|text| {
            !reflow_paragraph(&mut load(&bytes), 0, &a.id, text, &full)
                .unwrap()
                .applied
        })
        .expect("some addition overflows");
    let tightening = ReflowOptions {
        overflow: OverflowChoice::Tighten,
        ..full
    };
    let result = reflow_paragraph(&mut doc, 0, &a.id, &overflowing, &tightening).unwrap();
    if !result.applied {
        panic!("tightening didn't fit: {:?}", result.overflow);
    }
    assert_eq!(result.lines_after, 3);
    let applied = result.summary.tightening.expect("tightening is reported");
    assert!(
        applied.letter_spacing_em >= -0.015
            && applied.word_spacing_scale >= 0.85
            && applied.leading_scale >= 0.97
    );
    assert_eq!(
        result.objects.text_blocks[1].rect, objects.text_blocks[1].rect,
        "nothing below moved"
    );
    let text = support::page_text(&load(&save(&mut doc)), 0);
    assert!(
        text[0].ends_with(&format!(" {}", overflowing.rsplit(' ').next().unwrap())),
        "{text:?}"
    );
}

/// a page whose left column ends with paragraph A, for carrying.
fn bottom_paragraph_pdf() -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    let first = "BT /F1 10 Tf 14 TL 72 700 Td (Letters keeps the fonts and spacing of) Tj T* \
                 (every line while a paragraph is edited) Tj T* (so the page still looks right.) Tj ET";
    let second = "BT /F1 10 Tf 14 TL 72 700 Td (The next page starts with this line) Tj T* (and this one follows it.) Tj ET";
    pdf.page(&PageSetup::letter(), &[first], "/Font << /F1 3 0 R >>");
    pdf.page(&PageSetup::letter(), &[second], "/Font << /F1 3 0 R >>");
    pdf.finish()
}

#[test]
fn lines_that_do_not_fit_are_carried_to_the_next_page() {
    let mut doc = load(&bottom_paragraph_pdf());
    let objects = page_objects(&doc, 0).unwrap();
    let next_before = page_objects(&doc, 1).unwrap();
    let a = &objects.text_blocks[0];
    let carrying = ReflowOptions {
        overflow: OverflowChoice::CarryToNextPage,
        bottom_limit: Some(a.rect.bottom()),
        ..options()
    };
    let result = reflow_paragraph(&mut doc, 0, &a.id, LONGER, &carrying).unwrap();
    assert!(result.applied, "{:?}", result.overflow);
    assert_eq!(result.summary.pages_changed, [0, 1]);
    assert_eq!(
        result.objects.text_blocks[0].lines.len(),
        3,
        "this page keeps three lines"
    );
    let carried = result.lines_after - 3;
    let next = page_objects(&doc, 1).unwrap();
    // the carried text is drawn after the page's own content, so it is the last block read
    let first_next = next
        .text_blocks
        .iter()
        .find(|block| block.text.contains("end."))
        .unwrap();
    assert!(
        (first_next.rect.y - next_before.text_blocks[0].rect.y).abs() < 1.0,
        "the carried lines start where the column did"
    );
    assert!(
        first_next
            .text
            .replace('\n', " ")
            .ends_with("added at the end."),
        "{:?}",
        first_next.text
    );
    let pushed = next
        .text_blocks
        .iter()
        .find(|block| block.text.starts_with("The next page"))
        .unwrap();
    assert!(moved_by(
        &next_before.text_blocks[0].rect,
        &pushed.rect,
        14.0 * carried as f32
    ));
    let saved = save(&mut doc);
    assert!(
        !support::find_text(&saved, 1, "added at the end").is_empty(),
        "the carried text is real text"
    );
}

#[test]
fn reflowing_and_reflowing_back_restores_the_geometry() {
    let bytes = two_column_pdf(2);
    let mut doc = load(&bytes);
    let before = page_objects(&doc, 0).unwrap();
    let a = before.text_blocks[0].clone();
    let grown = reflow_paragraph(&mut doc, 0, &a.id, LONGER, &options()).unwrap();
    let back = ReflowOptions {
        close_gap: true,
        ..options()
    };
    let original = a.text.replace('\n', " ");
    let restored = reflow_paragraph(
        &mut doc,
        0,
        grown.block_id.as_ref().unwrap(),
        &original,
        &back,
    )
    .unwrap();
    let after = restored.objects;
    for (old, new) in before.text_blocks.iter().zip(&after.text_blocks) {
        assert!(
            (old.rect.y - new.rect.y).abs() < 0.01 && (old.rect.x - new.rect.x).abs() < 0.01,
            "{old:?} {new:?}"
        );
        assert_eq!(old.text, new.text);
    }
    for (old, new) in before.paths.iter().zip(&after.paths) {
        assert!((old.rect.y - new.rect.y).abs() < 0.01);
    }
    assert!((before.images[0].rect.y - after.images[0].rect.y).abs() < 0.01);
}

#[test]
fn repeated_reflows_converge() {
    let bytes = two_column_pdf(2);
    let mut doc = load(&bytes);
    let mut id = page_objects(&doc, 0).unwrap().text_blocks[0].id.clone();
    let mut positions = Vec::new();
    for _ in 0..4 {
        let result = reflow_paragraph(&mut doc, 0, &id, LONGER, &options()).unwrap();
        id = result.block_id.clone().unwrap();
        positions.push(result.objects.text_blocks[1].rect.y);
    }
    // the first edit grows the paragraph; the same text again changes nothing
    assert!(
        (positions[1] - positions[0]).abs() < 0.01 && (positions[3] - positions[0]).abs() < 0.01,
        "{positions:?}"
    );
}
