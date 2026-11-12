//! the accessibility checker's eyes: parts of pages drawn by the app's
//! pdf engine, so contrast can be measured from real pixels.
//!
//! the checker asks for the area under each piece of text, often hundreds
//! per page, so the whole page is drawn once and the areas are cut out of
//! it. only the last page drawn is kept (about 8 mb at 2 pixels per point).

use letters_a11y::{Rect, RegionRenderer, RgbaImage};
use letters_pdf::{page_pixel_size, DocumentId, PageSize, RenderWorker};

use crate::compare::{render_page, scale_in_thousandths};

pub struct PageRegions {
    worker: RenderWorker,
    document: DocumentId,
    page_sizes: Vec<PageSize>,
    /// (page, scale in thousandths) and its picture
    last: Option<((u32, u32), letters_compare::RgbaImage)>,
    /// how many whole pages were drawn, for the tests
    pub pages_drawn: usize,
}

impl PageRegions {
    pub fn new(worker: &RenderWorker, document: DocumentId, page_sizes: Vec<PageSize>) -> Self {
        Self {
            worker: worker.clone(),
            document,
            page_sizes,
            last: None,
            pages_drawn: 0,
        }
    }

    /// the whole page at `scale`, from the one kept or freshly drawn.
    fn page_picture(&mut self, page: u32, scale: f32) -> Option<&letters_compare::RgbaImage> {
        let key = (page, scale_in_thousandths(scale));
        if self.last.as_ref().map(|(kept, _)| *kept) != Some(key) {
            let size = *self.page_sizes.get(page as usize)?;
            let picture = render_page(&self.worker, self.document, size, page, scale).ok()?;
            self.pages_drawn += 1;
            self.last = Some((key, picture));
        }
        self.last.as_ref().map(|(_, picture)| picture)
    }
}

impl RegionRenderer for PageRegions {
    fn render_region(&mut self, page: u32, rect: Rect, scale: f32) -> Option<RgbaImage> {
        let size = *self.page_sizes.get(page as usize)?;
        let pixels = page_pixel_size(size, scale_in_thousandths(scale));
        let area = pixel_area(rect, scale, pixels)?;
        let picture = self.page_picture(page, scale)?;
        Some(crop(picture, area))
    }
}

/// a part of a page as a png, for showing a picture or a table in the
/// fix wizard. the area is grown by `margin` points so the edge shows.
pub fn region_png(
    regions: &mut PageRegions,
    page: u32,
    rect: Rect,
    margin: f32,
    scale: f32,
) -> Result<Vec<u8>, String> {
    let grown = Rect::new(
        rect.x - margin,
        rect.y - margin,
        rect.width + 2.0 * margin,
        rect.height + 2.0 * margin,
    );
    let picture = regions
        .render_region(page, grown, scale)
        .ok_or("That part of the page can’t be drawn.")?;
    let image = letters_compare::RgbaImage {
        width: picture.width,
        height: picture.height,
        pixels: picture.pixels,
    };
    crate::compare::encode_png(&image)
}

/// the pixels a rectangle in points covers, kept on the page: left, top,
/// right, bottom (right and bottom not included). none when it misses the page.
pub fn pixel_area(rect: Rect, scale: f32, page_pixels: (u32, u32)) -> Option<(u32, u32, u32, u32)> {
    let (width, height) = page_pixels;
    let clamp = |value: f32, limit: u32| value.max(0.0).min(limit as f32) as u32;
    let left = clamp((rect.x * scale).floor(), width);
    let top = clamp((rect.y * scale).floor(), height);
    let right = clamp((rect.right() * scale).ceil(), width);
    let bottom = clamp((rect.bottom() * scale).ceil(), height);
    (right > left && bottom > top).then_some((left, top, right, bottom))
}

/// cuts an area out of a page picture.
pub fn crop(
    picture: &letters_compare::RgbaImage,
    (left, top, right, bottom): (u32, u32, u32, u32),
) -> RgbaImage {
    let width = right - left;
    let height = bottom - top;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in top..bottom {
        let start = ((y * picture.width + left) * 4) as usize;
        pixels.extend_from_slice(&picture.pixels[start..start + (width * 4) as usize]);
    }
    RgbaImage {
        width,
        height,
        pixels,
    }
}
