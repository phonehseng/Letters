//! helpers shared by the integration tests: finding the engines, writing
//! test pdfs by hand, drawing their pages with our own renderer, and
//! measuring how well text was read.

// each test file uses only some of these helpers
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use image::{GrayImage, Luma};
use letters_ocr::{Engine, PageImage};
use letters_pdf::{
    page_pixel_size, tile_rect, DocumentId, PageSize, RenderWorker, TileKey, TILE_SIZE,
};

/// where tesseract lives for tests.
///
/// set LETTERS_TESSERACT_DIR to override; otherwise the folder filled by
/// tools/fetch-tesseract is used. tests that need it fail with this
/// message rather than quietly skipping.
pub fn tesseract_dir() -> PathBuf {
    let dir = match std::env::var_os("LETTERS_TESSERACT_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/tesseract"),
    };
    assert!(
        dir.join("tessdata").exists(),
        "tesseract was not found in {}. run tools/fetch-tesseract first.",
        dir.display()
    );
    dir
}

pub fn engine() -> Engine {
    Engine::locate(&tesseract_dir()).expect("run tools/fetch-tesseract first")
}

/// where the pdfium library lives for tests (see core/pdf/tests/support).
pub fn pdfium_dir() -> PathBuf {
    let dir = match std::env::var_os("LETTERS_PDFIUM_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium"),
    };
    assert!(
        dir.exists(),
        "pdfium was not found in {}. run tools/fetch-pdfium first.",
        dir.display()
    );
    dir
}

pub fn start_renderer() -> RenderWorker {
    RenderWorker::start(&pdfium_dir(), 64 * 1024 * 1024).unwrap()
}

