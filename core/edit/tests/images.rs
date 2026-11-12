//! images as page content: adding, replacing, moving, resizing, turning,
//! deleting, saving out and restacking.

mod support;

use letters_edit::{
    add_image, bring_image_to_front, delete_image, extract_image, move_image, page_objects,
    replace_image, resize_image, rotate_image, send_image_to_back, EditError, ImageFit, Rect,
};
use support::*;

/// a 30 by 10 png: red, see-through, then blue.
fn stripes_png() -> Vec<u8> {
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
    png_bytes(30, 10, png::ColorType::Rgba, &pixels)
}

fn solid_png(color: [u8; 3]) -> Vec<u8> {
    png_bytes(4, 4, png::ColorType::Rgb, &color.repeat(16))
}

fn png_bytes(width: u32, height: u32, color: png::ColorType, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(pixels).unwrap();
    writer.finish().unwrap();
    bytes
}

fn jpeg() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/red-blue.jpg"
    ))
    .unwrap()
}

fn blank_page(setup: PageSetup) -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    pdf.page(
        &setup,
        &["BT /F1 12 Tf 72 72 Td (Some text) Tj ET"],
        "/Font << /F1 3 0 R >>",
    );
    pdf.finish()
}

fn near(actual: [u8; 3], expected: [u8; 3], tolerance: u8) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= tolerance)
}

#[test]
fn added_pictures_land_upright_where_asked_on_every_layout() {
    for (layout, setup) in layouts() {
        let mut doc = load(&blank_page(setup));
        let rect = Rect::new(100.0, 100.0, 300.0, 100.0);
        let result = add_image(&mut doc, 0, rect, &stripes_png()).unwrap();
        let image = result
            .objects
            .images
            .iter()
            .find(|image| Some(&image.id) == result.image_id.as_ref())
            .expect("the new image is listed");
        assert!(
            (image.rect.x - 100.0).abs() < 0.05 && (image.rect.width - 300.0).abs() < 0.05,
            "{layout}: {:?}",
            image.rect
        );
        assert_eq!((image.pixel_width, image.pixel_height), (30, 10));
        assert!(
            image.has_alpha,
            "{layout}: the see-through stripe is a soft mask"
        );
        let saved = save(&mut doc);
        let shown = render(&saved, 0);
        assert!(
            near(shown.pixel(150.0, 150.0), [255, 0, 0], 10),
            "{layout}: red on the left"
        );
        assert_eq!(
            shown.pixel(250.0, 150.0),
            WHITE,
            "{layout}: the middle shows the page"
        );
        assert!(
            near(shown.pixel(350.0, 150.0), [0, 0, 255], 10),
            "{layout}: blue on the right"
        );
        assert_eq!(
            find_text(&saved, 0, "Some text").len(),
            1,
            "{layout}: the page's text is still there"
        );
    }
}

#[test]
fn jpegs_are_saved_out_exactly_as_they_went_in() {
    let mut doc = load(&blank_page(PageSetup::letter()));
    let result = add_image(&mut doc, 0, Rect::new(100.0, 100.0, 64.0, 32.0), &jpeg()).unwrap();
    let id = result.image_id.unwrap();
    let image = result
        .objects
        .images
        .iter()
        .find(|image| image.id == id)
        .unwrap();
    assert_eq!(image.filter.as_deref(), Some("DCTDecode"));
    let reloaded = reload(&mut doc);
    let extracted = extract_image(&reloaded, 0, &id).unwrap();
    assert_eq!(extracted.format, "jpeg");
    assert_eq!(extracted.bytes, jpeg());
}

#[test]
fn pngs_are_saved_out_with_their_transparency() {
    let mut doc = load(&blank_page(PageSetup::letter()));
    let id = add_image(
        &mut doc,
        0,
        Rect::new(100.0, 100.0, 30.0, 10.0),
        &stripes_png(),
    )
    .unwrap()
    .image_id
    .unwrap();
    let extracted = extract_image(&doc, 0, &id).unwrap();
    assert_eq!(extracted.format, "png");
    let mut reader = png::Decoder::new(std::io::Cursor::new(&extracted.bytes[..]))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(
        (frame.width, frame.height, frame.color_type),
        (30, 10, png::ColorType::Rgba)
    );
    assert_eq!(&pixels[0..4], &[255, 0, 0, 255]);
    assert_eq!(pixels[15 * 4 + 3], 0, "the middle stays see-through");
    assert_eq!(&pixels[25 * 4..25 * 4 + 4], &[0, 0, 255, 255]);
}

