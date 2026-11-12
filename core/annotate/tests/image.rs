//! pictures placed as stamp annotations, from png and jpeg files.

mod support;

use letters_annotate::{
    add_image, list, update, AnnotateError, AnnotationChanges, AnnotationDetails, AnnotationKind,
    ImageParams, Rect,
};
use lopdf::{Document, ObjectId};
use support::*;

/// a 30 by 10 png: red on the left, see-through in the middle, blue on the right.
fn three_stripe_png() -> Vec<u8> {
    let mut pixels = Vec::new();
    for _ in 0..10 {
        for x in 0..30 {
            pixels.extend_from_slice(match x / 10 {
                0 => &[255, 0, 0, 255],
                1 => &[0, 0, 0, 0],
                _ => &[0, 0, 255, 255],
            });
        }
    }
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, 30, 10);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&pixels).unwrap();
    writer.finish().unwrap();
    bytes
}

/// a 32 by 16 jpeg made for these tests: red on the left half, blue on the right.
fn red_blue_jpeg() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/red-blue.jpg"
    ))
    .unwrap()
}

fn picture(rect: Rect, image: Vec<u8>) -> ImageParams {
    ImageParams {
        rect,
        image,
        opacity: 1.0,
        contents: None,
        author: Some("Ada".into()),
    }
}

/// the image object a stamp's appearance draws.
fn shown_image(doc: &Document, annotation: &lopdf::Dictionary) -> ObjectId {
    let normal = annotation
        .get(b"AP")
        .unwrap()
        .as_dict()
        .unwrap()
        .get(b"N")
        .unwrap()
        .as_reference()
        .unwrap();
    let form = doc.get_object(normal).unwrap().as_stream().unwrap();
    let resources = form.dict.get(b"Resources").unwrap().as_dict().unwrap();
    let images = resources.get(b"XObject").unwrap().as_dict().unwrap();
    images.iter().next().unwrap().1.as_reference().unwrap()
}

#[test]
fn png_pictures_keep_their_transparency_and_stay_upright() {
    let rect = Rect::new(100.0, 100.0, 300.0, 100.0);
    for (layout, page) in [
        ("upright", TestPage::letter()),
        ("turned 90", TestPage::letter().rotated(90)),
        ("turned 180", TestPage::letter().rotated(180)),
        (
            "cropped",
            TestPage::letter().cropped([50.0, 80.0, 562.0, 742.0]),
        ),
    ] {
        let mut doc = load(&pdf_bytes(&[page], false));
        let info = add_image(&mut doc, 0, &picture(rect, three_stripe_png())).unwrap();
        assert_eq!(info.kind, AnnotationKind::Stamp);
        let saved = save(&mut doc);
        let reloaded = load(&saved);
        let annotation = annotation_by_name(&reloaded, 0, &info.id);
        assert_eq!(name_of(&annotation, b"Subtype"), "Stamp");
        assert_eq!(name_of(&annotation, b"Name"), "Image");
        let image = reloaded
            .get_object(shown_image(&reloaded, &annotation))
            .unwrap()
            .as_stream()
            .unwrap();
        assert_eq!(name_of(&image.dict, b"Subtype"), "Image");
        assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 30);
        assert!(
            image.dict.has(b"SMask"),
            "the see-through stripe needs a mask"
        );
        let listed = &list(&reloaded, 0).unwrap()[0];
        assert_eq!(
            listed.details,
            AnnotationDetails::Stamp { is_picture: true }
        );

        let shown = render(&saved, 0);
        assert_near(
            shown.pixel(150.0, 150.0),
            [255, 0, 0],
            10,
            &format!("{layout}: red on the left"),
        );
        assert_eq!(
            shown.pixel(250.0, 150.0),
            WHITE,
            "{layout}: the page shows through the middle"
        );
        assert_near(
            shown.pixel(350.0, 150.0),
            [0, 0, 255],
            10,
            &format!("{layout}: blue on the right"),
        );
        assert_eq!(
            shown.pixel(150.0, 205.0),
            WHITE,
            "{layout}: below the picture"
        );
    }
}

#[test]
fn jpeg_pictures_are_stored_as_they_are() {
    let jpeg = red_blue_jpeg();
    let mut doc = load(&pdf_bytes(&[TestPage::letter().rotated(270)], false));
    let info = add_image(
        &mut doc,
        0,
        &picture(Rect::new(100.0, 100.0, 200.0, 100.0), jpeg.clone()),
    )
    .unwrap();
    let saved = save(&mut doc);
    let reloaded = load(&saved);
    let annotation = annotation_by_name(&reloaded, 0, &info.id);
    let image = reloaded
        .get_object(shown_image(&reloaded, &annotation))
        .unwrap()
        .as_stream()
        .unwrap();
    assert_eq!(name_of(&image.dict, b"Filter"), "DCTDecode");
    assert_eq!(name_of(&image.dict, b"ColorSpace"), "DeviceRGB");
    assert_eq!(image.dict.get(b"Width").unwrap().as_i64().unwrap(), 32);
    assert_eq!(image.dict.get(b"Height").unwrap().as_i64().unwrap(), 16);
    assert_eq!(image.content, jpeg, "the jpeg bytes are kept exactly");

    let shown = render(&saved, 0);
    assert_near(shown.pixel(140.0, 150.0), [255, 0, 0], 40, "red half");
    assert_near(shown.pixel(260.0, 150.0), [0, 0, 255], 40, "blue half");
}

#[test]
fn resizing_a_picture_redraws_it_in_the_new_box() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let info = add_image(
        &mut doc,
        0,
        &picture(Rect::new(100.0, 100.0, 300.0, 100.0), three_stripe_png()),
    )
    .unwrap();
    let new_rect = Rect::new(50.0, 300.0, 150.0, 50.0);
    let changed = update(
        &mut doc,
        0,
        &info.id,
        &AnnotationChanges {
            rect: Some(new_rect),
            ..Default::default()
        },
    )
    .unwrap();
    assert!((changed.rect.x - 50.0).abs() < 0.01 && (changed.rect.width - 150.0).abs() < 0.01);
    let shown = render(&save(&mut doc), 0);
    assert_near(
        shown.pixel(75.0, 325.0),
        [255, 0, 0],
        10,
        "red, now smaller",
    );
    assert_near(shown.pixel(175.0, 325.0), [0, 0, 255], 10, "blue");
    assert_eq!(shown.pixel(150.0, 150.0), WHITE, "gone from the old place");
}

#[test]
fn files_that_are_not_pictures_are_refused() {
    let mut doc = load(&pdf_bytes(&[TestPage::letter()], false));
    let rect = Rect::new(100.0, 100.0, 100.0, 100.0);
    for bytes in [
        b"GIF89a....".to_vec(),
        vec![],
        b"\x89PNG\r\n\x1a\nbroken".to_vec(),
    ] {
        let error = add_image(&mut doc, 0, &picture(rect, bytes)).unwrap_err();
        assert!(
            matches!(error, AnnotateError::UnsupportedImage(_)),
            "{error:?}"
        );
        assert!(error.to_string().starts_with("This picture can’t be used."));
    }
    let empty_rect = Rect::new(100.0, 100.0, 0.0, 100.0);
    assert_eq!(
        add_image(&mut doc, 0, &picture(empty_rect, three_stripe_png())).unwrap_err(),
        AnnotateError::EmptyArea
    );
    assert!(page_annotations(&doc, 0).is_empty());
}