/// a fresh, empty folder for one test's files.
pub fn temp_dir(test_name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("letters-ocr-tests")
        .join(format!("{test_name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// us letter, in points.
pub const PAGE_WIDTH: f32 = 612.0;
pub const PAGE_HEIGHT: f32 = 792.0;

/// the two built-in fonts the tests write with. courier has every letter
/// 0.6 of the font size wide, so a word's place can be worked out exactly.
#[derive(Clone, Copy, PartialEq)]
pub enum Font {
    Helvetica,
    Courier,
}

/// one line of text, placed by its baseline.
pub struct TextLine {
    pub x: f32,
    /// the baseline's distance from the top of the page, in points
    pub baseline_from_top: f32,
    pub size: f32,
    pub text: String,
}

impl TextLine {
    pub fn new(x: f32, baseline_from_top: f32, size: f32, text: &str) -> Self {
        Self {
            x,
            baseline_from_top,
            size,
            text: text.to_owned(),
        }
    }
}

/// a page for `write_text_pdf`: its lines, and how far its content is
/// turned anticlockwise around the page's centre (a crooked scan).
pub struct TextPage {
    pub lines: Vec<TextLine>,
    pub font: Font,
    pub tilt_degrees: f32,
}

/// writes a us-letter pdf of text pages with built-in fonts, by hand, so
/// the tests do not depend on any pdf writing library being correct.
pub fn write_text_pdf(path: &Path, pages: &[TextPage]) {
    let mut pdf = PdfWriter::default();
    let page_object = |page: usize| 5 + 2 * page;
    pdf.object(1, "<< /Type /Catalog /Pages 2 0 R >>".into());
    let kids: String = (0..pages.len())
        .map(|page| format!("{} 0 R ", page_object(page)))
        .collect();
    pdf.object(
        2,
        format!(
            "<< /Type /Pages /Count {} /Kids [ {kids}] /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] >>",
            pages.len()
        ),
    );
    pdf.object(
        3,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
    );
    pdf.object(
        4,
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>".into(),
    );
    for (index, page) in pages.iter().enumerate() {
        pdf.object(
            page_object(index),
            format!(
                "<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 3 0 R /F2 4 0 R >> >> >>",
                page_object(index) + 1
            ),
        );
        let content = page_content(page);
        pdf.object(
            page_object(index) + 1,
            format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ),
        );
    }
    fs::write(path, pdf.finish()).unwrap();
}

fn page_content(page: &TextPage) -> String {
    let font = match page.font {
        Font::Helvetica => "F1",
        Font::Courier => "F2",
    };
    let (sin, cos) = page.tilt_degrees.to_radians().sin_cos();
    let (cx, cy) = (PAGE_WIDTH / 2.0, PAGE_HEIGHT / 2.0);
    // turn around the page centre: move the centre to the origin, turn, move back
    let mut content = format!(
        "q {cos} {sin} {} {cos} {} {} cm\n",
        -sin,
        cx - cos * cx + sin * cy,
        cy - sin * cx - cos * cy
    );
    for line in &page.lines {
        content.push_str(&format!(
            "BT /{font} {} Tf {} {} Td <{}> Tj ET\n",
            line.size,
            line.x,
            PAGE_HEIGHT - line.baseline_from_top,
            winansi_hex(&line.text)
        ));
    }
    content.push_str("Q\n");
    content
}

/// the tests only use latin-1 letters, whose windows codes are the same.
fn winansi_hex(text: &str) -> String {
    text.chars()
        .map(|letter| {
            let code = letter as u32;
            assert!(code < 256, "test text must be latin-1: {letter}");
            format!("{code:02X}")
        })
        .collect()
}

/// builds the bytes of a pdf and remembers where each object starts.
#[derive(Default)]
struct PdfWriter {
    bytes: Vec<u8>,
    offsets: Vec<usize>,
}

impl PdfWriter {
    fn object(&mut self, number: usize, body: String) {
        if self.bytes.is_empty() {
            self.bytes.extend_from_slice(b"%PDF-1.7\n");
        }
        assert_eq!(
            number,
            self.offsets.len() + 1,
            "objects must be added in order"
        );
        self.offsets.push(self.bytes.len());
        self.bytes
            .extend_from_slice(format!("{number} 0 obj\n{body}\nendobj\n").as_bytes());
    }

    fn finish(mut self) -> Vec<u8> {
        let table_start = self.bytes.len();
        let object_count = self.offsets.len() + 1;
        let mut table = format!("xref\n0 {object_count}\n0000000000 65535 f \n");
        for offset in &self.offsets {
            table.push_str(&format!("{offset:010} 00000 n \n"));
        }
        table.push_str(&format!(
            "trailer\n<< /Size {object_count} /Root 1 0 R >>\nstartxref\n{table_start}\n%%EOF\n"
        ));
        self.bytes.extend_from_slice(table.as_bytes());
        self.bytes
    }
}

/// the scale our renderer uses for a resolution, and the resolution that
/// scale really gives (scales are whole thousandths).
pub fn scale_for(dpi: f32) -> (u32, f32) {
    let scale_milli = (dpi / 72.0 * 1000.0).round() as u32;
    (scale_milli, scale_milli as f32 / 1000.0 * 72.0)
}

/// draws a whole page with letters-pdf, tile by tile, as gray pixels.
pub fn render_page(
    worker: &RenderWorker,
    id: DocumentId,
    size: PageSize,
    page: u32,
    dpi: f32,
) -> PageImage {
    let (scale_milli, real_dpi) = scale_for(dpi);
    let (width, height) = page_pixel_size(size, scale_milli);
    let mut picture = GrayImage::new(width, height);
    for row in 0..height.div_ceil(TILE_SIZE) {
        for column in 0..width.div_ceil(TILE_SIZE) {
            let rect = tile_rect((width, height), column, row).unwrap();
            let key = TileKey {
                document: id,
                page,
                scale_milli,
                column,
                row,
            };
            let tile = decode_rgba(&worker.tile(key).unwrap());
            for y in 0..rect.height {
                for x in 0..rect.width {
                    let start = ((y * rect.width + x) * 4) as usize;
                    let [r, g, b] = [tile[start], tile[start + 1], tile[start + 2]];
                    let gray =
                        (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32).round() as u8;
                    picture.put_pixel(rect.x + x, rect.y + y, Luma([gray]));
                }
            }
        }
    }
    PageImage::from_gray(picture, real_dpi)
}

/// opens a pdf in a fresh renderer and draws one page.
pub fn render_file(path: &Path, page: u32, dpi: f32) -> PageImage {
    let worker = start_renderer();
    let info = worker.open(path, None).unwrap();
    let picture = render_page(&worker, info.id, info.page_sizes[page as usize], page, dpi);
    worker.close(info.id);
    picture
}

/// every tile of a page as png bytes, for comparing two drawings exactly.
pub fn page_tiles(path: &Path, page: u32, scale_milli: u32) -> Vec<Vec<u8>> {
    let worker = start_renderer();
    let info = worker.open(path, None).unwrap();
    let (width, height) = page_pixel_size(info.page_sizes[page as usize], scale_milli);
    let mut tiles = Vec::new();
    for row in 0..height.div_ceil(TILE_SIZE) {
        for column in 0..width.div_ceil(TILE_SIZE) {
            let key = TileKey {
                document: info.id,
                page,
                scale_milli,
                column,
                row,
            };
            tiles.push(decode_rgba(&worker.tile(key).unwrap()));
        }
    }
    worker.close(info.id);
    tiles
}

fn decode_rgba(png_bytes: &[u8]) -> Vec<u8> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    rgba.truncate(info.buffer_size());
    rgba
}

/// a gray picture as png bytes, the way a scanner might save it.
pub fn png_bytes(picture: &GrayImage, dpi: f32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, picture.width(), picture.height());
    encoder.set_color(png::ColorType::Grayscale);
    let per_metre = (dpi / 0.0254).round() as u32;
    encoder.set_pixel_dims(Some(png::PixelDimensions {
        xppu: per_metre,
        yppu: per_metre,
        unit: png::Unit::Meter,
    }));
    encoder
        .write_header()
        .unwrap()
        .write_image_data(picture.as_raw())
        .unwrap();
    bytes
}

/// how much of `expected` was read correctly, from 0 to 1: one minus the
/// number of single character edits between the texts, divided by the
/// length of the expected text. spacing differences are ignored.
pub fn character_accuracy(expected: &str, found: &str) -> f32 {
    let normalise = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let expected: Vec<char> = normalise(expected).chars().collect();
    let found: Vec<char> = normalise(found).chars().collect();
    let distance = edit_distance(&expected, &found);
    1.0 - distance as f32 / expected.len().max(1) as f32
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, letter_a) in a.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, letter_b) in b.iter().enumerate() {
            let substitution = previous[j] + usize::from(letter_a != letter_b);
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}

/// a repeatable stream of pseudo-random numbers, so noisy pictures are the
/// same on every run.
pub struct Noise(u64);

impl Noise {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// a number from 0 to 1.
    pub fn next(&mut self) -> f32 {
        // a 64 bit linear congruential generator (knuth's constants)
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// adds grain (every pixel nudged by up to `grain` levels) and dust
/// (`specks` share of pixels set to black or white), as a poor scanner would.
pub fn add_noise(picture: &mut GrayImage, grain: f32, specks: f32, seed: u64) {
    let mut noise = Noise::new(seed);
    for pixel in picture.pixels_mut() {
        let roll = noise.next();
        let value = if roll < specks / 2.0 {
            0.0
        } else if roll < specks {
            255.0
        } else {
            pixel.0[0] as f32 + (noise.next() - 0.5) * 2.0 * grain
        };
        pixel.0[0] = value.clamp(0.0, 255.0) as u8;
    }
}
