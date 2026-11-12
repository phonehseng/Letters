//! images: covered pixels are replaced in the image data itself, images
//! fully under a box are removed, and images that can't be decoded are
//! removed whole.

mod support;

use letters_redact::lopdf::{Document, Object, Stream};
use letters_redact::{redact, save_clean, PageRect, RedactOptions, RedactionArea};
use support::*;

const SIZE: usize = 40;

/// a pixel pattern nobody would draw by accident.
fn pattern_pixel(x: usize, y: usize) -> [u8; 3] {
    [
        (x * 6 + 1) as u8,
        (y * 6 + 2) as u8,
        ((x * 7 + y * 3) % 251) as u8,
    ]
}

fn pattern_rgb(size: usize) -> Vec<u8> {
    (0..size)
        .flat_map(|y| (0..size).flat_map(move |x| pattern_pixel(x, y)))
        .collect()
}

/// rows filtered with the png "sub" predictor, so redaction has to undo
/// a predictor before it can see the pixels.
fn png_sub_rows(rgb: &[u8], width: usize) -> Vec<u8> {
    let row_bytes = width * 3;
    let mut out = Vec::new();
    for row in rgb.chunks(row_bytes) {
        out.push(1);
        for (index, byte) in row.iter().enumerate() {
            let left = if index >= 3 { row[index - 3] } else { 0 };
            out.push(byte.wrapping_sub(left));
        }
    }
    out
}

/// the image drawn as a 200 point square with its bottom left corner at
/// (100, 400) in page space.
fn image_page(pdf: &mut Pdf, image: usize) -> (usize, Vec<usize>) {
    let page = PageSpec::new(
        &format!("/XObject << /Im1 {image} 0 R >>"),
        "q 200 0 0 200 100 400 cm /Im1 Do Q",
    );
    add_pages(pdf, vec![page], "")
}

/// the left half of the image, in visible page coordinates.
fn left_half() -> Vec<RedactionArea> {
    vec![RedactionArea {
        page: 0,
        rects: vec![PageRect::new(100.0, 192.0, 100.0, 200.0)],
    }]
}

/// every image object in a saved file, decoded.
fn images_in(file: &[u8]) -> Vec<(Stream, Vec<u8>)> {
    let doc = Document::load_mem(file).unwrap();
    doc.objects
        .values()
        .filter_map(|object| object.as_stream().ok())
        .filter(|stream| {
            stream
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|s| s.as_name().ok())
                == Some(b"Image")
        })
        .map(|stream| {
            (
                stream.clone(),
                stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone()),
            )
        })
        .collect()
}

fn flate_image_pdf() -> Vec<u8> {
    let mut pdf = Pdf::new();
    let data = deflate(&png_sub_rows(&pattern_rgb(SIZE), SIZE));
    let image = pdf.add_stream(
        &format!("/Type /XObject /Subtype /Image /Width {SIZE} /Height {SIZE} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /DecodeParms << /Predictor 15 /Colors 3 /BitsPerComponent 8 /Columns {SIZE} >>"),
        &data,
    );
    let (catalog, _) = image_page(&mut pdf, image);
    pdf.finish(catalog, "")
}

#[test]
fn covered_pixels_of_a_flate_image_become_the_fill_colour() {
    let dir = temp_dir("flate-image");
    let worker = start_worker();
    let original = flate_image_pdf();
    let mut doc = Document::load_mem(&original).unwrap();
    let report = redact(&mut doc, &left_half(), &RedactOptions::default()).unwrap();
    assert_eq!(report.pages[0].images_rewritten, 1);
    let saved = save_clean(&mut doc).unwrap();

    // (e) the image in the saved file, decoded: left half black, right half as it was
    let images = images_in(&saved);
    assert_eq!(
        images.len(),
        1,
        "only the rewritten image is left in the file"
    );
    let pixels = &images[0].1;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let at = (y * SIZE + x) * 3;
            let expected = if x < SIZE / 2 {
                [0, 0, 0]
            } else {
                pattern_pixel(x, y)
            };
            assert_eq!(pixels[at..at + 3], expected, "pixel ({x}, {y})");
        }
    }
    // the original pixel rows of the covered half are nowhere in the file
    let first_row_left: Vec<u8> = pattern_rgb(SIZE)[..SIZE / 2 * 3].to_vec();
    assert_not_in_bytes(&saved, &String::from_utf8_lossy(&first_row_left));
    assert!(!saved
        .windows(first_row_left.len())
        .any(|w| w == first_row_left.as_slice()));

    // (d) drawn: black under the box, unchanged elsewhere
    let before = worker
        .open(&write(&dir, "before.pdf", &original), None)
        .unwrap();
    let after = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    let old = render_page(&worker, before.id, 0, 1.0, (612.0, 792.0));
    let new = render_page(&worker, after.id, 0, 1.0, (612.0, 792.0));
    for y in 195..390 {
        for x in 103..197 {
            assert_eq!(new.pixel(x, y), [0, 0, 0], "drawn pixel ({x}, {y})");
        }
        for x in 205..298 {
            assert_eq!(
                new.pixel(x, y),
                old.pixel(x, y),
                "drawn pixel ({x}, {y}) changed"
            );
        }
    }
}

