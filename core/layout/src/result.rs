//! what layout produces: pages of positioned glyphs, rules and pictures,
//! with a map from model positions to page coordinates and back (for the
//! editor's caret, selection and hit testing).
//!
//! all coordinates are points from the page's top-left corner, y down,
//! fractional: the result is never snapped to a device grid.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::flavor::Flavor;
use crate::fonts::{FontId, FontOrigin, FontSubstitution, LoadedFace};
use crate::unsupported::Unsupported;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutResult {
    pub flavor: Flavor,
    pub pages: Vec<Page>,
    /// every face a glyph run refers to, by `FontId`
    pub fonts: Vec<FontInfo>,
    pub substitutions: Vec<FontSubstitution>,
    pub unsupported: Vec<Unsupported>,
    /// the loaded faces behind `fonts`, for the pdf writer (not in json)
    #[serde(skip)]
    pub faces: Vec<Arc<LoadedFace>>,
}

/// a face as the result describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontInfo {
    pub family: String,
    pub postscript_name: String,
    pub bold: bool,
    pub italic: bool,
    pub synthetic_bold: bool,
    pub synthetic_italic: bool,
    pub origin: FontOrigin,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub index: u32,
    /// the number the page shows (sections can restart numbering)
    pub number: u32,
    pub section: u32,
    pub width: f64,
    pub height: f64,
    pub lines: Vec<Line>,
    pub rects: Vec<Rect>,
    pub images: Vec<PlacedImage>,
    pub frames: Vec<Frame>,
    /// the page came from exact-position content (see `positioned`)
    #[serde(default)]
    pub exact: bool,
}

/// which part of the document a line belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Story {
    Body,
    Header {
        id: String,
    },
    Footer {
        id: String,
    },
    Footnote {
        id: String,
    },
    Endnote {
        id: String,
    },
    /// a text box, by the body block it is anchored at
    TextBox {
        block: u32,
    },
    /// exact-position content, by the index of the positioned page
    Positioned {
        page: u32,
    },
}

/// a model position: a paragraph (by its path) and an offset in it.
/// the path is the block index, then for a table cell: row, cell, block
/// inside the cell, and so on for nested tables.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPosition {
    pub story: Story,
    pub block: Vec<u32>,
    /// offset in the paragraph's units (see `text` module notes)
    pub offset: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    /// stays the same across relayouts while the line's paragraph and its
    /// place among that paragraph's lines do (see incremental)
    #[serde(default)]
    pub id: u64,
    pub story: Story,
    pub block: Vec<u32>,
    /// the model offsets the line covers: `start..end`
    pub start: u32,
    pub end: u32,
    pub x: f64,
    pub width: f64,
    pub top: f64,
    pub height: f64,
    pub baseline: f64,
    pub ascent: f64,
    pub descent: f64,
    /// caret x for each offset `start..=end` (len `end - start + 1`)
    pub carets: Vec<f64>,
    pub runs: Vec<GlyphRun>,
}

/// glyphs of one face, size and colour on one baseline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlyphRun {
    pub font: FontId,
    pub size: f64,
    /// the baseline of this run (superscripts sit above the line's)
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
    pub glyphs: Vec<u16>,
    /// pen x of every glyph on the page
    pub x: Vec<f64>,
    /// the text the run shows, and for each glyph the char index in it
    /// where its characters start (for copying text out of the pdf)
    pub text: String,
    pub clusters: Vec<u32>,
    /// the model offsets of the run's text: `start..end`
    pub start: u32,
    pub end: u32,
    /// slant made up for an italic the family lacks (degrees)
    #[serde(default, skip_serializing_if = "is_zero")]
    pub skew: f64,
    /// stroke width made up for a bold the family lacks
    #[serde(default, skip_serializing_if = "is_zero")]
    pub embolden: f64,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RectKind {
    Border,
    Shading,
    Underline,
    Strike,
    Highlight,
    /// the line above the footnotes
    NoteSeparator,
    /// a tab leader's dots or dashes, drawn as a dotted rule
    Leader,
    ColumnSeparator,
}

