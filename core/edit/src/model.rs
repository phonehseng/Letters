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
}
