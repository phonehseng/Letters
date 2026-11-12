//! rotating and cropping, checked both in the file's structure and in how
//! pdfium actually draws the pages.

mod support;

use letters_pages::{
    crop, rotate, uncrop, CropParams, CropRect, PagesError, RotateParams, UncropParams,
};
use lopdf::{Document, Object};
use support::*;

/// a 200 by 100 page with a red square in the paper's bottom left corner.
fn corner_page() -> PageSpec {
    PageSpec::new(200.0, 100.0).drawing(&red_square(0.0, 0.0, 20.0))
}

fn turn(doc: &mut Document, pages: Vec<u32>, quarter_turns: i32) {
    rotate(
        doc,
        &RotateParams {
            pages,
            quarter_turns,
        },
    )
    .unwrap();
}

fn crop_to(
    doc: &mut Document,
    pages: Vec<u32>,
    left: f32,
    top: f32,
    width: f32,
    height: f32,
) -> Result<(), PagesError> {
    crop(
        doc,
        &CropParams {
            pages,
            rect: CropRect {
                left,
                top,
                width,
                height,
            },
        },
    )
}

#[test]
fn rotating_adds_quarter_turns_and_wraps_around() {
    let mut doc = build(&[corner_page(), corner_page().rotated(270), corner_page()]);
    turn(&mut doc, vec![0, 1], 1);
    turn(&mut doc, vec![2], -1);
    let doc = reload(&mut doc);
    let rotations: Vec<i64> = page_ids(&doc)
        .into_iter()
        .map(|page| rotation(&doc, page))
        .collect();
    assert_eq!(rotations, [90, 0, 270]);
}

#[test]
fn a_rotated_page_is_drawn_turned() {
    let mut doc = build(&[corner_page()]);
    turn(&mut doc, vec![0], 1);
    let shown = render(&reload(&mut doc), 0, "rotated");
    assert_eq!((shown.width_pt, shown.height_pt), (100.0, 200.0));
    // a quarter turn clockwise brings the paper's bottom left to the top left
    assert_eq!(shown.image.pixel(5, 5), RED);
    assert_eq!(shown.image.pixel(5, 195), WHITE);
    assert_eq!(shown.image.pixel(95, 5), WHITE);
}

#[test]
fn rotation_builds_on_a_turn_inherited_from_the_page_tree() {
    let mut doc = build(&[corner_page()]);
    // move the turn onto the root of the page tree, where the page inherits it
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_dictionary_mut(root)
        .unwrap()
        .set("Rotate", Object::Integer(90));
    turn(&mut doc, vec![0], 1);
    let doc = reload(&mut doc);
    assert_eq!(rotation(&doc, page_ids(&doc)[0]), 180);
    let shown = render(&doc, 0, "inherited-rotation");
    assert_eq!((shown.width_pt, shown.height_pt), (200.0, 100.0));
    // half a turn puts the paper's bottom left at the top right
    assert_eq!(shown.image.pixel(195, 5), RED);
    assert_eq!(shown.image.pixel(5, 95), WHITE);
}

#[test]
fn cropping_uses_the_page_as_the_person_sees_it() {
    let mut doc = build(&[corner_page().rotated(90)]);
    // shown, the page is 100 wide and 200 tall with the square at the top left
    crop_to(&mut doc, vec![0], 0.0, 0.0, 50.0, 60.0).unwrap();
    let doc = reload(&mut doc);
    let page = page_ids(&doc)[0];
    assert_eq!(
        page_box(&doc, page, b"CropBox"),
        Some([0.0, 0.0, 60.0, 50.0])
    );
    let shown = render(&doc, 0, "crop-rotated");
    assert_eq!((shown.width_pt, shown.height_pt), (50.0, 60.0));
    assert_eq!(shown.image.pixel(5, 5), RED);
    assert_eq!(shown.image.pixel(45, 55), WHITE);
    // the content outside is hidden, not removed
    assert!(content_text(&doc, page).contains("0 0 20 20 re"));
}

#[test]
fn cropping_works_when_the_paper_does_not_start_at_zero() {
    let page = PageSpec::new(200.0, 100.0)
        .origin(100.0, 200.0)
        .drawing(&red_square(280.0, 280.0, 20.0));
    let mut doc = build(&[page]);
    crop_to(&mut doc, vec![0], 150.0, 0.0, 50.0, 50.0).unwrap();
    let doc = reload(&mut doc);
    assert_eq!(
        page_box(&doc, page_ids(&doc)[0], b"CropBox"),
        Some([250.0, 250.0, 300.0, 300.0])
    );
    let shown = render(&doc, 0, "crop-offset");
    assert_eq!((shown.width_pt, shown.height_pt), (50.0, 50.0));
    assert_eq!(shown.image.pixel(40, 10), RED);
    assert_eq!(shown.image.pixel(10, 40), WHITE);
}

#[test]
fn a_second_crop_is_measured_inside_the_first() {
    let mut doc = build(&[corner_page()]);
    crop_to(&mut doc, vec![0], 0.0, 50.0, 100.0, 50.0).unwrap();
    crop_to(&mut doc, vec![0], 0.0, 25.0, 40.0, 25.0).unwrap();
    let doc = reload(&mut doc);
    assert_eq!(
        page_box(&doc, page_ids(&doc)[0], b"CropBox"),
        Some([0.0, 0.0, 40.0, 25.0])
    );
    let shown = render(&doc, 0, "crop-twice");
    assert_eq!((shown.width_pt, shown.height_pt), (40.0, 25.0));
    assert_eq!(shown.image.pixel(5, 20), RED);
    assert_eq!(shown.image.pixel(30, 5), WHITE);
}

#[test]
fn uncrop_shows_the_whole_page_again() {
    let mut doc = build(&[corner_page(), corner_page()]);
    crop_to(&mut doc, vec![0, 1], 10.0, 10.0, 50.0, 50.0).unwrap();
    // page 2 also inherits a crop box from the page tree
    let root = doc
        .catalog()
        .unwrap()
        .get(b"Pages")
        .unwrap()
        .as_reference()
        .unwrap();
    doc.get_dictionary_mut(root)
        .unwrap()
        .set("CropBox", rect(0.0, 0.0, 30.0, 30.0));
    uncrop(&mut doc, &UncropParams { pages: vec![0, 1] }).unwrap();
    let doc = reload(&mut doc);
    for index in 0..2 {
        let shown = render(&doc, index, "uncrop");
        assert_eq!(
            (shown.width_pt, shown.height_pt),
            (200.0, 100.0),
            "page {index}"
        );
        assert_eq!(shown.image.pixel(5, 95), RED);
    }
}

#[test]
fn a_crop_outside_the_page_or_too_small_is_refused() {
    let mut doc = build(&[corner_page(), corner_page().rotated(90)]);
    let before = fingerprint(&doc);
    // fits the wide first page but not the tall second one
    assert_eq!(
        crop_to(&mut doc, vec![0, 1], 0.0, 0.0, 150.0, 50.0),
        Err(PagesError::CropOutsidePage(2))
    );
    assert_eq!(
        crop_to(&mut doc, vec![0], 0.0, 0.0, 2.0, 50.0),
        Err(PagesError::CropTooSmall)
    );
    assert_eq!(
        crop_to(&mut doc, vec![0], f32::NAN, 0.0, 20.0, 20.0),
        Err(PagesError::CropTooSmall)
    );
    assert_eq!(
        fingerprint(&doc),
        before,
        "page 1 must not be cropped when page 2 is refused"
    );
}