/// a filled rectangle: rules, borders, shading, decoration lines.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub kind: RectKind,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub color: [u8; 3],
    /// dotted (leaders, dotted borders)
    #[serde(default)]
    pub dotted: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedImage {
    pub media_id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub story: Story,
    pub block: Vec<u32>,
    pub offset: u32,
    pub floating: bool,
}

/// a floating frame: a text box's outline and fill (its text is in the
/// page's lines with `Story::TextBox`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<[u8; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<([u8; 3], f64)>,
    pub block: u32,
}

/// where a caret goes: a page, an x and the line's vertical extent.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caret {
    pub page: u32,
    pub x: f64,
    pub top: f64,
    pub bottom: f64,
}

impl Line {
    fn covers(&self, position: &ModelPosition) -> bool {
        self.story == position.story
            && self.block == position.block
            && position.offset >= self.start
            && position.offset <= self.end
    }

    fn caret_x(&self, offset: u32) -> f64 {
        let index = (offset.saturating_sub(self.start)) as usize;
        self.carets
            .get(index)
            .copied()
            .unwrap_or(self.x + self.width)
    }
}

impl LayoutResult {
    /// where the caret for a model position goes. a position at the very
    /// end of one line and the start of the next goes to the next line,
    /// as editors show it.
    pub fn locate(&self, position: &ModelPosition) -> Option<Caret> {
        let mut found = None;
        for page in &self.pages {
            for line in &page.lines {
                if line.covers(position) {
                    let caret = Caret {
                        page: page.index,
                        x: line.caret_x(position.offset),
                        top: line.top,
                        bottom: line.top + line.height,
                    };
                    // a later line starting exactly here wins
                    if found.is_none() || line.start == position.offset {
                        found = Some(caret);
                    }
                }
            }
        }
        found
    }

    /// the model position nearest to a point on a page: the closest line
    /// vertically, then the closest caret along it.
    pub fn hit_test(&self, page: u32, x: f64, y: f64) -> Option<ModelPosition> {
        let page = self.pages.get(page as usize)?;
        let line = page.lines.iter().min_by(|a, b| {
            vertical_distance(a, y)
                .total_cmp(&vertical_distance(b, y))
                .then(horizontal_distance(a, x).total_cmp(&horizontal_distance(b, x)))
        })?;
        let offset = (0..line.carets.len())
            .min_by(|&a, &b| {
                (line.carets[a] - x)
                    .abs()
                    .total_cmp(&(line.carets[b] - x).abs())
            })
            .map(|index| line.start + index as u32)
            .unwrap_or(line.start);
        Some(ModelPosition {
            story: line.story.clone(),
            block: line.block.clone(),
            offset,
        })
    }

    /// the boxes covering a selection between two positions in one
    /// paragraph (multi-paragraph selections are a list of these).
    pub fn selection_boxes(&self, from: &ModelPosition, to: &ModelPosition) -> Vec<(u32, Rect)> {
        let mut boxes = Vec::new();
        if from.story != to.story || from.block != to.block {
            return boxes;
        }
        let (start, end) = (from.offset.min(to.offset), from.offset.max(to.offset));
        for page in &self.pages {
            for line in &page.lines {
                if line.story != from.story
                    || line.block != from.block
                    || line.end < start
                    || line.start > end
                {
                    continue;
                }
                let left = line.caret_x(start.max(line.start));
                let right = line.caret_x(end.min(line.end));
                boxes.push((
                    page.index,
                    Rect {
                        kind: RectKind::Highlight,
                        x: left,
                        y: line.top,
                        width: (right - left).max(0.0),
                        height: line.height,
                        color: [51, 153, 255],
                        dotted: false,
                    },
                ));
            }
        }
        boxes
    }
}

fn vertical_distance(line: &Line, y: f64) -> f64 {
    if y < line.top {
        line.top - y
    } else if y > line.top + line.height {
        y - line.top - line.height
    } else {
        0.0
    }
}

fn horizontal_distance(line: &Line, x: f64) -> f64 {
    if x < line.x {
        line.x - x
    } else if x > line.x + line.width {
        x - line.x - line.width
    } else {
        0.0
    }
}
