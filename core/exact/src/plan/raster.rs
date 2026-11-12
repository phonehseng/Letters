//! last-resort rasterising of vector-only content (recipe 10).
//!
//! the page's resources are copied into a new one-page pdf that holds only
//! the given items' operators. the caller's render callback draws it at
//! 300 dpi on a transparent background, and the picture is cropped to the
//! items' bounding box plus 1 pt. text is never rasterised.

use std::collections::HashMap;

use image::RgbaImage;
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

use crate::pdf::content::{PageContent, PathItem};
use crate::pdf::images::encode_png;

pub const RASTER_DPI: f32 = 300.0;

/// a page drawn by the caller: rgba pixels, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedPage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// draws page 0 of a pdf (given as bytes) at a resolution in dots per
/// inch, on a transparent background.
pub type RenderFn = dyn Fn(&[u8], f32) -> Result<RenderedPage, String> + Send + Sync;

/// a rasterised picture of some vector items, in page points (y up).
#[derive(Clone, Debug)]
pub struct RasterPicture {
    pub left: f64,
    pub bottom: f64,
    pub right: f64,
    pub top: f64,
    pub png: Vec<u8>,
    pub items: usize,
    pub sequence: usize,
}

/// copies an object and everything it refers to from one document to another.
fn copy_object(
    source: &Document,
    object: &Object,
    target: &mut Document,
    copied: &mut HashMap<ObjectId, ObjectId>,
) -> Object {
    match object {
        Object::Reference(id) => {
            if let Some(new_id) = copied.get(id) {
                return Object::Reference(*new_id);
            }
            let new_id = target.new_object_id();
            copied.insert(*id, new_id);
            let inner = source.get_object(*id).cloned().unwrap_or(Object::Null);
            let copy = copy_object(source, &inner, target, copied);
            target.objects.insert(new_id, copy);
            Object::Reference(new_id)
        }
        Object::Array(items) => Object::Array(
            items
                .iter()
                .map(|i| copy_object(source, i, target, copied))
                .collect(),
        ),
        Object::Dictionary(dict) => {
            let mut copy = lopdf::Dictionary::new();
            for (key, value) in dict.iter() {
                if key == b"Parent" {
                    continue;
                }
                copy.set(key.clone(), copy_object(source, value, target, copied));
            }
            Object::Dictionary(copy)
        }
        Object::Stream(stream) => {
            let mut dict = lopdf::Dictionary::new();
            for (key, value) in stream.dict.iter() {
                dict.set(key.clone(), copy_object(source, value, target, copied));
            }
            Object::Stream(Stream::new(dict, stream.content.clone()).with_compression(false))
        }
        other => other.clone(),
    }
}

/// a one-page pdf with the page's resources and only these items.
pub fn vector_only_pdf(
    source: &Document,
    resources: Option<&Object>,
    width: f64,
    height: f64,
    items: &[&PathItem],
) -> Result<Vec<u8>, String> {
    let mut target = Document::with_version("1.7");
    let mut copied = HashMap::new();
    let pages_id = target.new_object_id();
    let content: Vec<u8> = items.iter().flat_map(|item| item.raw.clone()).collect();
    let content_id = target.add_object(Stream::new(lopdf::Dictionary::new(), content));
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), Object::Real(width as f32), Object::Real(height as f32)],
        "Contents" => content_id,
    };
    if let Some(resources) = resources {
        page.set(
            "Resources",
            copy_object(source, resources, &mut target, &mut copied),
        );
    }
    let page_id = target.add_object(page);
    target.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 },
        ),
    );
    let catalog = target.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    target.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    target.save_to(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// one transparent picture holding only the given vector items of a page.
pub fn rasterise(
    source: &Document,
    resources: Option<&Object>,
    page: &PageContent,
    items: &[&PathItem],
    render: &RenderFn,
) -> Result<Option<RasterPicture>, String> {
    if items.is_empty() {
        return Ok(None);
    }
    let boxes: Vec<(f64, f64, f64, f64)> = items.iter().map(|i| i.bbox()).collect();
    let left = (boxes.iter().map(|b| b.0).fold(f64::INFINITY, f64::min) - 1.0).max(0.0);
    let bottom = (boxes.iter().map(|b| b.1).fold(f64::INFINITY, f64::min) - 1.0).max(0.0);
    let right = (boxes.iter().map(|b| b.2).fold(f64::NEG_INFINITY, f64::max) + 1.0).min(page.width);
    let top = (boxes.iter().map(|b| b.3).fold(f64::NEG_INFINITY, f64::max) + 1.0).min(page.height);
    let pdf = vector_only_pdf(source, resources, page.width, page.height, items)?;
    let rendered = render(&pdf, RASTER_DPI)?;
    let image = RgbaImage::from_raw(rendered.width, rendered.height, rendered.rgba)
        .ok_or("the drawn page has the wrong size")?;
    let scale = f64::from(RASTER_DPI) / 72.0;
    let x0 = (left * scale) as u32;
    let y0 = ((page.height - top) * scale) as u32;
    let x1 = ((right * scale) as u32 + 1).min(image.width());
    let y1 = (((page.height - bottom) * scale) as u32 + 1).min(image.height());
    if x1 <= x0 || y1 <= y0 {
        return Ok(None);
    }
    let cropped = image::imageops::crop_imm(&image, x0, y0, x1 - x0, y1 - y0).to_image();
    let png = encode_png(cropped)?;
    Ok(Some(RasterPicture {
        left: f64::from(x0) / scale,
        bottom: page.height - f64::from(y1) / scale,
        right: f64::from(x1) / scale,
        top: page.height - f64::from(y0) / scale,
        png,
        items: items.len(),
        sequence: items.iter().map(|i| i.sequence).min().unwrap_or(0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::content::Segment;

    #[test]
    fn the_vector_only_page_holds_just_the_items() {
        let item = PathItem {
            page: 0,
            segments: vec![Segment::Rect([
                (0.0, 0.0),
                (10.0, 0.0),
                (10.0, 10.0),
                (0.0, 10.0),
            ])],
            fill: true,
            stroke: false,
            even_odd: false,
            fill_color: Some([1.0, 0.0, 0.0]),
            stroke_color: None,
            line_width: 1.0,
            dashed: true,
            alpha: 1.0,
            clip: Vec::new(),
            pattern: false,
            raw: b"q 1 0 0 rg 0 0 10 10 re f Q\n".to_vec(),
            sequence: 3,
        };
        let pdf =
            vector_only_pdf(&Document::with_version("1.7"), None, 100.0, 50.0, &[&item]).unwrap();
        let reread = Document::load_mem(&pdf).unwrap();
        let page = *reread.get_pages().values().next().unwrap();
        let content = reread.get_page_content(page);
        assert!(String::from_utf8_lossy(&content).contains("0 0 10 10 re f"));
    }
}
