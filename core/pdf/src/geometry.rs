//! the arithmetic for cutting a page into tiles.
//!
//! kept free of any pdf library types so it can be tested on its own.

use crate::DocumentId;

/// width and height of a full tile in pixels.
///
/// 512 keeps a single tile around one megabyte before compression,
/// small enough to render quickly and big enough that a screen only
/// needs a handful of them.
pub const TILE_SIZE: u32 = 512;

/// the size of a page in points (1 point = 1/72 inch), as stored in the pdf.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSize {
    pub width_pt: f32,
    pub height_pt: f32,
}

/// a rectangle in page pixels, measured from the top left of the page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// names one tile of one page at one zoom level.
///
/// the scale is stored in thousandths of a pixel per point so the key
/// can be hashed and compared exactly (floats can't be).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileKey {
    pub document: DocumentId,
    pub page: u32,
    pub scale_milli: u32,
    pub column: u32,
    pub row: u32,
}

/// how many pixels a page covers at the given scale.
///
/// rounds up so the last sliver of the page is never cut off.
pub fn page_pixel_size(size: PageSize, scale_milli: u32) -> (u32, u32) {
    let scale = scale_milli as f32 / 1000.0;
    let width = (size.width_pt * scale).ceil().max(1.0) as u32;
    let height = (size.height_pt * scale).ceil().max(1.0) as u32;
    (width, height)
}

/// the pixel area a tile covers, or none if the tile lies outside the page.
///
/// tiles on the right and bottom edges are smaller than a full tile,
/// so no memory is spent on pixels beyond the page.
pub fn tile_rect(page_pixels: (u32, u32), column: u32, row: u32) -> Option<PixelRect> {
    let (page_width, page_height) = page_pixels;
    let x = column.checked_mul(TILE_SIZE)?;
    let y = row.checked_mul(TILE_SIZE)?;
    if x >= page_width || y >= page_height {
        return None;
    }
    Some(PixelRect {
        x,
        y,
        width: TILE_SIZE.min(page_width - x),
        height: TILE_SIZE.min(page_height - y),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTER: PageSize = PageSize {
        width_pt: 612.0,
        height_pt: 792.0,
    };

    #[test]
    fn page_pixels_follow_the_scale() {
        assert_eq!(page_pixel_size(LETTER, 1000), (612, 792));
        assert_eq!(page_pixel_size(LETTER, 2000), (1224, 1584));
    }

    #[test]
    fn page_pixels_round_up() {
        // 612 * 1.501 = 918.6, which must become 919 and not 918
        assert_eq!(page_pixel_size(LETTER, 1502).0, 920);
        assert_eq!(page_pixel_size(LETTER, 1501).0, 919);
    }

    #[test]
    fn inner_tiles_are_full_size() {
        let rect = tile_rect((1224, 1584), 1, 2).unwrap();
        assert_eq!(
            rect,
            PixelRect {
                x: 512,
                y: 1024,
                width: 512,
                height: 512
            }
        );
    }

    #[test]
    fn edge_tiles_are_trimmed_to_the_page() {
        let rect = tile_rect((1224, 1584), 2, 3).unwrap();
        assert_eq!(rect.width, 1224 - 1024);
        assert_eq!(rect.height, 1584 - 1536);
    }

    #[test]
    fn tiles_outside_the_page_do_not_exist() {
        assert_eq!(tile_rect((1224, 1584), 3, 0), None);
        assert_eq!(tile_rect((1224, 1584), 0, 4), None);
        assert_eq!(tile_rect((1224, 1584), u32::MAX, 0), None);
    }
}
