//! drawing a page of an open document as one gray picture for recognition.
//!
//! the pdf worker draws pages as png tiles of at most 512 pixels, so a
//! page at 300 dpi is drawn tile by tile and the tiles are put together in
//! a gray picture (a letter page is about 8 MB that way; rgba would be 33).
//! the tiles go through the worker's normal queue and cache, so the page
//! on screen keeps being drawn in between, and a tile the queue skipped is
//! simply asked for again.

use std::thread;
use std::time::Duration;

use image::{GrayImage, Luma};
use letters_ocr::{CancelFlag, OcrError, PageImage};
use letters_pdf::{
    page_pixel_size, tile_rect, DocumentId, PageSize, PdfError, RenderWorker, TileKey, TILE_SIZE,
};

/// the resolution tesseract is tuned for.
pub const OCR_DPI: f32 = 300.0;

/// how often a skipped tile is asked for again before giving up.
const TILE_ATTEMPTS: u32 = 20;

/// the worker's scale for a resolution (whole thousandths of a pixel per
/// point) and the resolution that scale really gives.
pub fn scale_for(dpi: f32) -> (u32, f32) {
    let scale_milli = (dpi / 72.0 * 1000.0).round() as u32;
    (scale_milli, scale_milli as f32 / 1000.0 * 72.0)
}

/// one page drawn at about 300 dpi, as gray pixels.
pub fn render_page(
    worker: &RenderWorker,
    document: DocumentId,
    page: u32,
    size: PageSize,
    cancel: &CancelFlag,
) -> Result<PageImage, OcrError> {
    let (scale_milli, dpi) = scale_for(OCR_DPI);
    let (width, height) = page_pixel_size(size, scale_milli);
    let mut picture = GrayImage::from_pixel(width, height, Luma([255]));
    for row in 0..height.div_ceil(TILE_SIZE) {
        for column in 0..width.div_ceil(TILE_SIZE) {
            if cancel.is_cancelled() {
                return Err(OcrError::Cancelled);
            }
            let Some(rect) = tile_rect((width, height), column, row) else {
                continue;
            };
            let key = TileKey {
                document,
                page,
                scale_milli,
                column,
                row,
            };
            let png = tile_with_retries(worker, key)?;
            paste_tile(&mut picture, &png, rect.x, rect.y)?;
        }
    }
    Ok(PageImage::from_gray(picture, dpi))
}

fn tile_with_retries(worker: &RenderWorker, key: TileKey) -> Result<Vec<u8>, OcrError> {
    let mut last = String::new();
    for attempt in 0..TILE_ATTEMPTS {
        match worker.tile(key) {
            Ok(tile) => return Ok(tile.to_vec()),
            // a busy queue answers "skipped" to older requests: wait a moment
            Err(PdfError::Superseded) => {
                last = PdfError::Superseded.to_string();
                thread::sleep(Duration::from_millis(20 * u64::from(attempt + 1)));
            }
            Err(error) => return Err(OcrError::UnreadableImage(error.to_string())),
        }
    }
    Err(OcrError::UnreadableImage(last))
}

/// decodes a png tile and writes its gray levels into the page picture.
fn paste_tile(
    picture: &mut GrayImage,
    png_bytes: &[u8],
    left: u32,
    top: u32,
) -> Result<(), OcrError> {
    let unreadable =
        |detail: String| OcrError::UnreadableImage(format!("(a drawn tile: {detail})"));
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|error| unreadable(error.to_string()))?;
    let mut buffer = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| unreadable(error.to_string()))?;
    let channels = info.color_type.samples();
    for y in 0..info.height {
        for x in 0..info.width {
            let at = ((y * info.width + x) as usize) * channels;
            let gray = gray_of(&buffer[at..at + channels]);
            let (px, py) = (left + x, top + y);
            if px < picture.width() && py < picture.height() {
                picture.put_pixel(px, py, Luma([gray]));
            }
        }
    }
    Ok(())
}

/// a pixel's gray level, with see-through parts taken as white paper.
fn gray_of(sample: &[u8]) -> u8 {
    let (value, alpha) = match sample {
        [gray] => (f32::from(*gray), 255.0),
        [gray, alpha] => (f32::from(*gray), f32::from(*alpha)),
        [r, g, b] => (luma(*r, *g, *b), 255.0),
        [r, g, b, alpha, ..] => (luma(*r, *g, *b), f32::from(*alpha)),
        [] => (255.0, 255.0),
    };
    let on_white = value * alpha / 255.0 + 255.0 * (1.0 - alpha / 255.0);
    on_white.round().clamp(0.0, 255.0) as u8
}

fn luma(r: u8, g: u8, b: u8) -> f32 {
    0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)
}
