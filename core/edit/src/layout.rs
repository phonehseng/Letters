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
    let blocks = group_blocks(lines);
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

/// a glyph on screen: baseline start and end, size and direction.
struct VisibleGlyph {
    origin: Point,
    end: Point,
    direction: Point,
    size: f32,
}

fn visible(run: &Interpretation, glyph: &GlyphRecord) -> VisibleGlyph {
    let origin = run.space.visible_point(glyph.origin);
    let end = run.space.visible_point(glyph.end);
    let mut direction = end.minus(origin);
    if direction.length() < 1e-6 {
        // a glyph with no width (a combining accent, a zero-width space)
        // runs in the direction of its text matrix
        let to_page = glyph.text_matrix.then(&run.shows[glyph.show].ctm);
        direction = run
            .space
            .visible_vector(to_page.apply_vector(Point::new(1.0, 0.0)));
    }
    VisibleGlyph {
        origin,
        end,
        direction: direction.unit(),
        size: glyph.size.max(0.1),
    }
}

fn group_lines(run: &Interpretation) -> Vec<LineModel> {
    let mut lines: Vec<LineModel> = Vec::new();
    for (index, glyph) in run.glyphs.iter().enumerate() {
        let seen = visible(run, glyph);
        let joins = lines
            .last()
            .and_then(|line| joining_gap(run, line, &seen, glyph));
        match (joins, lines.last_mut()) {
            (Some(gap), Some(line)) => {
                let previous_space = match line.chars.last() {
                    Some(LineChar::Glyph(last)) => run.glyphs[*last].is_space,
                    _ => true,
                };
                if gap > WORD_GAP * line.size && !previous_space && !glyph.is_space {
                    line.chars.push(LineChar::Gap);
                }
                line.chars.push(LineChar::Glyph(index));
                line.length = line
                    .length
                    .max(seen.end.minus(line.start).dot(line.direction));
            }
            _ => lines.push(LineModel {
                chars: vec![LineChar::Glyph(index)],
                start: seen.origin,
                direction: seen.direction,
                normal: Point::new(-seen.direction.y, seen.direction.x),
                size: seen.size,
                length: seen.end.minus(seen.origin).dot(seen.direction),
            }),
        }
    }
    lines
}

/// the gap before a glyph if it continues the line, or none if it starts
/// a new one.
fn joining_gap(
    run: &Interpretation,
    line: &LineModel,
    seen: &VisibleGlyph,
    glyph: &GlyphRecord,
) -> Option<f32> {
    let same_direction = line.direction.dot(seen.direction) > 0.995;
    let off_baseline = seen.origin.minus(line.start).dot(line.normal).abs();
    let ratio = seen.size / line.size;
    let line_end = line.start.plus(line.direction.scaled(line.length));
    let gap = seen.origin.minus(line_end).dot(line.direction);
    let close = gap > -0.6 * line.size && gap < COLUMN_GAP * line.size;
    // invisible text (a scan's text layer) never joins visible text
    let last_invisible = line
        .chars
        .iter()
        .rev()
        .find_map(|char| match char {
            LineChar::Glyph(index) => Some(run.glyphs[*index].invisible),
            LineChar::Gap => None,
        })
        .unwrap_or(false);
    (same_direction
        && off_baseline < BASELINE_TOLERANCE * line.size
        && (0.6..1.7).contains(&ratio)
        && close
        && last_invisible == glyph.invisible)
        .then_some(gap.max(0.0))
}

fn group_blocks(lines: Vec<LineModel>) -> Vec<BlockModel> {
    let mut blocks: Vec<BlockModel> = Vec::new();
    for line in lines {
        let joins = blocks
            .last()
            .is_some_and(|block| continues_block(block, &line));
        match (joins, blocks.last_mut()) {
            (true, Some(block)) => {
                let previous = block.lines.last().expect("blocks have lines");
                let step = line.start.minus(previous.start).dot(previous.normal);
                block.leading.get_or_insert(step);
                block.lines.push(line);
            }
            _ => blocks.push(BlockModel {
                lines: vec![line],
                alignment: TextAlignment::Left,
                leading: None,
            }),
        }
    }
    for block in &mut blocks {
        block.alignment = alignment_of(block);
    }
    blocks
}

fn continues_block(block: &BlockModel, line: &LineModel) -> bool {
    let first = &block.lines[0];
    let previous = block.lines.last().expect("blocks have lines");
    if first.direction.dot(line.direction) < 0.995
        || !(0.8..1.25).contains(&(line.size / first.size))
    {
        return false;
    }
    let step = line.start.minus(previous.start).dot(previous.normal);
    if !(0.8 * first.size..2.5 * first.size).contains(&step) {
        return false;
    }
    if let Some(leading) = block.leading {
        if (step - leading).abs() > 0.25 * leading {
            return false;
        }
    }
    // the line must sit under the block, not off to one side
    let along = |line: &LineModel| {
        let start = line.start.minus(first.start).dot(first.direction);
        (start, start + line.length)
    };
    let (start, end) = along(line);
    let (block_start, block_end) = block
        .lines
        .iter()
        .map(along)
        .fold((f32::MAX, f32::MIN), |(low, high), (start, end)| {
            (low.min(start), high.max(end))
        });
    let slack = 0.5 * first.size;
    start < block_end + slack && end > block_start - slack
}

