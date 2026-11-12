//! placing a picture of a signature: in place, upright, see-through.

mod support;

use letters_forms::{place_signature_image, FormError, SignatureImage, VisibleRect};
use support::{load, plain_page_pdf, render_page, save, signature_png, PageSetup};

/// a page with a red block, drawn upright for the page's turn, under where
/// the signature goes (visible 72..272 across, 192..292 down).
fn red_page(rotate: i32) -> lopdf::Document {
    let mut page = PageSetup::letter(rotate);
    let top = if rotate % 180 == 0 { 792.0 } else { 612.0 };
    let matrix = match rotate {
        90 => "0 1 -1 0 612 0",
        180 => "-1 0 0 -1 612 792",
        270 => "0 -1 1 0 0 792",
        _ => "1 0 0 1 0 0",
    };
    page.content = format!("q {matrix} cm 1 0 0 rg 72 {} 200 100 re f Q", top - 292.0);
    load(&plain_page_pdf(&page))
}

fn place(rotate: i32, as_annotation: bool) {
    let mut doc = red_page(rotate);
    let rect = VisibleRect::new(80.0, 200.0, 180.0, 80.0);
    let request = SignatureImage {
        page: 0,
        rect,
        png: signature_png(90, 40),
        as_annotation,
    };
    place_signature_image(&mut doc, &request).unwrap();
    let bytes = save(&mut doc);
    let image = render_page(&bytes, 0, 1000, &format!("stamp-{rotate}-{as_annotation}"));
    let at = |x: f32, y: f32| image.pixel(x as u32, y as u32);
    // the ink band runs across the middle of the picture
    let ink = at(170.0, 240.0);
    assert!(
        ink[2] > ink[0] && ink[0] < 60,
        "no ink at the centre at {rotate}: {ink:?}"
    );
    // the band is horizontal on screen, so the picture is upright
    let ink_left = at(90.0, 240.0);
    assert!(
        ink_left[0] < 60,
        "picture is not upright at {rotate}: {ink_left:?}"
    );
    // the see-through part shows the red page underneath
    assert_eq!(
        at(170.0, 210.0),
        [255, 0, 0],
        "transparency lost at {rotate}"
    );
    // nothing is drawn outside the rectangle
    assert_eq!(at(170.0, 300.0), [255, 255, 255]);
}

#[test]
fn signature_pictures_become_part_of_the_page() {
    for rotate in [0, 90, 180, 270] {
        place(rotate, false);
    }
}

#[test]
fn signature_pictures_can_be_stamps() {
    for rotate in [0, 90] {
        place(rotate, true);
    }
    let mut doc = red_page(0);
    let request = SignatureImage {
        page: 0,
        rect: VisibleRect::new(80.0, 200.0, 180.0, 80.0),
        png: signature_png(30, 10),
        as_annotation: true,
    };
    place_signature_image(&mut doc, &request).unwrap();
    let page = doc.get_dictionary(doc.page_iter().next().unwrap()).unwrap();
    let annots = page.get(b"Annots").unwrap().as_array().unwrap();
    let stamp = doc
        .get_dictionary(annots[0].as_reference().unwrap())
        .unwrap();
    assert_eq!(stamp.get(b"Subtype").unwrap().as_name().unwrap(), b"Stamp");
}

#[test]
fn bad_pictures_are_refused() {
    let mut doc = red_page(0);
    let request = SignatureImage {
        page: 0,
        rect: VisibleRect::new(80.0, 200.0, 180.0, 80.0),
        png: b"not a picture".to_vec(),
        as_annotation: false,
    };
    assert!(matches!(
        place_signature_image(&mut doc, &request),
        Err(FormError::BadImage(_))
    ));
}
