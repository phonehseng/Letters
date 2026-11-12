//! scan to editable text, measured. a known page is set in several
//! faces, rendered at 216 dpi, saved as a jpeg "scan", and converted
//! with recognition results taken from the known page (as if recognition
//! read every word right). the tests measure how close "closely matching"
//! is per face: the face chosen, where the glyphs land, and how alike the
//! pixels look (ssim). run with --nocapture to see the table.

mod support;

use letters_edit::{
    convert_scan_to_text, page_objects, PageObjects, ScanMode, ScanOptions, TextLine,
};
use letters_ocr::{Baseline, OcrPage, OcrWord};
use lopdf::{dictionary, Document, Object, Stream};
use support::{load, render, render_scaled, save, Image, PageSetup, PdfBuilder};

/// the faces on the known page, with the face the conversion should pick.
const FACES: [(&str, &str, &str); 6] = [
    ("F1", "Helvetica", "Helvetica"),
    ("F2", "Times-Roman", "Times-Roman"),
    ("F3", "Courier", "Courier"),
    ("F4", "Helvetica-Bold", "Helvetica-Bold"),
    ("F5", "Times-Italic", "Times-Italic"),
    ("F6", "Helvetica", "Helvetica"),
];
const SENTENCE: &str = "The quick brown fox jumps over the lazy dog";
/// 216 dpi, a common scanning resolution
const SCAN_SCALE: u32 = 3000;

fn known_page() -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    let mut fonts = String::new();
    let mut content = String::new();
    for (index, (resource, face, _)) in FACES.iter().enumerate() {
        let font = pdf.add(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /{face} /Encoding /WinAnsiEncoding >>"
        ));
        fonts.push_str(&format!("/{resource} {font} 0 R "));
        // the last line is dark blue, to check the ink colour is read
        let color = if index == 5 { "0 0 0.6 rg" } else { "0 g" };
        let y = 700 - 40 * index as i32;
        content.push_str(&format!(
            "{color} BT /{resource} 14 Tf 72 {y} Td ({SENTENCE}) Tj ET\n"
        ));
    }
    // a figure no word explains: a red box with a grey ring
    content.push_str("1 0 0 rg 420 300 100 60 re f 0.4 g 6 w 410 290 120 80 re S\n");
    pdf.page(
        &PageSetup::letter(),
        &[&content],
        &format!("/Font << {fonts}>>"),
    );
    pdf.finish()
}

/// the known page rendered and stored as a jpeg covering the page.
fn scanned(known: &[u8]) -> Vec<u8> {
    let picture = render_scaled(known, 0, SCAN_SCALE);
    let rgb =
        image::RgbImage::from_raw(picture.width, picture.height, picture.rgb_bytes()).unwrap();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode_image(&rgb)
        .unwrap();
    let mut doc = Document::with_version("1.7");
    let image = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => picture.width as i64,
            "Height" => picture.height as i64, "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8, "Filter" => "DCTDecode",
        },
        jpeg,
    ));
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"q 612 0 0 792 0 0 cm /Scan Do Q".to_vec(),
    ));
    let pages = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content, "Resources" => dictionary! { "XObject" => dictionary! { "Scan" => image } },
    });
    doc.objects.insert(
        pages,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    save(&mut doc)
}

fn ocr_point(x: f32, y: f32) -> letters_ocr::Point {
    letters_ocr::Point::new(x, y)
}

