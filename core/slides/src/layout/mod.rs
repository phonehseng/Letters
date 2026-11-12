//! lays out the text of one shape in its box: bullets, indents, line
//! breaking, line and paragraph spacing, alignment, vertical anchoring
//! and the stored autofit scale. coordinates are points from the top
//! left of the text area (the shape's text rectangle less its insets).
//!
//! core/layout lays out whole word-processor documents (pages, sections,
//! floats); a shape's text needs none of that and follows powerpoint's
//! own spacing rules, so this compact layout is separate. to unify later:
//! the cluster and line-breaking steps are the same ideas as core/layout's
//! `text` and `paragraph` stages and could share their shaping.

pub mod bullets;
mod lines;
pub mod runs;

use crate::fonts::{FontId, FontLibrary};
use crate::model::{Anchor, Spacing, TextAlign};
use lines::{break_lines, line_extent, LineRange};
use runs::{clusters, Cluster, StyledRun};

/// a paragraph ready for layout: resolved properties and runs.
#[derive(Clone, Debug)]
pub struct ParagraphInput {
    pub runs: Vec<StyledRun>,
    /// the paragraph mark's run, which gives an empty paragraph its height
    pub mark: StyledRun,
    pub align: TextAlign,
    pub margin_left: f64,
    pub margin_right: f64,
    pub indent: f64,
    pub line_spacing: Spacing,
    pub space_before: Spacing,
    pub space_after: Spacing,
    pub bullet: Option<StyledRun>,
}

/// the text area and how text sits in it.
#[derive(Clone, Debug)]
pub struct BoxInput {
    pub width: f64,
    pub height: f64,
    pub anchor: Anchor,
    pub wrap: bool,
    /// autofit as stored: a font scale and a line spacing reduction (both
    /// fractions: 0.625 and 0.2)
    pub font_scale: f64,
    pub line_reduction: f64,
    pub columns: u32,
    pub column_spacing: f64,
    /// shrink on overflow without a stored scale: work one out
    pub shrink_to_fit: bool,
}

