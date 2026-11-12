//! pictures to pdf through letters-ocr, with and without reading text.

mod support;

use std::path::{Path, PathBuf};

use letters_convert::*;
use support::*;

/// a png of one colour with its resolution stored in it.
fn write_png(path: &Path, width: u32, height: u32, dpi: f32) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let per_metre = (dpi / 0.0254).round() as u32;
    encoder.set_pixel_dims(Some(png::PixelDimensions {
        xppu: per_metre,
        yppu: per_metre,
        unit: png::Unit::Meter,
    }));
    let pixels = [40u8, 120, 200].repeat((width * height) as usize);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&pixels)
        .unwrap();
}

fn page_sizes(path: &Path) -> Vec<(f32, f32)> {
    let worker = letters_pdf::RenderWorker::start(&pdfium_dir(), 1024 * 1024).unwrap();
    let info = worker.open(path, None).unwrap();
    info.page_sizes
        .iter()
        .map(|s| (s.width_pt, s.height_pt))
        .collect()
}

#[test]
fn each_picture_becomes_a_page_of_its_size() {
    let dir = temp_dir("pictures");
    let first = dir.join("wide.png");
    let second = dir.join("square.png");
    write_png(&first, 300, 150, 150.0);
    write_png(&second, 200, 200, 100.0);
    let output = dir.join("Pictures.pdf");
    let mut seen = 0;
    images_to_pdf_file(
        &[first, second],
        &ImagesToPdfOptions::default(),
        None,
        &output,
        &mut |_| seen += 1,
        &CancelFlag::new(),
    )
    .unwrap();
    assert_eq!(seen, 2);
    // 300 pixels at 150 dpi is two inches, 144 points
    let sizes = page_sizes(&output);
    assert_eq!(sizes.len(), 2);
    assert!(
        (sizes[0].0 - 144.0).abs() < 0.5 && (sizes[0].1 - 72.0).abs() < 0.5,
        "{sizes:?}"
    );
    assert!(
        (sizes[1].0 - 144.0).abs() < 0.5 && (sizes[1].1 - 144.0).abs() < 0.5,
        "{sizes:?}"
    );
}

#[test]
fn no_pictures_and_unreadable_pictures_are_explained() {
    let dir = temp_dir("no-pictures");
    let output = dir.join("None.pdf");
    let none = images_to_pdf_file(
        &[],
        &ImagesToPdfOptions::default(),
        None,
        &output,
        &mut |_| {},
        &CancelFlag::new(),
    );
    assert_eq!(
        none.unwrap_err().to_string(),
        "There are no pictures to make a PDF from."
    );
    let broken = dir.join("broken.png");
    std::fs::write(&broken, b"not a picture").unwrap();
    let error = images_to_pdf_file(
        &[broken],
        &ImagesToPdfOptions::default(),
        None,
        &output,
        &mut |_| {},
        &CancelFlag::new(),
    )
    .unwrap_err();
    assert!(
        error.to_string().starts_with("This picture can’t be read."),
        "{error}"
    );
    assert!(!output.exists());
}

fn tesseract_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/tesseract")
}

#[test]
fn pictures_of_text_become_searchable() {
    let Ok(engine) = Engine::locate(&tesseract_dir()) else {
        panic!("tesseract was not found in vendor/tesseract. run tools/fetch-tesseract first.");
    };
    // a page of text drawn as a 300 dpi picture, then made into a pdf again
    let dir = temp_dir("ocr");
    let pdf = dir.join("text.pdf");
    let mut page = TestPage::sized(400.0, 120.0);
    page.text(Font::Helvetica, 20.0, 30.0, 60.0, "Searchable words here");
    write_pdf(&pdf, &[page]);
    let source = PdfSource::open(&pdf).unwrap();
    let options = ImageExportOptions {
        dpi: 300.0,
        ..Default::default()
    };
    let pictures = pdf_to_images(
        &source,
        &mut TestRenderer::new(),
        &options,
        &dir,
        "Scan",
        &mut |_| {},
        &CancelFlag::new(),
    )
    .unwrap();
    let output = dir.join("Scan.pdf");
    let ocr = OcrOptions {
        languages: vec!["eng".into()],
        ..Default::default()
    };
    images_to_pdf_file(
        &pictures,
        &ImagesToPdfOptions::default(),
        Some((&engine, &ocr)),
        &output,
        &mut |_| {},
        &CancelFlag::new(),
    )
    .unwrap();
    let text = pdf_pages_text(&output).join(" ");
    assert!(
        text.contains("Searchable") && text.contains("words"),
        "{text}"
    );
}
