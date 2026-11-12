//! drawing whole pages for the converters, with the app's own pdf engine.
//!
//! the engine draws tiles of at most 512 pixels a side, as png. a whole
//! page is put together from its tiles. the converters open their own
//! copy of the file (sometimes a copy without text, for a background),
//! so each page is drawn from a fresh open of exactly that file.

use std::path::Path;
use std::sync::Arc;

use letters_convert::{ConvertError, PageRenderer, RgbaImage};
use letters_pdf::{page_pixel_size, RenderWorker, TileKey, TILE_SIZE};

/// a page drawn as rgba pixels.
pub struct DrawnPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// draws one page of the pdf file at `pdf` at `scale` pixels per point.
pub fn draw_page(
    worker: &RenderWorker,
    pdf: &Path,
    page: usize,
    scale: f32,
) -> Result<DrawnPage, String> {
    let info = worker.open(pdf, None).map_err(|error| error.to_string())?;
    let drawn = draw_open_page(worker, info.id, &info.page_sizes, page, scale);
    // closing waits until the engine has let go of the file
    worker.close(info.id);
    drawn
}

fn draw_open_page(
    worker: &RenderWorker,
    id: letters_pdf::DocumentId,
    sizes: &[letters_pdf::PageSize],
    page: usize,
    scale: f32,
) -> Result<DrawnPage, String> {
    let size = *sizes.get(page).ok_or("That page isn’t in the file.")?;
    let scale_milli = (scale * 1000.0).round().max(1.0) as u32;
    let (width, height) = page_pixel_size(size, scale_milli);
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for row in 0..height.div_ceil(TILE_SIZE) {
        for column in 0..width.div_ceil(TILE_SIZE) {
            let key = TileKey {
                document: id,
                page: page as u32,
                scale_milli,
                column,
                row,
            };
            let tile = worker.tile(key).map_err(|error| error.to_string())?;
            let tile = decode_png(&tile)?;
            copy_tile(&mut rgba, width, &tile, column * TILE_SIZE, row * TILE_SIZE);
        }
    }
    Ok(DrawnPage {
        width,
        height,
        rgba,
    })
}

/// a decoded tile, always rgba.
struct Tile {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

fn decode_png(bytes: &[u8]) -> Result<Tile, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::ALPHA);
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let mut buffer = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or("The page picture is too big.")?
    ];
    let frame = reader
        .next_frame(&mut buffer)
        .map_err(|error| error.to_string())?;
    buffer.truncate(frame.buffer_size());
    let rgba = match frame.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::GrayscaleAlpha => buffer
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|[gray, alpha]| [*gray, *gray, *gray, *alpha])
            .collect(),
        other => return Err(format!("unexpected tile colour type {other:?}")),
    };
    Ok(Tile {
        width: frame.width,
        height: frame.height,
        rgba,
    })
}

fn copy_tile(page: &mut [u8], page_width: u32, tile: &Tile, x: u32, y: u32) {
    let length = (tile.width * 4) as usize;
    for row in 0..tile.height {
        let from = (row * tile.width * 4) as usize;
        let to = (((y + row) * page_width + x) * 4) as usize;
        if to + length <= page.len() && from + length <= tile.rgba.len() {
            page[to..to + length].copy_from_slice(&tile.rgba[from..from + length]);
        }
    }
}

/// the convert engine's renderer, backed by the app's pdf engine.
pub struct WorkerRenderer {
    worker: RenderWorker,
}

impl WorkerRenderer {
    pub fn new(worker: RenderWorker) -> Self {
        Self { worker }
    }
}

impl PageRenderer for WorkerRenderer {
    fn render(&mut self, pdf: &Path, page: usize, scale: f32) -> Result<RgbaImage, ConvertError> {
        let drawn =
            draw_page(&self.worker, pdf, page, scale).map_err(ConvertError::RenderFailed)?;
        RgbaImage::new(drawn.width, drawn.height, drawn.rgba)
    }
}

/// the exact match engine's renderer: page 0 of a pdf given as bytes, at
/// a resolution in dots per inch. the pdf engine reads only files, so
/// the bytes go to a temporary file first.
pub fn exact_renderer(
    worker: RenderWorker,
    work_dir: std::path::PathBuf,
) -> Arc<letters_exact::RenderFn> {
    let counter = std::sync::atomic::AtomicU64::new(0);
    Arc::new(move |bytes: &[u8], dpi: f32| {
        let number = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::fs::create_dir_all(&work_dir).map_err(|error| error.to_string())?;
        let file = work_dir.join(format!("draw-{}-{number}.pdf", std::process::id()));
        std::fs::write(&file, bytes).map_err(|error| error.to_string())?;
        let drawn = draw_page(&worker, &file, 0, dpi / 72.0);
        let _ = std::fs::remove_file(&file);
        drawn.map(|page| letters_exact::RenderedPage {
            width: page.width,
            height: page.height,
            rgba: page.rgba,
        })
    })
}