/// a page drawing the same image object twice, at (100, 100) and (300, 100).
fn twice_drawn(setup: PageSetup, in_form: bool) -> Vec<u8> {
    let red: Vec<u8> = [255u8, 0, 0].repeat(4);
    let mut pdf = PdfBuilder::new();
    let image = pdf.stream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8", &red);
    let (resources, drawn) = if in_form {
        let form = pdf.stream(
            &format!("/Type /XObject /Subtype /Form /BBox [0 0 1 1] /Resources << /XObject << /Im1 {image} 0 R >> >>"),
            b"/Im1 Do",
        );
        (format!("/XObject << /Fm1 {form} 0 R >>"), "Fm1")
    } else {
        (format!("/XObject << /Im1 {image} 0 R >>"), "Im1")
    };
    let content =
        format!("q 100 0 0 100 100 592 cm /{drawn} Do Q q 100 0 0 100 300 592 cm /{drawn} Do Q");
    pdf.page(&setup, &[&content], &resources);
    pdf.finish()
}

#[test]
fn moving_one_drawing_of_a_shared_image_leaves_the_other() {
    for in_form in [false, true] {
        let mut doc = load(&twice_drawn(PageSetup::letter(), in_form));
        let objects = page_objects(&doc, 0).unwrap();
        assert_eq!(objects.images.len(), 2, "in form: {in_form}");
        let first = objects
            .images
            .iter()
            .find(|image| image.rect.x < 200.0)
            .unwrap();
        let result = move_image(&mut doc, 0, &first.id, 0.0, 200.0).unwrap();
        assert!(result.image_id.is_some());
        let rects: Vec<Rect> = result
            .objects
            .images
            .iter()
            .map(|image| image.rect)
            .collect();
        assert!(
            rects
                .iter()
                .any(|rect| (rect.x - 100.0).abs() < 0.05 && (rect.y - 300.0).abs() < 0.05),
            "in form {in_form}: {rects:?}"
        );
        assert!(
            rects
                .iter()
                .any(|rect| (rect.x - 300.0).abs() < 0.05 && (rect.y - 100.0).abs() < 0.05),
            "the other drawing stayed"
        );
        let shown = render(&save(&mut doc), 0);
        assert!(near(shown.pixel(150.0, 350.0), [255, 0, 0], 4), "moved");
        assert_eq!(shown.pixel(150.0, 150.0), WHITE, "gone from where it was");
        assert!(
            near(shown.pixel(350.0, 150.0), [255, 0, 0], 4),
            "the other one is untouched"
        );
    }
}

#[test]
fn resizing_and_turning_change_only_that_image() {
    let mut doc = load(&twice_drawn(PageSetup::letter().rotated(90), true));
    let objects = page_objects(&doc, 0).unwrap();
    let target = objects.images[0].clone();
    let other = objects.images[1].clone();
    let result = resize_image(
        &mut doc,
        0,
        &target.id,
        Rect::new(400.0, 300.0, 50.0, 150.0),
    )
    .unwrap();
    let resized = result
        .objects
        .images
        .iter()
        .find(|image| Some(&image.id) == result.image_id.as_ref())
        .unwrap();
    assert!((resized.rect.width - 50.0).abs() < 0.05 && (resized.rect.height - 150.0).abs() < 0.05);
    assert!(
        result
            .objects
            .images
            .iter()
            .any(|image| image.rect == other.rect),
        "the other drawing stayed"
    );

    let turned = rotate_image(&mut doc, 0, &resized.id, 90.0).unwrap();
    let image = turned
        .objects
        .images
        .iter()
        .find(|image| Some(&image.id) == turned.image_id.as_ref())
        .unwrap();
    // a quarter turn swaps the box's width and height around the centre
    assert!(
        (image.rect.width - 150.0).abs() < 0.05 && (image.rect.height - 50.0).abs() < 0.05,
        "{:?}",
        image.rect
    );
    // turned clockwise: the picture's top left corner is now at the top right
    assert!(image.quad[0].x > image.quad[3].x);
    let shown = render(&save(&mut doc), 0);
    assert!(near(shown.pixel(425.0, 375.0), [255, 0, 0], 4));
}

#[test]
fn replacing_a_picture_keeps_its_place() {
    let mut doc = load(&twice_drawn(PageSetup::letter(), false));
    let objects = page_objects(&doc, 0).unwrap();
    let target = objects.images[0].clone();
    let result = replace_image(
        &mut doc,
        0,
        &target.id,
        &solid_png([0, 160, 0]),
        ImageFit::Stretch,
    )
    .unwrap();
    let replaced = result
        .objects
        .images
        .iter()
        .find(|image| Some(&image.id) == result.image_id.as_ref())
        .unwrap();
    assert_eq!(replaced.rect, target.rect);
    assert_eq!((replaced.pixel_width, replaced.pixel_height), (4, 4));
    let shown = render(&save(&mut doc), 0);
    assert!(
        near(shown.pixel(150.0, 150.0), [0, 160, 0], 4),
        "the new picture"
    );
    assert!(
        near(shown.pixel(350.0, 150.0), [255, 0, 0], 4),
        "the other drawing still shows the old one"
    );

    // fitting a wide picture into the square keeps it wide, centred
    let id = replaced.id.clone();
    let wide = png_bytes(4, 2, png::ColorType::Rgb, &[0u8, 0, 255].repeat(8));
    let fitted = replace_image(&mut doc, 0, &id, &wide, ImageFit::Fit).unwrap();
    let image = fitted
        .objects
        .images
        .iter()
        .find(|image| Some(&image.id) == fitted.image_id.as_ref())
        .unwrap();
    assert!(
        (image.rect.width - 100.0).abs() < 0.05 && (image.rect.height - 50.0).abs() < 0.05,
        "{:?}",
        image.rect
    );
    assert!(
        (image.rect.y - 125.0).abs() < 0.05,
        "centred in the old place"
    );
}