#[test]
fn an_image_fully_under_a_box_is_removed() {
    let mut doc = Document::load_mem(&flate_image_pdf()).unwrap();
    let areas = vec![RedactionArea {
        page: 0,
        rects: vec![PageRect::new(90.0, 180.0, 220.0, 220.0)],
    }];
    let report = redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    assert_eq!(report.pages[0].images_removed, 1);
    let saved = save_clean(&mut doc).unwrap();
    assert!(images_in(&saved).is_empty());
}

#[test]
fn covered_pixels_of_a_jpeg_become_the_fill_colour() {
    let dir = temp_dir("jpeg-image");
    let worker = start_worker();
    let size = 64usize;
    let mut jpeg = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut jpeg, 90);
    encoder
        .encode(
            &pattern_rgb(size),
            size as u16,
            size as u16,
            jpeg_encoder::ColorType::Rgb,
        )
        .unwrap();
    let mut pdf = Pdf::new();
    let image = pdf.add_stream(
        &format!("/Type /XObject /Subtype /Image /Width {size} /Height {size} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode"),
        &jpeg,
    );
    let (catalog, _) = image_page(&mut pdf, image);
    let original = pdf.finish(catalog, "");
    let mut doc = Document::load_mem(&original).unwrap();
    let report = redact(&mut doc, &left_half(), &RedactOptions::default()).unwrap();
    assert_eq!(report.pages[0].images_rewritten, 1);
    let saved = save_clean(&mut doc).unwrap();
    assert!(
        !saved.windows(10).any(|w| w == b"DCTDecode"),
        "the jpeg is gone from the file"
    );
    assert!(!saved
        .windows(jpeg.len().min(64))
        .any(|w| w == &jpeg[..jpeg.len().min(64)]));

    let images = images_in(&saved);
    assert_eq!(images.len(), 1);
    let mut decoder = jpeg_decoder::Decoder::new(jpeg.as_slice());
    let decoded_original = decoder.decode().unwrap();
    let pixels = &images[0].1;
    for y in 0..size {
        for x in 0..size {
            let at = (y * size + x) * 3;
            if x < size / 2 {
                assert_eq!(pixels[at..at + 3], [0, 0, 0], "pixel ({x}, {y})");
            } else {
                assert_eq!(
                    pixels[at..at + 3],
                    decoded_original[at..at + 3],
                    "pixel ({x}, {y}) changed"
                );
            }
        }
    }
    let after = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    let new = render_page(&worker, after.id, 0, 1.0, (612.0, 792.0));
    for y in 195..390 {
        for x in 103..197 {
            assert_eq!(new.pixel(x, y), [0, 0, 0], "drawn pixel ({x}, {y})");
        }
    }
}

#[test]
fn inline_images_are_rewritten_too() {
    let dir = temp_dir("inline-image");
    let worker = start_worker();
    // an 8 by 8 gray image whose bytes spell a recognisable run
    let data: Vec<u8> = (0..64u8).map(|value| 0x40 + value).collect();
    let mut content = b"q 160 0 0 160 100 400 cm BI /W 8 /H 8 /BPC 8 /CS /G ID ".to_vec();
    content.extend_from_slice(&data);
    content.extend_from_slice(b"\nEI Q");
    let mut pdf = Pdf::new();
    let mut page = PageSpec::new("", "");
    page.contents = vec![content];
    let (catalog, _) = add_pages(&mut pdf, vec![page], "");
    let original = pdf.finish(catalog, "");
    let mut doc = Document::load_mem(&original).unwrap();
    // the left half: columns 0 to 3 of every row
    let areas = vec![RedactionArea {
        page: 0,
        rects: vec![PageRect::new(100.0, 232.0, 80.0, 160.0)],
    }];
    let report = redact(&mut doc, &areas, &RedactOptions::default()).unwrap();
    assert_eq!(report.pages[0].images_rewritten, 1);
    let saved = save_clean(&mut doc).unwrap();
    // the first row's covered bytes, in any spelling, are gone
    assert_not_in_bytes(&saved, &String::from_utf8_lossy(&data[..4]));
    let row = |y: usize| String::from_utf8_lossy(&data[y * 8..y * 8 + 4]).into_owned();
    for y in 0..8 {
        assert_not_in_bytes(&saved, &row(y));
    }
    let after = worker
        .open(&write(&dir, "after.pdf", &saved), None)
        .unwrap();
    let new = render_page(&worker, after.id, 0, 1.0, (612.0, 792.0));
    for y in 235..390 {
        for x in 103..177 {
            assert_eq!(new.pixel(x, y), [0, 0, 0], "drawn pixel ({x}, {y})");
        }
    }
}