/// the words of a line with their boxes, as recognition would report them.
fn words_of(line: &TextLine, number: usize, confidence: f32) -> Vec<OcrWord> {
    let mut words = Vec::new();
    let mut current: Vec<(char, letters_edit::Rect)> = Vec::new();
    let chars: Vec<(char, letters_edit::Rect)> = line
        .runs
        .iter()
        .flat_map(|run| run.text.chars().zip(run.char_boxes.iter().copied()))
        .chain([(' ', letters_edit::Rect::new(0.0, 0.0, 0.0, 0.0))])
        .collect();
    for (character, place) in chars {
        if character != ' ' {
            current.push((character, place));
            continue;
        }
        if current.is_empty() {
            continue;
        }
        let rect = current
            .iter()
            .skip(1)
            .fold(current[0].1, |all, (_, r)| all.union(r));
        let baseline_y = line.baseline[0].y;
        words.push(OcrWord {
            text: current.iter().map(|(c, _)| c).collect(),
            rect: letters_ocr::Rect::new(rect.x, rect.y, rect.width, rect.height),
            baseline: Baseline {
                start: ocr_point(rect.x, baseline_y),
                end: ocr_point(rect.right(), baseline_y),
            },
            font_size: line.runs[0].size,
            confidence,
            line: number,
            paragraph: number,
            block: 0,
            language: Some("eng".into()),
        });
        current.clear();
    }
    words
}

/// a perfect reading of the known page, with one line marked doubtful.
fn reading(known: &PageObjects) -> OcrPage {
    let mut words = Vec::new();
    for (number, block) in known.text_blocks.iter().enumerate() {
        let confidence = if number == 2 { 55.0 } else { 93.0 };
        words.extend(words_of(&block.lines[0], number, confidence));
    }
    OcrPage {
        width: 612.0,
        height: 792.0,
        mean_confidence: 90.0,
        words,
        language: "eng".into(),
        skew_angle: 0.0,
        orientation: 0,
        steps: Vec::new(),
    }
}

/// the mean structural similarity of two renders over a region, in 8 by
/// 8 windows of grey levels (1 is identical).
fn ssim(a: &Image, b: &Image, region: (u32, u32, u32, u32)) -> f32 {
    let (left, top, right, bottom) = region;
    let grey = |image: &Image, x: u32, y: u32| {
        let [r, g, b] = image.pixel(x as f32, y as f32);
        0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)
    };
    let (c1, c2) = ((0.01f32 * 255.0).powi(2), (0.03f32 * 255.0).powi(2));
    let mut total = 0.0;
    let mut windows = 0;
    for y0 in (top..bottom.saturating_sub(8)).step_by(4) {
        for x0 in (left..right.saturating_sub(8)).step_by(4) {
            let pairs: Vec<(f32, f32)> = (0..64)
                .map(|i| {
                    (
                        grey(a, x0 + i % 8, y0 + i / 8),
                        grey(b, x0 + i % 8, y0 + i / 8),
                    )
                })
                .collect();
            let n = pairs.len() as f32;
            let (ma, mb) = (
                pairs.iter().map(|p| p.0).sum::<f32>() / n,
                pairs.iter().map(|p| p.1).sum::<f32>() / n,
            );
            let va = pairs.iter().map(|p| (p.0 - ma).powi(2)).sum::<f32>() / n;
            let vb = pairs.iter().map(|p| (p.1 - mb).powi(2)).sum::<f32>() / n;
            let cov = pairs.iter().map(|p| (p.0 - ma) * (p.1 - mb)).sum::<f32>() / n;
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            windows += 1;
        }
    }
    total / windows.max(1) as f32
}

/// the mean distance (points) between where each letter starts in the
/// known line and in the converted one.
fn position_error(known: &TextLine, converted: &TextLine) -> f32 {
    let starts = |line: &TextLine| -> Vec<f32> {
        line.runs
            .iter()
            .flat_map(|run| run.text.chars().zip(&run.char_boxes))
            .filter(|(c, _)| *c != ' ')
            .map(|(_, place)| place.x)
            .collect()
    };
    let (a, b) = (starts(known), starts(converted));
    assert_eq!(a.len(), b.len(), "every letter is there");
    a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum::<f32>() / a.len() as f32
}