#[test]
fn deleting_and_restacking() {
    // an image under a black square that overlaps its right half
    let red: Vec<u8> = [255u8, 0, 0].repeat(4);
    let mut pdf = PdfBuilder::new();
    let image = pdf.stream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceRGB /BitsPerComponent 8", &red);
    pdf.page(
        &PageSetup::letter(),
        &["q 100 0 0 100 100 592 cm /Im1 Do Q 0 g 150 592 100 100 re f"],
        &format!("/XObject << /Im1 {image} 0 R >>"),
    );
    let mut doc = load(&pdf.finish());
    let id = page_objects(&doc, 0).unwrap().images[0].id.clone();
    assert_eq!(
        render(&save(&mut doc), 0).pixel(175.0, 150.0),
        [0, 0, 0],
        "the square covers the image"
    );

    let front = bring_image_to_front(&mut doc, 0, &id).unwrap();
    assert!(
        near(
            render(&save(&mut doc), 0).pixel(175.0, 150.0),
            [255, 0, 0],
            4
        ),
        "now the image is on top"
    );
    let id = front.image_id.unwrap();
    let back = send_image_to_back(&mut doc, 0, &id).unwrap();
    assert_eq!(
        render(&save(&mut doc), 0).pixel(175.0, 150.0),
        [0, 0, 0],
        "and underneath again"
    );

    let id = back.image_id.unwrap();
    let deleted = delete_image(&mut doc, 0, &id).unwrap();
    assert!(deleted.objects.images.is_empty());
    let shown = render(&save(&mut doc), 0);
    assert_eq!(shown.pixel(120.0, 150.0), WHITE);
    assert_eq!(shown.pixel(175.0, 150.0), [0, 0, 0], "the square stays");
}

#[test]
fn bad_pictures_and_ids_change_nothing() {
    let mut doc = load(&twice_drawn(PageSetup::letter(), false));
    let before = save(&mut doc);
    let id = page_objects(&doc, 0).unwrap().images[0].id.clone();
    assert!(matches!(
        replace_image(&mut doc, 0, &id, b"GIF89a", ImageFit::Fit),
        Err(EditError::UnsupportedImage(_))
    ));
    assert!(matches!(
        add_image(
            &mut doc,
            0,
            Rect::new(0.0, 0.0, 0.0, 5.0),
            &solid_png([0, 0, 0])
        ),
        Err(EditError::InvalidValue(_))
    ));
    assert_eq!(
        move_image(&mut doc, 0, "image:nothing", 1.0, 1.0).unwrap_err(),
        EditError::UnknownObject
    );
    assert_eq!(save(&mut doc), before, "nothing changed");
}

#[test]
fn inline_images_are_listed_and_can_be_moved_but_not_saved_out() {
    let mut pdf = PdfBuilder::new();
    pdf.page(
        &PageSetup::letter(),
        &["q 50 0 0 50 100 642 cm BI /W 1 /H 1 /CS /RGB /BPC 8 ID \u{0}\u{0}\u{7f} EI Q"],
        "",
    );
    let mut doc = load(&pdf.finish());
    let image = page_objects(&doc, 0).unwrap().images[0].clone();
    assert!(image.inline);
    assert_eq!((image.pixel_width, image.pixel_height), (1, 1));
    assert!(matches!(
        extract_image(&doc, 0, &image.id),
        Err(EditError::CannotExtract(_))
    ));
    let moved = move_image(&mut doc, 0, &image.id, 100.0, 0.0).unwrap();
    assert!((moved.objects.images[0].rect.x - 200.0).abs() < 0.05);
    let shown = render(&save(&mut doc), 0);
    // the pixel is dark blue (0, 0, 127): content strings here are text, so ascii bytes only`n    assert!(near(shown.pixel(225.0, 125.0), [0, 0, 127], 4), "drawn in its new place");
    assert_eq!(shown.pixel(125.0, 125.0), WHITE);
}