fn alignment_of(block: &BlockModel) -> TextAlignment {
    if block.lines.len() < 2 {
        return TextAlignment::Left;
    }
    let first = &block.lines[0];
    let tolerance = 0.5 * first.size;
    let ends: Vec<(f32, f32)> = block
        .lines
        .iter()
        .map(|line| {
            let start = line.start.minus(first.start).dot(first.direction);
            (start, start + line.length)
        })
        .collect();
    let lined_up = |values: &[f32]| {
        values
            .iter()
            .all(|value| (value - values[0]).abs() < tolerance)
    };
    let starts: Vec<f32> = ends.iter().map(|(start, _)| *start).collect();
    let rights: Vec<f32> = ends.iter().map(|(_, end)| *end).collect();
    let centres: Vec<f32> = ends
        .iter()
        .map(|(start, end)| (start + end) / 2.0)
        .collect();
    let all_but_last = &rights[..rights.len() - 1];
    if lined_up(&starts) && all_but_last.len() >= 2 && lined_up(all_but_last) {
        TextAlignment::Justified
    } else if lined_up(&starts) {
        TextAlignment::Left
    } else if lined_up(&rights) {
        TextAlignment::Right
    } else if lined_up(&centres) {
        TextAlignment::Center
    } else {
        TextAlignment::Left
    }
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

    /// splits a line into runs of one font, size and colour; word gaps
    /// belong to the run before them.
    fn run_ranges(&self, line: &LineModel) -> Vec<(usize, usize)> {
        let mut ranges: Vec<(usize, usize)> = Vec::new();
        let mut current: Option<&GlyphRecord> = None;
        for (index, char) in line.chars.iter().enumerate() {
            let LineChar::Glyph(glyph) = char else {
                if let Some(last) = ranges.last_mut() {
                    last.1 = index + 1;
                }
                continue;
            };
            let glyph = &self.run.glyphs[*glyph];
            let same = current.is_some_and(|run| {
                run.font == glyph.font
                    && (run.size - glyph.size).abs() < 0.01
                    && run.color == glyph.color
                    && run.invisible == glyph.invisible
            });
            if same {
                ranges.last_mut().expect("a run is open").1 = index + 1;
            } else {
                ranges.push((index, index + 1));
                current = Some(glyph);
            }
        }
        ranges
    }

    fn char_box(&self, line: &LineModel, index: usize) -> Rect {
        let space = &self.run.space;
        match line.chars[index] {
            LineChar::Glyph(glyph) => space
                .visible_rect_of(&self.run.glyphs[glyph].corners)
                .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
            LineChar::Gap => {
                // the gap runs from the glyph before to the glyph after
                let glyph_of = |char: &LineChar| match char {
                    LineChar::Glyph(glyph) => Some(*glyph),
                    LineChar::Gap => None,
                };
                let before = line.chars[..index].iter().rev().find_map(glyph_of);
                let after = line.chars[index + 1..].iter().find_map(glyph_of);
                let mut corners: Vec<Point> = Vec::new();
                if let Some(glyph) = before {
                    let box_ = self.run.glyphs[glyph].corners;
                    corners.extend([box_[1], box_[3]]);
                }
                if let Some(glyph) = after {
                    let box_ = self.run.glyphs[glyph].corners;
                    corners.extend([box_[0], box_[2]]);
                }
                space
                    .visible_rect_of(&corners)
                    .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0))
            }
        }
    }

    pub fn font_info(&self, font: Option<usize>) -> FontInfo {
        let Some(index) = font else {
            return FontInfo {
                id: "font:none".into(),
                name: "Unknown".into(),
                family: crate::fonts::pdf_font::FontFamily::SansSerif,
                bold: false,
                italic: false,
                embedded: false,
                subset: false,
                kind: crate::fonts::pdf_font::FontKind::Type1,
                editable_chars: String::new(),
            };
        };
        let font: &PdfFont = &self.run.fonts[index];
        FontInfo {
            id: match font.object {
                Some((number, generation)) => format!("font:{number}-{generation}"),
                None => format!("font:#{index}"),
            },
            name: font.name.clone(),
            family: font.family,
            bold: font.bold,
            italic: font.italic,
            embedded: font.embedded,
            subset: font.subset,
            kind: font.kind,
            editable_chars: if font.writable() {
                font.drawable_chars()
            } else {
                String::new()
            },
        }
    }

    fn text_line(&self, block: usize, line_index: usize) -> TextLine {
        let line = &self.blocks[block].lines[line_index];
        let boxes: Vec<Rect> = (0..line.chars.len())
            .map(|index| self.char_box(line, index))
            .collect();
        let runs = self
            .run_ranges(line)
            .into_iter()
            .map(|(start, end)| {
                let LineChar::Glyph(first) = line.chars[start] else {
                    unreachable!("runs start with a glyph")
                };
                let glyph = &self.run.glyphs[first];
                let text: String = line.chars[start..end]
                    .iter()
                    .map(|char| self.char_text(char))
                    .collect();
                // ligatures give several characters one box; repeat it
                let char_boxes = line.chars[start..end]
                    .iter()
                    .zip(&boxes[start..end])
                    .flat_map(|(char, rect)| {
                        std::iter::repeat_n(*rect, self.char_text(char).chars().count())
                    })
                    .collect();
                TextRun {
                    id: self.key("run", first),
                    text,
                    font: self.font_info(glyph.font),
                    size: glyph.size,
                    color: glyph.color,
                    char_boxes,
                    invisible: glyph.invisible,
                }
            })
            .collect();
        let rect = boxes
            .iter()
            .skip(1)
            .fold(boxes[0], |all, rect| all.union(rect));
        TextLine {
            id: self.line_id(block, line_index),
            rect,
            text: self.line_text(line),
            baseline: [
                line.start,
                line.start.plus(line.direction.scaled(line.length)),
            ],
            runs,
        }
    }

    pub fn objects(&self) -> PageObjects {
        let text_blocks = self
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| {
                let lines: Vec<TextLine> = (0..block.lines.len())
                    .map(|line| self.text_line(index, line))
                    .collect();
                let rect = lines
                    .iter()
                    .skip(1)
                    .fold(lines[0].rect, |all, line| all.union(&line.rect));
                let direction = block.lines[0].direction;
                TextBlock {
                    id: self.block_id(index),
                    rect,
                    text: lines
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                    lines,
                    rotation: direction.y.atan2(direction.x).to_degrees(),
                    alignment: block.alignment,
                }
            })
            .collect();
        PageObjects {
            page: self.page,
            width: self.run.space.width,
            height: self.run.space.height,
            text_blocks,
            images: (0..self.run.images.len())
                .map(|image| self.image_object(image))
                .collect(),
        }
    }

    /// an image's corners on screen: top left, top right, bottom left,
    /// bottom right. image space puts the top of the picture at y = 1.
    pub fn image_quad(&self, image: usize) -> [Point; 4] {
        let ctm = self.run.images[image].ctm;
        [(0.0, 1.0), (1.0, 1.0), (0.0, 0.0), (1.0, 0.0)]
            .map(|(x, y)| self.run.space.visible_point(ctm.apply(Point::new(x, y))))
    }

    fn image_object(&self, image: usize) -> ImageObject {
        let record = &self.run.images[image];
        let quad = self.image_quad(image);
        let doc_dict = record.object.and_then(|id| self.image_dicts.get(&id));
        let inline = record.inline.as_ref();
        let dict = doc_dict.or(inline);
        let number = |long: &[u8], short: &[u8]| {
            dict.and_then(|dict| dict.get(long).or_else(|_| dict.get(short)).ok())
                .and_then(|value| value.as_float().ok())
                .unwrap_or(0.0)
        };
        let color_space = dict
            .and_then(|dict| dict.get(b"ColorSpace").or_else(|_| dict.get(b"CS")).ok())
            .map(|space| match space {
                lopdf::Object::Name(name) => String::from_utf8_lossy(name).into_owned(),
                lopdf::Object::Array(items) => items
                    .first()
                    .and_then(|item| item.as_name().ok())
                    .map(|name| String::from_utf8_lossy(name).into_owned())
                    .unwrap_or_default(),
                _ => "Named".into(),
            })
            .unwrap_or_default();
        let filter = dict
            .and_then(|dict| dict.get(b"Filter").or_else(|_| dict.get(b"F")).ok())
            .and_then(|filter| match filter {
                lopdf::Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
                lopdf::Object::Array(items) => items
                    .last()
                    .and_then(|item| item.as_name().ok())
                    .map(|name| String::from_utf8_lossy(name).into_owned()),
                _ => None,
            });
        ImageObject {
            id: self.image_id(image),
            rect: Rect::around(quad).expect("four corners"),
            quad,
            pixel_width: number(b"Width", b"W") as u32,
            pixel_height: number(b"Height", b"H") as u32,
            color_space,
            bits_per_component: number(b"BitsPerComponent", b"BPC") as u32,
            has_alpha: dict.is_some_and(|dict| dict.has(b"SMask") || dict.has(b"Mask")),
            inline: record.inline.is_some(),
            filter,
        }
    }
}