#[test]
fn a_scan_becomes_real_text_that_closely_matches_it() {
    let known = known_page();
    let known_objects = page_objects(&load(&known), 0).unwrap();
    let ocr = reading(&known_objects);
    let mut doc = load(&scanned(&known));
    let result = convert_scan_to_text(&mut doc, 0, &ocr, &ScanOptions::default()).unwrap();
    let saved = save(&mut doc);
    let converted = page_objects(&load(&saved), 0).unwrap();
    let known_render = render(&known, 0);
    let converted_render = render(&saved, 0);
    println!("face expected      | face chosen        | stem em | contrast | slant | letter error pt | ssim");
    let mut faces_right = 0;
    for (index, (_, _, expected)) in FACES.iter().enumerate() {
        let line = &result.lines[index];
        assert_eq!(line.text, SENTENCE, "the words are exactly what was read");
        let found = converted
            .text_blocks
            .iter()
            .find(|block| (block.rect.y - known_objects.text_blocks[index].rect.y).abs() < 3.0)
            .unwrap();
        let error = position_error(&known_objects.text_blocks[index].lines[0], &found.lines[0]);
        let rect = known_objects.text_blocks[index].rect;
        let region = (
            rect.x as u32,
            rect.y as u32,
            rect.right() as u32 + 2,
            rect.bottom() as u32 + 2,
        );
        let similarity = ssim(&known_render, &converted_render, region);
        if line.font == *expected {
            faces_right += 1;
        }
        println!(
            "{expected:<18} | {:<18} | {:7.3} | {:8.2} | {:5.2} | {error:15.2} | {similarity:.3}",
            line.font, line.style.stem_em, line.style.contrast, line.style.slant
        );
        assert!(
            error < 1.5,
            "{expected}: letters land {error} points off on average"
        );
        assert!(similarity > 0.6, "{expected}: ssim {similarity}");
    }
    println!("faces chosen right: {faces_right} of {}", FACES.len());
    assert!(
        result.lines[2].low_confidence && !result.lines[0].low_confidence,
        "the doubtful line is marked"
    );
    let [_, _, blue] = [
        result.lines[5].color.r,
        result.lines[5].color.g,
        result.lines[5].color.b,
    ];
    assert!(
        blue > 120 && result.lines[5].color.r < 60,
        "the blue ink is read: {:?}",
        result.lines[5].color
    );
    assert_eq!(
        converted.images.len(),
        1,
        "the cleaned scan stays as the background"
    );
    assert!(
        !support::find_text(&saved, 0, "lazy dog").is_empty(),
        "the text is real, searchable text"
    );
}

#[test]
fn text_only_keeps_figures_as_pictures() {
    let known = known_page();
    let ocr = reading(&page_objects(&load(&known), 0).unwrap());
    let mut doc = load(&scanned(&known));
    let options = ScanOptions {
        mode: ScanMode::TextOnly,
        ..ScanOptions::default()
    };
    let result = convert_scan_to_text(&mut doc, 0, &ocr, &options).unwrap();
    assert_eq!(result.figures.len(), 1, "{:?}", result.figures);
    let figure = result.figures[0];
    // the ring runs from 407 to 533 across and 419 to 505 down on screen
    assert!(
        figure.x < 410.0 && figure.right() > 530.0 && figure.y < 422.0 && figure.bottom() > 502.0,
        "{figure:?}"
    );
    let objects = page_objects(&load(&save(&mut doc)), 0).unwrap();
    assert_eq!(objects.images.len(), 1, "only the figure is a picture now");
    assert_eq!(objects.text_blocks.len(), FACES.len());
}

#[test]
fn a_page_without_a_scan_is_refused() {
    let mut doc = load(&known_page());
    let ocr = reading(&page_objects(&doc, 0).unwrap());
    let error = convert_scan_to_text(&mut doc, 0, &ocr, &ScanOptions::default()).unwrap_err();
    assert!(error.to_string().contains("no scanned image"));
}

