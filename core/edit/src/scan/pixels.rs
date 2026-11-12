//! the scan's pixels and where they are on the page: reading the page
//! image, turning page points into pixels and back, and sampling colours.

use image::{Rgb, RgbImage};

use crate::error::EditError;
use crate::geometry::{Point, Rect};
use crate::image_data;
use crate::layout::PageModel;

/// a scan covers at least this share of the page; smaller pictures are
/// figures on a page of real text, not a scanned page.
const SCAN_SHARE: f32 = 0.5;

/// the scanned page image, decoded, and where it is drawn.
pub(crate) struct Scan {
    pub image: usize,
    pub pixels: RgbImage,
    /// the image's box on screen (scans are drawn upright)
    pub rect: Rect,
}

impl Scan {
    /// pixels per point across and down.
    pub fn scale(&self) -> (f32, f32) {
        (
            self.pixels.width() as f32 / self.rect.width,
            self.pixels.height() as f32 / self.rect.height,
        )
    }

    /// the pixel under a point on the page, clamped to the image.
    pub fn pixel_at(&self, point: Point) -> (u32, u32) {
        let (sx, sy) = self.scale();
        let x = ((point.x - self.rect.x) * sx).max(0.0) as u32;
        let y = ((point.y - self.rect.y) * sy).max(0.0) as u32;
        (
            x.min(self.pixels.width() - 1),
            y.min(self.pixels.height() - 1),
        )
    }

    /// the pixels under a box on the page: left, top, right, bottom.
    pub fn pixel_box(&self, rect: &Rect) -> (u32, u32, u32, u32) {
        let (left, top) = self.pixel_at(Point::new(rect.x, rect.y));
        let (right, bottom) = self.pixel_at(Point::new(rect.right(), rect.bottom()));
        (left, top, right.max(left + 1), bottom.max(top + 1))
    }

    /// a box of pixels as a box on the page.
    pub fn page_rect(&self, left: u32, top: u32, right: u32, bottom: u32) -> Rect {
        let (sx, sy) = self.scale();
        Rect::new(
            self.rect.x + left as f32 / sx,
            self.rect.y + top as f32 / sy,
            (right - left) as f32 / sx,
            (bottom - top) as f32 / sy,
        )
    }
}

/// finds the page's scan and decodes it.
pub(crate) fn find_scan(doc: &lopdf::Document, model: &PageModel) -> Result<Scan, EditError> {
    let objects = model.objects();
    let page_area = objects.width * objects.height;
    let image = objects
        .images
        .iter()
        .enumerate()
        .filter(|(_, image)| image.rect.width * image.rect.height >= SCAN_SHARE * page_area)
        .max_by(|a, b| {
            (a.1.rect.width * a.1.rect.height).total_cmp(&(b.1.rect.width * b.1.rect.height))
        })
        .map(|(index, _)| index)
        .ok_or_else(|| {
            EditError::InvalidValue("This page has no scanned image to turn into text.".into())
        })?;
    let object = model.run.images[image].object.ok_or_else(|| {
        EditError::UnsupportedImage("The scan is written inside the page content.".into())
    })?;
    Ok(Scan {
        image,
        pixels: decode(doc, object)?,
        rect: objects.images[image].rect,
    })
}

/// the scan's pixels, from the jpeg or png that saving it out gives.
fn decode(doc: &lopdf::Document, object: lopdf::ObjectId) -> Result<RgbImage, EditError> {
    let file = image_data::extract(doc, object)?;
    let picture = image::load_from_memory(&file.bytes)
        .map_err(|_| EditError::UnsupportedImage("Letters can’t read the scan’s pixels.".into()))?;
    Ok(picture.to_rgb8())
}

/// the brightness of a pixel, 0 (black) to 255 (white).
pub(crate) fn luminance(pixel: &Rgb<u8>) -> f32 {
    let [r, g, b] = pixel.0;
    0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)
}

/// the middle value of some colours, channel by channel.
pub(crate) fn median_color(mut colors: Vec<[u8; 3]>) -> Option<[u8; 3]> {
    if colors.is_empty() {
        return None;
    }
    let mut channel = |index: usize| {
        colors.sort_by_key(|color| color[index]);
        colors[colors.len() / 2][index]
    };
    Some([channel(0), channel(1), channel(2)])
}
