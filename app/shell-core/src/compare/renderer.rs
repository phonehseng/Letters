//! drawing whole pages, and parts of pages, with the app's own pdf engine.
//!
//! the engine only draws tiles (see core/pdf). the comparison wants a
//! picture of a whole page, and the accessibility checker wants the
//! pixels under a piece of text, so both are stitched together here from
//! tiles. the tiles come from the shared cache when they are already there.

use letters_compare::RgbaImage;
use letters_pdf::{page_pixel_size, DocumentId, PageSize, RenderWorker, TileKey, TILE_SIZE};

/// draws page `page` (0-based) of an open document at `scale` pixels per point.
pub fn render_page(
    worker: &RenderWorker,
    document: DocumentId,
    size: PageSize,
    page: u32,
    scale: f32,
) -> Result<RgbaImage, String> {
    let scale_milli = scale_in_thousandths(scale);
    let (width, height) = page_pixel_size(size, scale_milli);
    let mut image = RgbaImage::white(width, height);
    for row in 0..height.div_ceil(TILE_SIZE) {
        for column in 0..width.div_ceil(TILE_SIZE) {
            let key = TileKey {
                document,
                page,
                scale_milli,
                column,
                row,
            };
            let png = worker.tile(key).map_err(|error| error.to_string())?;
            let tile = decode_png(&png)?;
            copy_tile(&mut image, &tile, column * TILE_SIZE, row * TILE_SIZE);
        }
    }
    Ok(image)
}

/// the scale as the engine names it: whole thousandths of a pixel per point.
pub fn scale_in_thousandths(scale: f32) -> u32 {
    ((scale * 1000.0).round() as u32).max(1)
}

/// copies a tile into the page picture at its place, cutting off what
/// falls outside (the last row and column of tiles are smaller anyway).
fn copy_tile(image: &mut RgbaImage, tile: &RgbaImage, left: u32, top: u32) {
    for y in 0..tile.height {
        let page_y = top + y;
        if page_y >= image.height {
            break;
        }
        for x in 0..tile.width {
            let page_x = left + x;
            if page_x < image.width {
                image.set_pixel(page_x, page_y, tile.pixel(x, y));
            }
        }
    }
}

/// reads a png the engine made (8-bit rgba, which is all it makes).
pub fn decode_png(bytes: &[u8]) -> Result<RgbaImage, String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
    let size = reader
        .output_buffer_size()
        .ok_or("the picture of a page is too big")?;
    let mut pixels = vec![0; size];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|error| error.to_string())?;
    pixels.truncate(info.buffer_size());
    let pixels = match info.color_type {
        png::ColorType::Rgba => pixels,
        // not expected from the engine, but cheap to accept
        png::ColorType::Rgb => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|&[r, g, b]| [r, g, b, 255])
            .collect(),
        other => return Err(format!("unexpected picture format {other:?}")),
    };
    Ok(RgbaImage {
        width: info.width,
        height: info.height,
        pixels,
    })
}

/// writes a picture as a png, for sending to the window as a `data:` address.
pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.width.max(1), image.height.max(1));
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        // speed over size: these pictures live for one screenful
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer
            .write_image_data(&image.pixels)
            .map_err(|error| error.to_string())?;
    }
    Ok(bytes)
}

/// a picture at most `max_pixels` big, so a huge page can't take all the memory.
pub fn safe_scale(size: PageSize, scale: f32, max_pixels: f32) -> f32 {
    let area = (size.width_pt * size.height_pt).max(1.0);
    let largest = (max_pixels / area).sqrt();
    scale.clamp(0.05, largest.max(0.05))
}
