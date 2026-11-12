//! previews of sample text in any font file, drawn by our own code: as an
//! svg path for the ui, or as an alpha bitmap. nothing is installed or
//! registered with the system, so fonts not installed (added, bundled,
//! downloaded, or just fetched for a look) preview the same way.

use serde::Serialize;
use ttf_parser::Face;

use crate::outline::{apply_variations, lay_out, svg_path};
use crate::raster::{fill, AlphaBitmap};

/// a line of sample text, ready to draw as `<path d="...">` in an svg
/// whose view box is `0 0 width height`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub path: String,
    /// characters the font can't draw
    pub missing: Vec<char>,
}

/// the largest preview size, in pixels per em.
pub const MAX_SIZE: f32 = 400.0;

fn face_at<'a>(data: &'a [u8], index: u32, variations: &[(String, f32)]) -> Option<Face<'a>> {
    let mut face = Face::parse(data, index).ok()?;
    apply_variations(&mut face, variations);
    Some(face)
}

/// `text` in the font at `size` pixels per em, as an svg path.
pub fn preview_svg(
    data: &[u8],
    index: u32,
    text: &str,
    size: f32,
    variations: &[(String, f32)],
) -> Option<Preview> {
    let face = face_at(data, index, variations)?;
    let laid = lay_out(&face, text, size.clamp(4.0, MAX_SIZE));
    Some(Preview {
        width: laid.width.ceil(),
        height: laid.height.ceil(),
        baseline: laid.baseline,
        path: svg_path(&laid.segments),
        missing: laid.missing,
    })
}

/// `text` in the font at `size` pixels per em, as an alpha bitmap one
/// line tall and as wide as the text.
pub fn preview_bitmap(
    data: &[u8],
    index: u32,
    text: &str,
    size: f32,
    variations: &[(String, f32)],
) -> Option<AlphaBitmap> {
    let face = face_at(data, index, variations)?;
    let laid = lay_out(&face, text, size.clamp(4.0, MAX_SIZE));
    let width = laid.width.ceil().max(1.0) as usize;
    let height = laid.height.ceil().max(1.0) as usize;
    Some(fill(&laid.segments, width, height))
}