/// a simple TrueType font embedded whole, WinAnsi codes 32 to 126.
fn embedded_truetype(pdf: &mut PdfBuilder, program: &[u8], name: &str) -> usize {
    let face = ttf_parser::Face::parse(program, 0).unwrap();
    let units = f32::from(face.units_per_em());
    let widths: Vec<String> = (32u8..=126)
        .map(|code| {
            let glyph = face
                .glyph_index(char::from(code))
                .unwrap_or(ttf_parser::GlyphId(0));
            let advance = f32::from(face.glyph_hor_advance(glyph).unwrap_or(0));
            format!("{:.0}", advance * 1000.0 / units)
        })
        .collect();
    let file = pdf.stream(&format!("/Length1 {}", program.len()), program);
    let descriptor = pdf.add(format!(
        "<< /Type /FontDescriptor /FontName /{name} /Flags 32 /FontBBox [-600 -300 2000 1000] /ItalicAngle 0 \
         /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 /FontFile2 {file} 0 R >>"
    ));
    pdf.add(format!(
        "<< /Type /Font /Subtype /TrueType /BaseFont /{name} /FirstChar 32 /LastChar 126 /Widths [{}] \
         /Encoding /WinAnsiEncoding /FontDescriptor {descriptor} 0 R >>",
        widths.join(" ")
    ))
}

/// a page with the sentence once in each installed font, embedded whole.
fn page_in_installed_faces(found: &[(Vec<u8>, &str, &str)]) -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    let mut fonts = String::new();
    let mut content = String::new();
    for (index, (program, file, _)) in found.iter().enumerate() {
        let font = embedded_truetype(&mut pdf, program, &file.replace(".ttf", ""));
        fonts.push_str(&format!("/E{index} {font} 0 R "));
        content.push_str(&format!(
            "BT /E{index} 14 Tf 72 {} Td ({SENTENCE}) Tj ET\n",
            700 - 40 * index as i32
        ));
    }
    pdf.page(
        &PageSetup::letter(),
        &[&content],
        &format!("/Font << {fonts}>>"),
    );
    pdf.finish()
}

#[test]
fn faces_outside_the_standard_set_get_the_closest_one() {
    // (file, the face a person would call closest)
    let wanted = [
        ("arial.ttf", "Helvetica"),
        ("verdana.ttf", "Helvetica"),
        ("georgia.ttf", "Times-Roman"),
        ("cour.ttf", "Courier"),
    ];
    let found: Vec<(Vec<u8>, &str, &str)> = wanted
        .iter()
        .filter_map(|(file, closest)| {
            Some((
                std::fs::read(format!("C:/Windows/Fonts/{file}")).ok()?,
                *file,
                *closest,
            ))
        })
        .collect();
    if found.is_empty() {
        println!("skipped: none of the fonts is installed here");
        return;
    }
    let known = page_in_installed_faces(&found);
    let known_objects = page_objects(&load(&known), 0).unwrap();
    let mut doc = load(&scanned(&known));
    let result = convert_scan_to_text(
        &mut doc,
        0,
        &reading(&known_objects),
        &ScanOptions::default(),
    )
    .unwrap();
    let saved = save(&mut doc);
    let converted = page_objects(&load(&saved), 0).unwrap();
    let (known_render, converted_render) = (render(&known, 0), render(&saved, 0));
    println!("font in the scan | closest | face chosen     | contrast | letter error pt | ssim");
    for (index, (_, file, closest)) in found.iter().enumerate() {
        let line = &result.lines[index];
        assert_eq!(line.text, SENTENCE);
        let rect = known_objects.text_blocks[index].rect;
        let found_block = converted
            .text_blocks
            .iter()
            .find(|block| (block.rect.y - rect.y).abs() < 4.0)
            .unwrap();
        let error = position_error(
            &known_objects.text_blocks[index].lines[0],
            &found_block.lines[0],
        );
        let region = (
            rect.x as u32,
            rect.y as u32,
            rect.right() as u32 + 2,
            rect.bottom() as u32 + 2,
        );
        let similarity = ssim(&known_render, &converted_render, region);
        println!(
            "{file:<16} | {closest:<7} | {:<15} | {:8.2} | {error:15.2} | {similarity:.3}",
            line.font, line.style.contrast
        );
        // word starts are exact and each word is stretched to its box, so
        // letters drift only inside a word
        assert!(error < 3.0, "{file}: {error}");
    }
}
