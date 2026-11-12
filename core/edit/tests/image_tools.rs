//! cropping, cutting, flipping and fading images, checked in pixels.

mod support;

use letters_edit::{
    crop_image, crop_image_pixels, flip_image, page_objects, replace_image, set_image_opacity,
    EditConsent, FlipDirection, ImageFit, Rect,
};
use support::{load, render, save, PageSetup, PdfBuilder};

/// a 4 x 2 picture, red on the left half and blue on the right, drawn
/// 200 by 100 points with its top left at (100, 100) on screen.
fn picture_pdf() -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    let red = [255u8, 0, 0];
    let blue = [0u8, 0, 255];
    let row: Vec<u8> = [red, red, blue, blue].concat();
    let pixels = [row.clone(), row].concat();
    let image = pdf.stream(
        "/Type /XObject /Subtype /Image /Width 4 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8",
        &pixels,
    );
    pdf.page(
        &PageSetup::letter(),
        &["q 200 0 0 100 100 592 cm /Im1 Do Q"],
        &format!("/XObject << /Im1 {image} 0 R >>"),
    );
    pdf.finish()
}

fn consent() -> EditConsent {
    EditConsent::default()
}

fn image_id(doc: &lopdf::Document) -> String {
    page_objects(doc, 0).unwrap().images[0].id.clone()
}

#[test]
fn resolution_at_the_placed_size_is_reported() {
    let doc = load(&picture_pdf());
    let image = &page_objects(&doc, 0).unwrap().images[0];
    // 4 pixels across 200 points (2.78 inches), 2 down 100 points
    assert!(
        (image.pixels_per_inch - 1.44).abs() < 0.01,
        "{}",
        image.pixels_per_inch
    );
    assert_eq!(image.opacity, 1.0);
    assert!(image.crop_rect.is_none());
}

#[test]
fn a_crop_hides_pixels_but_keeps_them() {
    let mut doc = load(&picture_pdf());
    let id = image_id(&doc);
    let keep = Rect::new(100.0, 100.0, 100.0, 100.0);
    let result = crop_image(&mut doc, 0, &id, keep, &consent()).unwrap();
    let image = &result.objects.images[0];
    assert_eq!(image.crop_rect, Some(keep));
    assert_eq!(image.pixel_width, 4, "every pixel stays in the file");
    let after = render(&save(&mut doc), 0);
    assert_eq!(after.pixel(150.0, 150.0), [255, 0, 0]);
    assert_eq!(
        after.pixel(250.0, 150.0),
        support::WHITE,
        "the right half is hidden"
    );

    // replacing the picture keeps the crop
    let jpeg = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/red-blue.jpg"
    ))
    .unwrap();
    let id = result.object_ids[0].clone();
    replace_image(&mut doc, 0, &id, &jpeg, ImageFit::Stretch).unwrap();
    let after = render(&save(&mut doc), 0);
    assert_eq!(
        after.pixel(250.0, 150.0),
        support::WHITE,
        "still cropped after replacing"
    );
    assert_ne!(after.pixel(150.0, 150.0), support::WHITE);
}

#[test]
fn crop_and_remove_hidden_pixels_rewrites_the_picture() {
    let bytes = picture_pdf();
    let mut doc = load(&bytes);
    let id = image_id(&doc);
    let keep = Rect::new(100.0, 100.0, 100.0, 100.0);
    let cropped = crop_image(&mut doc, 0, &id, keep, &consent()).unwrap();
    let clipped_pixels = render(&save(&mut doc), 0);
    let result = crop_image_pixels(&mut doc, 0, &cropped.object_ids[0], None, &consent()).unwrap();
    let image = result
        .objects
        .images
        .iter()
        .find(|image| image.id == result.object_ids[0])
        .unwrap();
    assert_eq!(
        (image.pixel_width, image.pixel_height),
        (2, 2),
        "only the kept pixels remain"
    );
    assert_eq!(image.rect, keep);
    let after = render(&save(&mut doc), 0);
    assert_eq!(
        after.differences_in(&clipped_pixels, (0, 0, 612, 792)),
        0,
        "it looks exactly like the crop"
    );
}

#[test]
fn flipping_swaps_the_sides() {
    let mut doc = load(&picture_pdf());
    let id = image_id(&doc);
    let result = flip_image(&mut doc, 0, &id, FlipDirection::Horizontal, &consent()).unwrap();
    assert_eq!(result.object_ids.len(), 1);
    let after = render(&save(&mut doc), 0);
    assert_eq!(after.pixel(150.0, 150.0), [0, 0, 255]);
    assert_eq!(after.pixel(250.0, 150.0), [255, 0, 0]);
}

#[test]
fn opacity_lets_the_page_show_through() {
    let mut doc = load(&picture_pdf());
    let id = image_id(&doc);
    let result = set_image_opacity(&mut doc, 0, &id, 0.5, &consent()).unwrap();
    assert_eq!(result.objects.images[0].opacity, 0.5);
    let after = render(&save(&mut doc), 0);
    let [r, g, b] = after.pixel(150.0, 150.0);
    assert!(
        r > 250 && (120..=135).contains(&g) && (120..=135).contains(&b),
        "{r} {g} {b}"
    );
    assert!(set_image_opacity(&mut doc, 0, &id, 1.5, &consent()).is_err());
}
