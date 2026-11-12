//! grouping glyphs into runs, lines and paragraph-like blocks.
//!
//! a pdf stores text as glyphs at positions, not as lines or paragraphs.
//! the grouping here follows what the eye does: glyphs on the same
//! baseline with small gaps make a line; a gap about a space wide is a
//! word gap; lines of similar size, evenly spaced and lined up make a
//! block, which is what clicking into a paragraph selects.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, ObjectId};

use crate::error::EditError;
use crate::fonts::pdf_font::PdfFont;
use crate::geometry::{Point, Rect};
use crate::interpret::{self, GlyphRecord, Interpretation, Origin, ShowItem};
use crate::model::{
    FontInfo, ImageObject, PageObjects, TextAlignment, TextBlock, TextLine, TextRun,
};
use crate::pages;
use crate::separators::{self, Separator};

mod grouping;
mod objects;

use grouping::{group_blocks, group_lines};
/// a gap wider than this (in font sizes) between two glyphs is a word gap.
const WORD_GAP: f32 = 0.2;
/// a gap wider than this starts a new line: it is a column or a table cell.
const COLUMN_GAP: f32 = 2.5;
/// glyphs whose baselines are further apart than this are on different lines.
const BASELINE_TOLERANCE: f32 = 0.3;

/// one character of a line: a glyph, or a word gap with no glyph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LineChar {
    Glyph(usize),
    /// a space the eye sees between two glyphs
    Gap,
}

#[derive(Clone, Debug)]
pub(crate) struct LineModel {
    pub chars: Vec<LineChar>,
    /// baseline start on screen, the direction text runs and "down"
    pub start: Point,
    pub direction: Point,
    pub normal: Point,
    pub size: f32,
    /// how far the line reaches along its direction, from `start`
    pub length: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct BlockModel {
    pub lines: Vec<LineModel>,
    pub alignment: TextAlignment,
    /// the distance between baselines, when the block has two lines or more
    pub leading: Option<f32>,
}

/// a page's text and images, grouped, with the interpretation behind it.
pub(crate) struct PageModel {
    pub page: usize,
    pub run: Interpretation,
    pub blocks: Vec<BlockModel>,
    pub fingerprint: u32,
    glyph_keys: Vec<String>,
    /// the dictionaries of the image objects drawn, read while building
    image_dicts: HashMap<ObjectId, Dictionary>,
}

pub(crate) fn build(doc: &Document, page: usize) -> Result<PageModel, EditError> {
    let page_id = pages::page_id(doc, page)?;
    let run = interpret::interpret(doc, page_id)?;
    let fingerprint = run.sources.iter().fold(0u32, |hash, source| {
        hash.rotate_left(5) ^ source.fingerprint
    });
    let glyph_keys = glyph_keys(&run);
    let lines = group_lines(&run);
    let separators = separators::horizontal_separators(&run);
    let blocks = group_blocks(lines, &separators);
    let image_dicts = run
        .images
        .iter()
        .filter_map(|image| image.object)
        .filter_map(|id| Some((id, doc.get_object(id).ok()?.as_stream().ok()?.dict.clone())))
        .collect();
    Ok(PageModel {
        page,
        run,
        blocks,
        fingerprint,
        glyph_keys,
        image_dicts,
    })
}

/// a short name for a content stream, e.g. "c0" or "c0>12" (the form
/// drawn by operator 12 of stream c0).
pub(crate) fn source_key(run: &Interpretation, source: usize) -> String {
    match &run.sources[source].origin {
        Origin::Page { index } => format!("c{index}"),
        Origin::Form { parent, op, .. } => format!("{}>{op}", source_key(run, *parent)),
    }
}

/// "stream:operator:glyph" for every glyph.
fn glyph_keys(run: &Interpretation) -> Vec<String> {
    let mut keys = vec![String::new(); run.glyphs.len()];
    for show in &run.shows {
        let source = source_key(run, show.source);
        let mut ordinal = 0;
        for item in &show.items {
            if let ShowItem::Glyph(glyph) = item {
                keys[*glyph] = format!("{source}:{}:{ordinal}", show.op);
                ordinal += 1;
            }
        }
    }
    keys
}

impl PageModel {
    fn key(&self, kind: &str, glyph: usize) -> String {
        format!("{kind}:{:08x}:{}", self.fingerprint, self.glyph_keys[glyph])
    }

    pub fn first_glyph(line: &LineModel) -> usize {
        line.chars
            .iter()
            .find_map(|char| match char {
                LineChar::Glyph(index) => Some(*index),
                LineChar::Gap => None,
            })
            .expect("lines start with a glyph")
    }

    pub fn block_id(&self, block: usize) -> String {
        self.key("block", Self::first_glyph(&self.blocks[block].lines[0]))
    }

    pub fn line_id(&self, block: usize, line: usize) -> String {
        self.key("line", Self::first_glyph(&self.blocks[block].lines[line]))
    }

    pub fn image_id(&self, image: usize) -> String {
        let record = &self.run.images[image];
        format!(
            "image:{:08x}:{}:{}",
            self.fingerprint,
            source_key(&self.run, record.source),
            record.op
        )
    }

    /// the text of one character of a line.
    pub fn char_text(&self, char: &LineChar) -> &str {
        match char {
            LineChar::Glyph(index) => &self.run.glyphs[*index].text,
            LineChar::Gap => " ",
        }
    }

    pub fn line_text(&self, line: &LineModel) -> String {
        line.chars.iter().map(|char| self.char_text(char)).collect()
    }

    pub fn find_block(&self, id: &str) -> Result<usize, EditError> {
        (0..self.blocks.len())
            .find(|block| self.block_id(*block) == id)
            .ok_or(EditError::UnknownObject)
    }

    pub fn find_line(&self, id: &str) -> Result<(usize, usize), EditError> {
        for (block, model) in self.blocks.iter().enumerate() {
            for line in 0..model.lines.len() {
                if self.line_id(block, line) == id {
                    return Ok((block, line));
                }
            }
        }
        Err(EditError::UnknownObject)
    }

    /// a run: its line, and the range of the line's characters it covers.
    pub fn find_run(&self, id: &str) -> Result<(usize, usize, std::ops::Range<usize>), EditError> {
        for (block, model) in self.blocks.iter().enumerate() {
            for (line_index, line) in model.lines.iter().enumerate() {
                for (start, end) in self.run_ranges(line) {
                    if let LineChar::Glyph(glyph) = line.chars[start] {
                        if self.key("run", glyph) == id {
                            return Ok((block, line_index, start..end));
                        }
                    }
                }
            }
        }
        Err(EditError::UnknownObject)
    }

    pub fn find_image(&self, id: &str) -> Result<usize, EditError> {
        (0..self.run.images.len())
            .find(|image| self.image_id(*image) == id)
            .ok_or(EditError::UnknownObject)
    }
}