#[test]
fn soft_masks_lose_their_covered_pixels_too() {
    let mut pdf = Pdf::new();
    let mask: Vec<u8> = (0..SIZE * SIZE)
        .map(|index| (index % 200 + 30) as u8)
        .collect();
    let smask = pdf.add_flate_stream(
        &format!("/Type /XObject /Subtype /Image /Width {SIZE} /Height {SIZE} /ColorSpace /DeviceGray /BitsPerComponent 8"),
        &mask,
    );
    let image = pdf.add_flate_stream(
        &format!("/Type /XObject /Subtype /Image /Width {SIZE} /Height {SIZE} /ColorSpace /DeviceRGB /BitsPerComponent 8 /SMask {smask} 0 R"),
        &pattern_rgb(SIZE),
    );
    let (catalog, _) = image_page(&mut pdf, image);
    let mut doc = Document::load_mem(&pdf.finish(catalog, "")).unwrap();
    redact(&mut doc, &left_half(), &RedactOptions::default()).unwrap();
    let saved = save_clean(&mut doc).unwrap();
    let images = images_in(&saved);
    assert_eq!(images.len(), 2, "the new image and its new mask");
    let (_, new_mask) = images
        .iter()
        .find(|(stream, _)| {
            stream
                .dict
                .get(b"ColorSpace")
                .ok()
                .and_then(|c| c.as_name().ok())
                == Some(b"DeviceGray")
        })
        .unwrap();
    for y in 0..SIZE {
        for x in 0..SIZE {
            let expected = if x < SIZE / 2 {
                255
            } else {
                mask[y * SIZE + x]
            };
            assert_eq!(new_mask[y * SIZE + x], expected, "mask pixel ({x}, {y})");
        }
    }
}

#[test]
fn an_image_that_cannot_be_decoded_is_removed_whole() {
    let mut pdf = Pdf::new();
    let image = pdf.add_stream(
        "/Type /XObject /Subtype /Image /Width 40 /Height 40 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /JPXDecode",
        b"not really jpeg 2000 SECRETPIXELS",
    );
    let (catalog, _) = image_page(&mut pdf, image);
    let mut doc = Document::load_mem(&pdf.finish(catalog, "")).unwrap();
    let report = redact(&mut doc, &left_half(), &RedactOptions::default()).unwrap();
    assert_eq!(report.pages[0].images_removed, 1);
    assert!(report.pages[0]
        .removed_wholesale
        .iter()
        .any(|reason| reason.contains("JPEG 2000")));
    let saved = save_clean(&mut doc).unwrap();
    assert!(images_in(&saved).is_empty());
    assert_not_in_bytes(&saved, "SECRETPIXELS");
}

#[test]
fn an_image_shared_with_another_page_is_kept_there_and_reported() {
    let mut pdf = Pdf::new();
    let image = pdf.add_flate_stream(
        &format!("/Type /XObject /Subtype /Image /Width {SIZE} /Height {SIZE} /ColorSpace /DeviceRGB /BitsPerComponent 8"),
        &pattern_rgb(SIZE),
    );
    let resources = format!("/XObject << /Im1 {image} 0 R >>");
    let pages = vec![
        PageSpec::new(&resources, "q 200 0 0 200 100 400 cm /Im1 Do Q"),
        PageSpec::new(&resources, "q 200 0 0 200 100 400 cm /Im1 Do Q"),
    ];
    let (catalog, _) = add_pages(&mut pdf, pages, "");
    let mut doc = Document::load_mem(&pdf.finish(catalog, "")).unwrap();
    let report = redact(&mut doc, &left_half(), &RedactOptions::default()).unwrap();
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("page 2")),
        "{:?}",
        report.warnings
    );
    let saved = save_clean(&mut doc).unwrap();
    assert_eq!(images_in(&saved).len(), 2, "page 2 keeps the original");
    let _ = Object::Null;
}
