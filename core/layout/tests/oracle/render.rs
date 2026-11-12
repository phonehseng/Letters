//! renders pdf pages at 150 dpi with letters-pdf (the app's own renderer)
//! and compares two renderings: ssim and the share of pixels that differ,
//! so "barely noticeable" has a number.

use std::path::Path;

use letters_pdf::{page_pixel_size, RenderWorker, TileKey, TILE_SIZE};

/// a page as grey levels, one byte per pixel.
pub struct GreyPage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

const DPI: u32 = 150;

/// every page of a pdf as grey pixels at 150 dpi.
pub fn render(engine: &mut RenderWorker, path: &Path) -> Result<Vec<GreyPage>, String> {
    let info = engine.open(path, None).map_err(|e| format!("{e:?}"))?;
    let (id, sizes) = (info.id, info.page_sizes);
    let scale_milli = DPI * 1000 / 72;
    let mut pages = Vec::new();
    for (index, size) in sizes.iter().enumerate() {
        let (width, height) = page_pixel_size(*size, scale_milli);
        let mut pixels = vec![255u8; (width * height) as usize];
        for row in 0..height.div_ceil(TILE_SIZE) {
            for column in 0..width.div_ceil(TILE_SIZE) {
                let key = TileKey {
                    document: id,
                    page: index as u32,
                    scale_milli,
                    column,
                    row,
                };
                let Ok(png_bytes) = engine.tile(key) else {
                    continue;
                };
                paste_tile(
                    &png_bytes,
                    column * TILE_SIZE,
                    row * TILE_SIZE,
                    width,
                    &mut pixels,
                );
            }
        }
        pages.push(GreyPage {
            width,
            height,
            pixels,
        });
    }
    engine.close(id);
    Ok(pages)
}

fn paste_tile(png_bytes: &[u8], left: u32, top: u32, page_width: u32, pixels: &mut [u8]) {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let Ok(mut reader) = decoder.read_info() else {
        return;
    };
    let Some(size) = reader.output_buffer_size() else {
        return;
    };
    let mut buffer = vec![0; size];
    let Ok(info) = reader.next_frame(&mut buffer) else {
        return;
    };
    for y in 0..info.height {
        for x in 0..info.width {
            let at = ((y * info.width + x) * 4) as usize;
            let (r, g, b) = (
                u32::from(buffer[at]),
                u32::from(buffer[at + 1]),
                u32::from(buffer[at + 2]),
            );
            let grey = ((r * 299 + g * 587 + b * 114) / 1000) as u8;
            let target = ((top + y) * page_width + left + x) as usize;
            if target < pixels.len() {
                pixels[target] = grey;
            }
        }
    }
}

/// mean ssim over 8 by 8 windows, and the share of pixels differing by
/// more than a quarter of the grey range (anti-aliasing noise left out).
pub fn compare(a: &GreyPage, b: &GreyPage) -> (f64, f64) {
    let width = a.width.min(b.width) as usize;
    let height = a.height.min(b.height) as usize;
    let (c1, c2) = ((0.01f64 * 255.0).powi(2), (0.03f64 * 255.0).powi(2));
    let mut total = 0.0;
    let mut windows = 0usize;
    let mut differing = 0usize;
    let at =
        |page: &GreyPage, x: usize, y: usize| f64::from(page.pixels[y * page.width as usize + x]);
    for wy in (0..height.saturating_sub(7)).step_by(8) {
        for wx in (0..width.saturating_sub(7)).step_by(8) {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for y in wy..wy + 8 {
                for x in wx..wx + 8 {
                    let (pa, pb) = (at(a, x, y), at(b, x, y));
                    sa += pa;
                    sb += pb;
                    saa += pa * pa;
                    sbb += pb * pb;
                    sab += pa * pb;
                    if (pa - pb).abs() > 64.0 {
                        differing += 1;
                    }
                }
            }
            let n = 64.0;
            let (ma, mb) = (sa / n, sb / n);
            let (va, vb, cov) = (saa / n - ma * ma, sbb / n - mb * mb, sab / n - ma * mb);
            total += ((2.0 * ma * mb + c1) * (2.0 * cov + c2))
                / ((ma * ma + mb * mb + c1) * (va + vb + c2));
            windows += 1;
        }
    }
    let ssim = if windows == 0 {
        1.0
    } else {
        total / windows as f64
    };
    (ssim, differing as f64 / (width * height).max(1) as f64)
}

/// the mean comparison over the pages both documents have.
pub fn compare_documents(a: &[GreyPage], b: &[GreyPage]) -> Option<(f64, f64)> {
    let count = a.len().min(b.len());
    if count == 0 {
        return None;
    }
    let (mut ssim, mut diff) = (0.0, 0.0);
    for index in 0..count {
        let (s, d) = compare(&a[index], &b[index]);
        ssim += s;
        diff += d;
    }
    Some((ssim / count as f64, diff / count as f64))
}

/// the app's render worker, with pdfium from vendor/pdfium.
pub fn engine() -> Option<RenderWorker> {
    // the glyph reader binds pdfium first, so both share one library
    drop(super::pdf_glyphs::pdfium());
    RenderWorker::start(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/pdfium"),
        64 << 20,
    )
    .ok()
}