/// glyphs of one font at one size on a line.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub font: FontId,
    pub size: f64,
    /// (glyph, x of its origin, the character it draws)
    pub glyphs: Vec<(u16, f64, char)>,
    /// the run it belongs to; `None` for the bullet
    pub run: Option<usize>,
    pub baseline_shift: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LaidLine {
    pub paragraph: usize,
    pub top: f64,
    pub height: f64,
    pub baseline: f64,
    pub left: f64,
    pub width: f64,
    pub pieces: Vec<Piece>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaidText {
    pub lines: Vec<LaidLine>,
    /// from the first line's top to the last line's bottom
    pub content_height: f64,
    /// the text is taller than its box
    pub overflows: bool,
}

/// single line spacing as a multiple of the font size: powerpoint's line
/// pitch for 90 % spacing of 24 pt calibri is 25.9 pt (FIDELITY_REPORT.md,
/// cause 1), 1.2 times the size times 0.9, for every font measured
pub const SINGLE_LINE_FACTOR: f64 = 1.2;

pub fn layout_box(
    input: &BoxInput,
    paragraphs: &[ParagraphInput],
    fonts: &mut FontLibrary,
) -> LaidText {
    let laid = layout_at_scale(input, paragraphs, fonts);
    if !(input.shrink_to_fit && laid.overflows) {
        return laid;
    }
    shrunk_to_fit(input, paragraphs, fonts)
}

/// shrink on overflow with no stored scale (opendocument's `shrink-to-fit`
/// and new text): the largest scale, found by halving the interval eight
/// times, at which the text fits. the line spacing shrinks with it by up
/// to 20 percentage points, as powerpoint's own autofit does.
fn shrunk_to_fit(
    input: &BoxInput,
    paragraphs: &[ParagraphInput],
    fonts: &mut FontLibrary,
) -> LaidText {
    let (mut low, mut high) = (MINIMUM_FIT_SCALE, 1.0);
    for _ in 0..8 {
        let middle = (low + high) / 2.0;
        if layout_at_scale(&scaled_input(input, middle), paragraphs, fonts).overflows {
            high = middle;
        } else {
            low = middle;
        }
    }
    layout_at_scale(&scaled_input(input, low), paragraphs, fonts)
}

fn scaled_input(input: &BoxInput, scale: f64) -> BoxInput {
    BoxInput {
        font_scale: scale,
        line_reduction: ((1.0 - scale) * 0.5).min(0.2),
        ..input.clone()
    }
}

/// powerpoint does not shrink text below a quarter of its size
const MINIMUM_FIT_SCALE: f64 = 0.25;

fn layout_at_scale(
    input: &BoxInput,
    paragraphs: &[ParagraphInput],
    fonts: &mut FontLibrary,
) -> LaidText {
    let columns = input.columns.max(1);
    let column_width =
        (input.width - input.column_spacing * f64::from(columns - 1)) / f64::from(columns);
    let mut laid = LaidText::default();
    let mut cursor = 0.0;
    for (index, paragraph) in paragraphs.iter().enumerate() {
        let scaled = scale_paragraph(paragraph, input.font_scale);
        cursor += space(&scaled.space_before, &scaled, fonts, index == 0);
        let lines = layout_paragraph(index, &scaled, column_width, input, fonts, cursor);
        if let Some(last) = lines.last() {
            cursor = last.top + last.height;
        }
        laid.lines.extend(lines);
        cursor += space(&scaled.space_after, &scaled, fonts, false);
    }
    laid.content_height = cursor;
    flow_columns(&mut laid, input, column_width);
    anchor_lines(&mut laid, input);
    laid
}

/// sizes multiplied by the stored autofit scale.
fn scale_paragraph(paragraph: &ParagraphInput, scale: f64) -> ParagraphInput {
    let mut scaled = paragraph.clone();
    for run in scaled
        .runs
        .iter_mut()
        .chain(std::iter::once(&mut scaled.mark))
    {
        run.size *= scale;
    }
    if let Some(bullet) = &mut scaled.bullet {
        bullet.size *= scale;
    }
    scaled
}

/// space before or after a paragraph. a percentage is of the
/// paragraph's single line height.
fn space(
    spacing: &Spacing,
    paragraph: &ParagraphInput,
    fonts: &mut FontLibrary,
    first: bool,
) -> f64 {
    // powerpoint does not add space before the first paragraph of a box;
    // checked against its pdfs in FIDELITY_REPORT.md (first baselines)
    if first {
        return 0.0;
    }
    match spacing {
        Spacing::Points(hundredths) => f64::from(*hundredths) / 100.0,
        Spacing::Percent(thousandths) => {
            let (size, _) = mark_extent(paragraph, fonts);
            size * SINGLE_LINE_FACTOR * f64::from(*thousandths) / 100_000.0
        }
    }
}

fn mark_extent(paragraph: &ParagraphInput, fonts: &mut FontLibrary) -> (f64, f64) {
    let marks = clusters(std::slice::from_ref(&paragraph.mark), fonts);
    let font = marks.first().map_or(0, |c| c.font);
    line_extent(&[], fonts, (font, paragraph.mark.size))
}

fn layout_paragraph(
    index: usize,
    paragraph: &ParagraphInput,
    column_width: f64,
    input: &BoxInput,
    fonts: &mut FontLibrary,
    top: f64,
) -> Vec<LaidLine> {
    let items = clusters(&paragraph.runs, fonts);
    let bullet = paragraph
        .bullet
        .as_ref()
        .map(|run| clusters(std::slice::from_ref(run), fonts))
        .filter(|b| !b.is_empty() && !items.is_empty());
    let first_start = first_line_start(paragraph, bullet.as_deref());
    let rest_start = paragraph.margin_left;
    let first_width = column_width - first_start - paragraph.margin_right;
    let rest_width = column_width - rest_start - paragraph.margin_right;
    let ranges = if items.is_empty() {
        vec![LineRange {
            start: 0,
            end: 0,
            width: 0.0,
        }]
    } else {
        break_lines(&items, first_width, rest_width, input.wrap)
    };
    let mark_font = clusters(std::slice::from_ref(&paragraph.mark), fonts)
        .first()
        .map_or(0, |c| c.font);
    let mut lines = Vec::new();
    let mut cursor = top;
    for (number, range) in ranges.iter().enumerate() {
        let start = if number == 0 { first_start } else { rest_start };
        let available = if number == 0 { first_width } else { rest_width };
        let slice = &items[range.start..range.end];
        let (size, ascent_share) = line_extent(slice, fonts, (mark_font, paragraph.mark.size));
        let (height, baseline_offset) = line_height(paragraph, input, size, ascent_share);
        let mut line = LaidLine {
            paragraph: index,
            top: cursor,
            height,
            baseline: cursor + baseline_offset,
            left: start,
            width: range.width,
            pieces: pieces(slice, &paragraph.runs),
        };
        let is_last = number + 1 == ranges.len();
        align_line(&mut line, paragraph.align, available, is_last);
        if number == 0 {
            if let Some(bullet) = &bullet {
                line.pieces.insert(0, bullet_piece(bullet, paragraph));
            }
        }
        cursor += height;
        lines.push(line);
    }
    lines
}

/// where the first line's text starts: the indent applies to the first
/// line, and with a bullet the text starts at the margin unless the
/// bullet does not fit in the hanging space.
fn first_line_start(paragraph: &ParagraphInput, bullet: Option<&[Cluster]>) -> f64 {
    let indented = (paragraph.margin_left + paragraph.indent).max(0.0);
    match bullet {
        None => indented,
        Some(label) => {
            let label_end = indented + label.iter().map(|c| c.advance).sum::<f64>();
            paragraph.margin_left.max(label_end)
        }
    }
}

/// a line's height and where its baseline sits below its top. a
/// percentage is of 1.2 times the biggest size on the line, whatever the
/// font, and the baseline divides the line in the font's ascent to
/// descent proportion (FIDELITY_REPORT.md, cause 1). points set the
/// height directly.
fn line_height(
    paragraph: &ParagraphInput,
    input: &BoxInput,
    size: f64,
    ascent_share: f64,
) -> (f64, f64) {
    let height = match paragraph.line_spacing {
        Spacing::Percent(thousandths) => {
            let factor = (f64::from(thousandths) / 100_000.0 - input.line_reduction).max(0.0);
            size * SINGLE_LINE_FACTOR * factor
        }
        Spacing::Points(hundredths) => f64::from(hundredths) / 100.0 * input.font_scale,
    };
    (height, height * ascent_share)
}
/// glyphs grouped into pieces of one run each, x from the line start.
fn pieces(slice: &[Cluster], runs: &[StyledRun]) -> Vec<Piece> {
    let mut pieces: Vec<Piece> = Vec::new();
    let mut x = 0.0;
    for cluster in slice {
        if cluster.ch == '\n' {
            continue;
        }
        let same = pieces
            .last()
            .is_some_and(|p| p.run == Some(cluster.run) && p.font == cluster.font);
        if !same {
            let shift = runs
                .get(cluster.run)
                .map_or(0.0, |r| r.baseline_shift * r.size);
            pieces.push(Piece {
                font: cluster.font,
                size: cluster.size,
                glyphs: Vec::new(),
                run: Some(cluster.run),
                baseline_shift: shift,
            });
        }
        if let Some(piece) = pieces.last_mut() {
            piece.glyphs.push((cluster.glyph, x, cluster.ch));
        }
        x += cluster.advance;
    }
    pieces
}

fn bullet_piece(bullet: &[Cluster], paragraph: &ParagraphInput) -> Piece {
    let start = (paragraph.margin_left + paragraph.indent).max(0.0);
    let mut x = start;
    let mut glyphs = Vec::new();
    for cluster in bullet {
        glyphs.push((cluster.glyph, x, cluster.ch));
        x += cluster.advance;
    }
    Piece {
        font: bullet[0].font,
        size: bullet[0].size,
        glyphs,
        run: None,
        baseline_shift: 0.0,
    }
}

/// moves a line's text for its alignment; the bullet stays put.
fn align_line(line: &mut LaidLine, align: TextAlign, available: f64, is_last: bool) {
    let free = (available - line.width).max(0.0);
    let shift = match align {
        TextAlign::Center => free / 2.0,
        TextAlign::Right => free,
        _ => 0.0,
    };
    let justify = matches!(align, TextAlign::Justify | TextAlign::Distributed) && !is_last;
    let spaces: usize = line
        .pieces
        .iter()
        .flat_map(|p| &p.glyphs)
        .filter(|g| g.2 == ' ')
        .count();
    let extra = if justify && spaces > 0 {
        free / spaces as f64
    } else {
        0.0
    };
    let mut added = 0.0;
    for piece in &mut line.pieces {
        for glyph in &mut piece.glyphs {
            glyph.1 += line.left + shift + added;
            if glyph.2 == ' ' {
                added += extra;
            }
        }
    }
    line.left += shift;
}

/// spreads lines over columns: a column takes lines until the box
/// height is used, then the next column starts at the top.
fn flow_columns(laid: &mut LaidText, input: &BoxInput, column_width: f64) {
    if input.columns <= 1 {
        return;
    }
    let mut column = 0u32;
    let mut offset = 0.0;
    for line in &mut laid.lines {
        if line.top + line.height - offset > input.height && column + 1 < input.columns {
            column += 1;
            offset = line.top;
        }
        let shift_x = f64::from(column) * (column_width + input.column_spacing);
        move_line(line, shift_x, -offset);
    }
}

/// vertical anchoring: top, middle or bottom of the text area.
fn anchor_lines(laid: &mut LaidText, input: &BoxInput) {
    let height = laid.content_height;
    laid.overflows = height > input.height + 0.01;
    let free = input.height - height;
    let shift = match input.anchor {
        Anchor::Middle => free / 2.0,
        Anchor::Bottom => free,
        _ => 0.0,
    };
    for line in &mut laid.lines {
        move_line(line, 0.0, shift);
    }
}

fn move_line(line: &mut LaidLine, dx: f64, dy: f64) {
    line.top += dy;
    line.baseline += dy;
    line.left += dx;
    for piece in &mut line.pieces {
        for glyph in &mut piece.glyphs {
            glyph.1 += dx;
        }
    }
}
