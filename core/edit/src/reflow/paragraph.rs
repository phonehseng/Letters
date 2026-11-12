//! a paragraph as reflow sees it, and setting new text into its lines:
//! wrapped to its width, aligned as it was (justified stays justified),
//! with letter spacing, word spacing and leading that tightening can
//! reduce, and the font's kerning pairs as the page uses them.

use std::collections::HashMap;

use crate::colors::Color;
use crate::geometry::{Matrix, Point, Rect};
use crate::layout::{LineModel, PageModel};
use crate::model::TextAlignment;
use crate::text_edit::{look_of, to_page, wrap};
use crate::text_fonts::FontChoice;
use crate::text_write::{advance, FontRef, Placed};

/// line spacing as a multiple of the size when a paragraph has one line
/// and nothing on the page shows its real spacing: the usual default of
/// word processors and pdf writers.
const DEFAULT_LEADING: f32 = 1.2;

/// what reflow needs to know about the paragraph being edited.
#[derive(Clone, Debug)]
pub(crate) struct Paragraph {
    pub block: usize,
    pub anchor_glyph: usize,
    pub anchor_show: usize,
    /// the text matrix of the first glyph: new lines are measured from it
    pub anchor_matrix: Matrix,
    pub size: f32,
    pub rise: f32,
    pub color: Color,
    /// the edges along the first line, in its text space
    pub left: f32,
    pub right: f32,
    /// the distance between baselines in text space and on screen
    pub line_step: f32,
    pub leading: f32,
    pub alignment: TextAlignment,
    pub lines_before: usize,
    /// the block's box on screen
    pub rect: Rect,
    /// screen points per text-space unit along the line
    pub along_scale: f32,
}

/// how tightly text is set. normal spacing is all zeros and ones.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Spacing {
    /// added to every character's advance, in ems (negative is tighter)
    pub letter_em: f32,
    /// word spaces as a share of their own width
    pub word_scale: f32,
    /// baseline distance as a share of the original
    pub leading_scale: f32,
}

impl Spacing {
    pub const NORMAL: Spacing = Spacing {
        letter_em: 0.0,
        word_scale: 1.0,
        leading_scale: 1.0,
    };
}

/// the new lines of a paragraph and their placed glyphs.
pub(crate) struct SetText {
    /// each line's text and whether it ends a paragraph
    pub lines: Vec<(String, bool)>,
    pub placed: Vec<Placed>,
}

/// screen points per text-space unit in a direction of the text matrix.
fn scale_along(model: &PageModel, matrix: &Matrix, direction: Point) -> f32 {
    model
        .run
        .space
        .visible_vector(matrix.apply_vector(direction))
        .length()
        .max(1e-6)
}

/// reads the paragraph: its anchor, look, edges and line spacing.
pub(crate) fn paragraph_of(model: &PageModel, block_index: usize, rect: Rect) -> Paragraph {
    let block = &model.blocks[block_index];
    let first_line = &block.lines[0];
    let anchor_glyph = PageModel::first_glyph(first_line);
    let anchor_page = to_page(model, anchor_glyph);
    let along_scale = scale_along(model, &anchor_page, Point::new(1.0, 0.0));
    let across_scale = scale_along(model, &anchor_page, Point::new(0.0, 1.0));
    let leading = typical_leading(model, block_index);
    let along = |line: &LineModel| {
        line.start.minus(first_line.start).dot(first_line.direction) / along_scale
    };
    let left = block.lines.iter().map(along).fold(f32::MAX, f32::min);
    let right = block
        .lines
        .iter()
        .map(|line| along(line) + line.length / along_scale)
        .fold(f32::MIN, f32::max);
    let (size, rise, color) = look_of(model, anchor_glyph);
    Paragraph {
        block: block_index,
        anchor_glyph,
        anchor_show: model.run.glyphs[anchor_glyph].show,
        anchor_matrix: model.run.glyphs[anchor_glyph].text_matrix,
        size,
        rise,
        color,
        left,
        right,
        line_step: leading / across_scale,
        leading,
        alignment: block.alignment,
        lines_before: block.lines.len(),
        rect,
        along_scale,
    }
}

/// the block's own line spacing, or that of a block of the same size on
/// the page, or the usual default.
fn typical_leading(model: &PageModel, block_index: usize) -> f32 {
    let block = &model.blocks[block_index];
    if let Some(leading) = block.leading {
        return leading;
    }
    let size = block.lines[0].size;
    model
        .blocks
        .iter()
        .filter(|other| (other.lines[0].size - size).abs() < 0.05)
        .find_map(|other| other.leading)
        .unwrap_or(size * DEFAULT_LEADING)
}

/// everything that decides where each character goes.
pub(crate) struct Typesetter<'a> {
    pub model: &'a PageModel,
    pub paragraph: &'a Paragraph,
    pub choice: &'a FontChoice,
    pub spacing: Spacing,
    /// TJ numbers between character pairs in the original font
    pub kerning: &'a HashMap<(char, char), f32>,
}

