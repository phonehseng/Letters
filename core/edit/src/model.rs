//! what the app is told about a page: its text blocks and its images.
//!
//! ids name the exact operators an object was drawn by (the content
//! stream, the operator's position in it, and the glyph inside the
//! operator), together with a fingerprint of the page's content. they
//! are only valid until the next edit of the page: every edit returns
//! the page's objects again with fresh ids.

use serde::{Deserialize, Serialize};

use crate::colors::Color;
use crate::fonts::pdf_font::{FontFamily, FontKind};
use crate::geometry::{Point, Rect};

/// everything on a page that can be edited directly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageObjects {
    /// 0-based page number
    pub page: usize,
    /// the page as shown, in points
    pub width: f32,
    pub height: f32,
    pub text_blocks: Vec<TextBlock>,
    pub images: Vec<ImageObject>,
    /// vector drawings: each path painted on its own
    #[serde(default)]
    pub paths: Vec<PathObject>,
    /// paths that look like one object (a logo, a drawing in one group),
    /// so the ui can select them together
    #[serde(default)]
    pub path_groups: Vec<PathGroup>,
}

/// the outline of a drawn path, as far as editing its points goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathShape {
    /// four straight sides at right angles on screen; corners can be edited
    Rectangle,
    /// one straight segment; its ends can be edited
    Line,
    /// four curves around a centre
    Ellipse,
    /// anything else: it can be moved, resized and restyled, not reshaped
    Free,
    /// a smooth colour blend filling an area
    Shading,
}

/// a fill or stroke colour exactly as the file sets it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaintInfo {
    /// how it looks on screen (an approximation for special colour spaces)
    pub color: Color,
    /// "DeviceRGB", "DeviceGray", "DeviceCMYK", "Pattern" or the name of a
    /// colour space in the page's resources
    pub color_space: String,
    pub components: Vec<f32>,
}

/// how a path's outline is drawn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrokeInfo {
    pub paint: PaintInfo,
    /// the line width as seen on the page, in points
    pub width: f32,
    /// dash lengths in points as seen; empty for a solid line
    pub dash: Vec<f32>,
}

/// one drawn path (or one shading) on the page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathObject {
    pub id: String,
    /// the box around it on screen, including half the line width
    pub rect: Rect,
    pub shape: PathShape,
    pub fill: Option<PaintInfo>,
    pub stroke: Option<StrokeInfo>,
    /// 0 to 1
    pub fill_opacity: f32,
    pub stroke_opacity: f32,
    /// drawn through a clipping path that cuts part of it off
    pub clipped: bool,
    /// whether its points can be edited with `set_path_points`
    pub point_editable: bool,
    /// a rectangle's corners (top left, top right, bottom left, bottom
    /// right) or a line's two ends, on screen; empty for other shapes
    pub points: Vec<Point>,
}

/// why some paths are suggested as one object.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupReason {
    /// drawn inside one q ... Q block
    SameBlock,
    /// drawn inside one marked-content sequence
    MarkedContent,
    /// many small paths drawn one after another, touching: a logo or icon
    Cluster,
}

/// a suggested group of paths.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathGroup {
    pub id: String,
    pub path_ids: Vec<String>,
    pub rect: Rect,
    pub reason: GroupReason,
}

/// how a block's lines line up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlignment {
    #[default]
    Left,
    Right,
    Center,
    /// both edges line up; the last line starts at the left
    Justified,
}

/// a paragraph-like group of lines: what clicking into text selects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBlock {
    pub id: String,
    pub rect: Rect,
    /// the lines' text joined with new lines
    pub text: String,
    pub lines: Vec<TextLine>,
    /// the direction the text runs, in degrees clockwise on screen (0 is
    /// left to right)
    pub rotation: f32,
    pub alignment: TextAlignment,
}

/// one line of text along one baseline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextLine {
    pub id: String,
    pub rect: Rect,
    /// the text, with a space wherever the gap between letters is a word
    /// gap (many pdfs leave spaces out and just move the next word)
    pub text: String,
    /// where the baseline starts and ends on screen
    pub baseline: [Point; 2],
    pub runs: Vec<TextRun>,
}

/// a stretch of a line in one font, size and colour.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRun {
    pub id: String,
    pub text: String,
    pub font: FontInfo,
    /// the size as seen on the page, in points
    pub size: f32,
    pub color: Color,
    /// one box per character of `text`, on screen
    pub char_boxes: Vec<Rect>,
    /// invisible text, such as the searchable layer over a scan
    pub invisible: bool,
}

/// what the app needs to know about a font to decide what can be typed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontInfo {
    pub id: String,
    /// the font's name without any subset tag, e.g. "Arial-BoldMT"
    pub name: String,
    pub family: FontFamily,
    pub bold: bool,
    pub italic: bool,
    /// whether the font's glyphs are stored in the file
    pub embedded: bool,
    /// whether only some of the font's glyphs are stored
    pub subset: bool,
    pub kind: FontKind,
    /// every character this font can still draw; others need a fallback
    pub editable_chars: String,
}

/// one image drawn on the page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageObject {
    pub id: String,
    /// the smallest upright box around the image on screen
    pub rect: Rect,
    /// the image's corners on screen: top left, top right, bottom left,
    /// bottom right (different from the box when turned or slanted)
    pub quad: [Point; 4],
    pub pixel_width: u32,
    pub pixel_height: u32,
    /// e.g. "DeviceRGB", "DeviceGray", "ICCBased", "Indexed"
    pub color_space: String,
    pub bits_per_component: u32,
    pub has_alpha: bool,
    /// written inside the page content (BI ... EI) rather than as an object
    pub inline: bool,
    /// e.g. "DCTDecode" for a jpeg
    pub filter: Option<String>,
    /// the part that shows, when a clipping path (a crop) hides the rest
    #[serde(default)]
    pub crop_rect: Option<Rect>,
    /// pixels per inch at the size it is placed (the lower of across and
    /// down), so the ui can warn about blurry pictures; 0 when unknown
    #[serde(default)]
    pub pixels_per_inch: f32,
    /// 0 to 1
    #[serde(default = "fully_opaque")]
    pub opacity: f32,
}

fn fully_opaque() -> f32 {
    1.0
}
