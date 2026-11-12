//! locations are measured on the page as shown: after /CropBox and
//! /Rotate, from the visible top left corner, y down. each case is also
//! checked against pdfium's drawing of the page: the reported rectangle
//! must cover the picture's pixels.

mod support;

use letters_a11y::{check, Rect, RegionRenderer};
use lopdf::Document;
use support::*;

/// a tagged page whose only content is a figure with no alt text,
/// 200 by 150 points, its top left corner 72 across and 200 down on the
/// unturned paper.
fn figure_page(spec: impl FnOnce(PageSpec) -> PageSpec) -> Document {
    let mut builder = Builder::new();
    let content = format!(
        "{}{}",
        marked("H1", 0, &text("F1", 24.0, 72.0, 100.0, "Title")),
        marked("Figure", 1, &picture(72.0, 200.0, 200.0, 150.0))
    );
    builder.page_spec(spec(PageSpec::new(&content)));
    let pages = builder.pages.clone();
    let mut doc = builder.build();
    add_tags(
        &mut doc,
        &pages,
        vec![leaf("H1", 0, &[0]), leaf("Figure", 0, &[1])],
    );
    add_metadata(&mut doc, "en", "Places");
    doc
}

fn figure_rect(doc: &Document) -> Rect {
    check(doc, None)
        .unwrap()
        .with_id("missing-alt-text")
        .next()
        .unwrap()
        .location
        .rect
        .unwrap()
}

fn assert_close(actual: Rect, expected: Rect) {
    let close = |a: f32, b: f32| (a - b).abs() < 0.5;
    assert!(
        close(actual.x, expected.x)
            && close(actual.y, expected.y)
            && close(actual.width, expected.width)
            && close(actual.height, expected.height),
        "{actual:?} is not {expected:?}"
    );
}

/// every pixel just inside the rectangle shows the picture (it has no white).
fn assert_picture_drawn_at(doc: &Document, rect: Rect, name: &str) {
    let mut pdfium = Pdfium::open(doc, name);
    let inside = Rect::new(
        rect.x + 4.0,
        rect.y + 4.0,
        rect.width - 8.0,
        rect.height - 8.0,
    );
    let image = pdfium.render_region(0, inside, 1.0).unwrap();
    for y in 0..image.height {
        for x in 0..image.width {
            assert_ne!(
                image.pixel(x, y),
                Some([255, 255, 255]),
                "white at {x},{y} inside {rect:?}"
            );
        }
    }
}

#[test]
fn an_unturned_page_measures_from_its_top_left() {
    let doc = figure_page(|spec| spec);
    let rect = figure_rect(&doc);
    assert_close(rect, Rect::new(72.0, 200.0, 200.0, 150.0));
    assert_picture_drawn_at(&doc, rect, "plain");
}

#[test]
fn a_quarter_turned_page_measures_on_the_turned_page() {
    let doc = figure_page(|spec| spec.rotated(90));
    let rect = figure_rect(&doc);
    // turned clockwise, the paper's left edge is now the top
    assert_close(rect, Rect::new(792.0 - 350.0, 72.0, 150.0, 200.0));
    assert_picture_drawn_at(&doc, rect, "turned-90");
}

#[test]
fn a_page_turned_upside_down() {
    let doc = figure_page(|spec| spec.rotated(180));
    let rect = figure_rect(&doc);
    assert_close(rect, Rect::new(612.0 - 272.0, 792.0 - 350.0, 200.0, 150.0));
    assert_picture_drawn_at(&doc, rect, "turned-180");
}

#[test]
fn a_page_turned_three_quarters() {
    let doc = figure_page(|spec| spec.rotated(270));
    let rect = figure_rect(&doc);
    assert_close(rect, Rect::new(200.0, 612.0 - 272.0, 150.0, 200.0));
    assert_picture_drawn_at(&doc, rect, "turned-270");
}

#[test]
fn a_cropped_page_measures_from_the_crop() {
    let doc = figure_page(|spec| spec.cropped([50.0, 50.0, 562.0, 742.0]));
    let rect = figure_rect(&doc);
    assert_close(rect, Rect::new(22.0, 150.0, 200.0, 150.0));
    assert_picture_drawn_at(&doc, rect, "cropped");
}

#[test]
fn a_cropped_and_turned_page() {
    let doc = figure_page(|spec| spec.cropped([50.0, 50.0, 562.0, 742.0]).rotated(90));
    let rect = figure_rect(&doc);
    // the shown area is 512 wide and 692 tall before turning
    assert_close(rect, Rect::new(692.0 - 300.0, 22.0, 150.0, 200.0));
    assert_picture_drawn_at(&doc, rect, "cropped-turned");
}
