// shared by the fidelity and back-and-forth tests, which each use part of it
#![allow(dead_code)]

//! reads pdfs the same way for every application (powerpoint's,
//! libreoffice's and ours) with pdfium: where each glyph's pen origin
//! is, the boxes of drawn shapes and pictures, and the page as grey
//! pixels. differences between the applications then come from their
//! layout, not from the reader. (the glyph reading follows
//! core/layout/tests/oracle/pdf_glyphs.rs.)

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use pdfium_render::prelude::*;

#[derive(Clone, Debug)]
pub struct Glyph {
    pub ch: char,
    /// pen origin, points from the page's top left
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

#[derive(Clone, Debug, Default)]
pub struct Page {
    pub width: f64,
    pub height: f64,
    pub glyphs: Vec<Glyph>,
    /// boxes of filled or stroked paths and pictures: left, top, right, bottom
    pub boxes: Vec<[f64; 4]>,
    /// grey levels, `pixel_width` wide
    pub pixels: Vec<u8>,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

/// pages are rendered this many pixels wide for the ssim comparison
pub const RENDER_WIDTH: u32 = 640;

pub struct SharedPdfium(pub Pdfium);

// safety: every use goes through the mutex below, one thread at a time
unsafe impl Send for SharedPdfium {}

pub fn pdfium() -> MutexGuard<'static, SharedPdfium> {
    static PDFIUM: OnceLock<Mutex<SharedPdfium>> = OnceLock::new();
    PDFIUM
        .get_or_init(|| {
            let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium");
            let library = Pdfium::pdfium_platform_library_name_at_path(&folder);
            let pdfium = match Pdfium::bind_to_library(&library) {
                Ok(bindings) => Pdfium::new(bindings),
                Err(_) => Pdfium::default(),
            };
            Mutex::new(SharedPdfium(pdfium))
        })
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn read_pdf(path: &Path) -> Result<Vec<Page>, String> {
    let library = pdfium();
    let document = library
        .0
        .load_pdf_from_file(path, None)
        .map_err(|error| format!("{}: {error:?}", path.display()))?;
    let mut pages = Vec::new();
    for page in document.pages().iter() {
        pages.push(read_page(&page));
    }
    Ok(pages)
}

fn read_page(page: &PdfPage<'_>) -> Page {
    let height = f64::from(page.height().value);
    let mut out = Page {
        width: f64::from(page.width().value),
        height,
        ..Default::default()
    };
    out.glyphs = glyphs(page, height);
    out.boxes = boxes(page, height, out.width);
    render(page, &mut out);
    out
}

fn glyphs(page: &PdfPage<'_>, height: f64) -> Vec<Glyph> {
    let mut found = Vec::new();
    let Ok(text) = page.text() else {
        return found;
    };
    for character in text.chars().iter() {
        if character.is_generated().unwrap_or(false) {
            continue;
        }
        let Some(ch) = character.unicode_char() else {
            continue;
        };
        if ch.is_whitespace() || ch.is_control() {
            continue;
        }
        let Ok((x, y)) = character.origin() else {
            continue;
        };
        found.push(Glyph {
            ch,
            x: f64::from(x.value),
            y: height - f64::from(y.value),
            size: f64::from(character.scaled_font_size().value),
        });
    }
    found
}

/// boxes of paths and pictures, leaving out page-sized backgrounds.
fn boxes(page: &PdfPage<'_>, height: f64, width: f64) -> Vec<[f64; 4]> {
    let mut found = Vec::new();
    for object in page.objects().iter() {
        if !matches!(
            object.object_type(),
            PdfPageObjectType::Path | PdfPageObjectType::Image
        ) {
            continue;
        }
        let Ok(bounds) = object.bounds() else {
            continue;
        };
        let rect = [
            f64::from(bounds.left().value),
            height - f64::from(bounds.top().value),
            f64::from(bounds.right().value),
            height - f64::from(bounds.bottom().value),
        ];
        let covers_page = rect[2] - rect[0] > width * 0.98 && rect[3] - rect[1] > height * 0.98;
        if !covers_page {
            found.push(rect);
        }
    }
    found
}

fn render(page: &PdfPage<'_>, out: &mut Page) {
    let config = PdfRenderConfig::new().set_target_width(RENDER_WIDTH as i32);
    let Ok(bitmap) = page.render_with_config(&config) else {
        return;
    };
    let (width, height) = (bitmap.width() as u32, bitmap.height() as u32);
    let raw = bitmap.as_raw_bytes();
    let mut grey = Vec::with_capacity((width * height) as usize);
    for pixel in raw.chunks(4) {
        // pdfium hands back bgra
        let value =
            0.114 * f64::from(pixel[0]) + 0.587 * f64::from(pixel[1]) + 0.299 * f64::from(pixel[2]);
        grey.push(value.round() as u8);
    }
    out.pixels = grey;
    out.pixel_width = width;
    out.pixel_height = height;
}

/// writes grey pixels as a png, for looking at differences.
pub fn save_png(page: &Page, path: &Path) {
    let Ok(file) = std::fs::File::create(path) else {
        return;
    };
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        page.pixel_width,
        page.pixel_height,
    );
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    if let Ok(mut writer) = encoder.write_header() {
        let _ = writer.write_image_data(&page.pixels);
    }
}