impl Typesetter<'_> {
    fn state(&self) -> &crate::interpret::TextState {
        &self.model.run.shows[self.paragraph.anchor_show].before
    }

    /// the font a character is written in, and its code and width.
    fn glyph_of(&self, character: char) -> Option<(usize, &crate::fonts::pdf_font::EncodedChar)> {
        let font = *self.choice.by_char.get(&character)?;
        let code = self.choice.fonts[font].codes.get(&character)?;
        Some((font, code))
    }

    /// how far one character moves the pen, with this spacing.
    fn char_advance(&self, character: char) -> f32 {
        let state = self.state();
        let size = self.paragraph.size;
        let letter = self.spacing.letter_em * size * state.h_scale;
        match self.glyph_of(character) {
            Some((_, code)) if character == ' ' => {
                advance(
                    state,
                    code.width * self.spacing.word_scale,
                    size,
                    code.is_word_space,
                ) + letter
            }
            Some((_, code)) => advance(state, code.width, size, code.is_word_space) + letter,
            // a space no font here draws becomes a plain move a quarter wide
            None => advance(state, 250.0 * self.spacing.word_scale, size, false) + letter,
        }
    }

    /// the kerning move between two characters, in text space.
    fn kern(&self, previous: char, next: char) -> f32 {
        let both_original = matches!(
            (self.glyph_of(previous), self.glyph_of(next)),
            (Some((0, _)), Some((0, _)))
        );
        match self.kerning.get(&(previous, next)) {
            Some(number) if both_original => {
                -number / 1000.0 * self.paragraph.size * self.state().h_scale
            }
            _ => 0.0,
        }
    }

    /// the width of some text as it would be set.
    pub fn measure(&self, text: &str) -> f32 {
        let mut width = 0.0;
        let mut previous: Option<char> = None;
        for character in text.chars() {
            if let Some(before) = previous {
                width += self.kern(before, character);
            }
            width += self.char_advance(character);
            previous = Some(character);
        }
        width
    }

    /// wraps the text to the paragraph's width and places every glyph.
    pub fn set(&self, text: &str) -> SetText {
        let width = self.paragraph.right - self.paragraph.left;
        let measure = |piece: &str| self.measure(piece);
        let lines = wrap(text, width, &measure);
        let mut placed = Vec::new();
        for (number, (line_text, ends_paragraph)) in lines.iter().enumerate() {
            placed.extend(self.place_line(line_text, *ends_paragraph, number));
        }
        SetText { lines, placed }
    }

    /// where a line starts and how much each word space grows, for the
    /// paragraph's alignment.
    fn line_start(&self, line_text: &str, ends_paragraph: bool) -> (f32, f32) {
        let paragraph = self.paragraph;
        let width = paragraph.right - paragraph.left;
        let natural = self.measure(line_text);
        let spaces = line_text.chars().filter(|c| *c == ' ').count();
        match paragraph.alignment {
            TextAlignment::Left => (paragraph.left, 0.0),
            TextAlignment::Right => (paragraph.right - natural, 0.0),
            TextAlignment::Center => (paragraph.left + (width - natural) / 2.0, 0.0),
            // justified lines fill the width; the last line of a paragraph
            // stays ragged, and no hyphens are ever added
            TextAlignment::Justified if !ends_paragraph && spaces > 0 => {
                (paragraph.left, (width - natural).max(0.0) / spaces as f32)
            }
            TextAlignment::Justified => (paragraph.left, 0.0),
        }
    }

    fn place_line(&self, line_text: &str, ends_paragraph: bool, number: usize) -> Vec<Placed> {
        let (mut x, extra_per_space) = self.line_start(line_text, ends_paragraph);
        let mut placed = Vec::new();
        let mut previous: Option<char> = None;
        for character in line_text.chars() {
            if let Some(before) = previous {
                x += self.kern(before, character);
            }
            if let Some((font, code)) = self.glyph_of(character) {
                placed.push(Placed {
                    x,
                    line: number,
                    font: FontRef::Object(self.choice.fonts[font].object),
                    size: self.paragraph.size,
                    rise: self.paragraph.rise,
                    color: self.paragraph.color,
                    code: Some(code.code.clone()),
                    width: code.width,
                    is_word_space: code.is_word_space,
                });
            }
            x += self.char_advance(character);
            if character == ' ' {
                x += extra_per_space;
            }
            previous = Some(character);
        }
        placed
    }

    /// the distance between baselines in text space, with this spacing.
    pub fn line_step(&self) -> f32 {
        self.paragraph.line_step * self.spacing.leading_scale
    }

    /// the distance between baselines on screen, with this spacing.
    pub fn leading(&self) -> f32 {
        self.paragraph.leading * self.spacing.leading_scale
    }
}
