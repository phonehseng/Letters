//! text while running content: each show operator's glyphs, placed with
//! the text and graphics state, and the text position moved after them.

use super::*;

impl Runner<'_> {
    /// records the glyphs of one show operator and moves the text position.
    pub(super) fn show(
        &mut self,
        source: usize,
        op: usize,
        operation: &Operation,
        state: &mut GraphicsState,
    ) {
        let operands = &operation.operands;
        let mut pieces: Vec<&Object> = Vec::new();
        match operation.operator.as_str() {
            "TJ" => {
                if let Some(Object::Array(items)) = operands.first() {
                    pieces.extend(items.iter());
                }
            }
            "\"" => {
                let numbers = numbers_of(operands);
                if numbers.len() >= 2 {
                    state.text.word_spacing = numbers[0];
                    state.text.char_spacing = numbers[1];
                }
                let leading = state.text.leading;
                move_line(&mut state.text, 0.0, -leading);
                pieces.extend(operands.last());
            }
            "'" => {
                let leading = state.text.leading;
                move_line(&mut state.text, 0.0, -leading);
                pieces.extend(operands.last());
            }
            _ => pieces.extend(operands.first()),
        }
        let show_index = self.result.shows.len();
        let before = state.text.clone();
        let mut items = Vec::new();
        for piece in pieces {
            match piece {
                Object::String(bytes, _) => {
                    let glyphs = state
                        .text
                        .font
                        .map(|font| self.result.fonts[font].decode(bytes))
                        .unwrap_or_default();
                    for glyph in glyphs {
                        if let Some(font) = state.text.font {
                            self.result.fonts[font].seen(&glyph.code);
                        }
                        items.push(ShowItem::Glyph(self.result.glyphs.len()));
                        let record = self.glyph(show_index, state, glyph);
                        self.result.glyphs.push(record);
                    }
                }
                other => {
                    if let Ok(adjust) = other.as_float() {
                        let text = &mut state.text;
                        let shift = -adjust / 1000.0 * text.size * text.h_scale;
                        text.text_matrix = Matrix::translate(shift, 0.0).then(&text.text_matrix);
                        items.push(ShowItem::Adjust(adjust));
                    }
                }
            }
        }
        self.result.shows.push(ShowOp {
            source,
            op,
            before,
            text_matrix_after: state.text.text_matrix,
            line_matrix_after: state.text.line_matrix,
            ctm: state.ctm,
            items,
            marked: self.open_marked.last().copied(),
        });
    }

    pub(super) fn glyph(
        &self,
        show: usize,
        state: &mut GraphicsState,
        glyph: crate::fonts::pdf_font::DecodedGlyph,
    ) -> GlyphRecord {
        let text = &state.text;
        let font = text.font.map(|font| &self.result.fonts[font]);
        let (ascent, descent) = font
            .map(|font| (font.ascent, font.descent))
            .unwrap_or((750.0, 250.0));
        let word_space = if glyph.is_word_space {
            text.word_spacing
        } else {
            0.0
        };
        let width = glyph.width / 1000.0 * text.size * text.h_scale;
        let advance =
            (glyph.width / 1000.0 * text.size + text.char_spacing + word_space) * text.h_scale;
        let to_page = text.text_matrix.then(&state.ctm);
        let at = |x: f32, y: f32| to_page.apply(Point::new(x, y));
        let top = text.rise + ascent / 1000.0 * text.size;
        let bottom = text.rise - descent / 1000.0 * text.size;
        let record = GlyphRecord {
            show,
            is_space: glyph.text.chars().all(char::is_whitespace) && !glyph.text.is_empty(),
            text: glyph.text,
            code: glyph.code,
            width: glyph.width,
            font: text.font,
            text_matrix: text.text_matrix,
            advance,
            origin: at(0.0, text.rise),
            end: at(width, text.rise),
            corners: [
                at(0.0, top),
                at(width, top),
                at(0.0, bottom),
                at(width, bottom),
            ],
            size: to_page.apply_vector(Point::new(0.0, text.size)).length(),
            color: text.fill.color,
            invisible: text.render_mode == 3 || text.render_mode == 7,
        };
        state.text.text_matrix = Matrix::translate(advance, 0.0).then(&state.text.text_matrix);
        record
    }
}

pub(super) fn move_line(text: &mut TextState, x: f32, y: f32) {
    text.line_matrix = Matrix::translate(x, y).then(&text.line_matrix);
    text.text_matrix = text.line_matrix;
}
