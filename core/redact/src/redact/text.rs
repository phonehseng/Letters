//! text: following the text state and taking single glyphs out of the
//! strings that `Tj`, `TJ`, `'` and `"` show.
//!
//! every glyph's box is worked out from the font's widths and the text
//! state, exactly as a viewer places it. a glyph whose box overlaps a
//! redaction area is removed from its string, and the gap it leaves is
//! filled with a `TJ` number that moves the pen by the same distance the
//! glyph would have. so the glyphs that stay are drawn exactly where they
//! were, and the removed ones are simply not in the file any more.
//!
//! when a font's widths are not in the file (`metrics_known` is false),
//! the positions of glyphs after the first are not known. then the whole
//! string is treated as reaching as far as it possibly could, and if that
//! could touch an area, the string and the rest of its line are removed.

use std::rc::Rc;

use lopdf::{Object, StringFormat};

use crate::content::Op;
use crate::fonts::Font;
use crate::geometry::{Matrix, Rect};
use crate::objects::number;
use crate::redact::walker::{matrix_operand, GlyphRecord, UncertainLine, WalkMode, Walker};

/// one glyph of a shown string, or a `TJ` spacing number.
enum Piece {
    Glyph {
        bytes: Vec<u8>,
        format: StringFormat,
        removed: bool,
        /// how far the glyph moves the pen, in text space
        advance: f64,
    },
    Spacing(f64),
}

impl Walker<'_, '_> {
    /// handles text state, positioning and showing operators. returns
    /// false for anything else.
    pub fn handle_text(&mut self, op: &Op) -> bool {
        let doc = self.doc();
        let operands = op.operands();
        let first = || operands.first().and_then(|o| number(doc, o));
        let text = &mut self.state.text;
        match op.name() {
            "BT" => {
                self.text_matrix = Matrix::IDENTITY;
                self.line_matrix = Matrix::IDENTITY;
                self.new_line_reset();
            }
            "ET" => {}
            "Tc" => text.char_spacing = first().unwrap_or(0.0),
            "Tw" => text.word_spacing = first().unwrap_or(0.0),
            "Tz" => text.scale = first().unwrap_or(100.0) / 100.0,
            "TL" => text.leading = first().unwrap_or(0.0),
            "Ts" => text.rise = first().unwrap_or(0.0),
            "Tr" => {}
            "Tf" => self.set_font(operands),
            "Td" | "TD" => {
                let x = first().unwrap_or(0.0);
                let y = operands.get(1).and_then(|o| number(doc, o)).unwrap_or(0.0);
                if op.name() == "TD" {
                    self.state.text.leading = -y;
                }
                self.move_line(x, y);
            }
            "Tm" => {
                if let Some(matrix) = matrix_operand(doc, operands) {
                    self.text_matrix = matrix;
                    self.line_matrix = matrix;
                }
                self.new_line_reset();
            }
            "T*" => self.next_line(),
            "Tj" | "TJ" | "'" | "\"" => {
                self.show_text(op.clone());
                return true;
            }
            _ => return false,
        }
        self.emit(op.clone());
        true
    }

    fn set_font(&mut self, operands: &[Object]) {
        let size = operands
            .get(1)
            .and_then(|o| number(self.doc(), o))
            .unwrap_or(0.0);
        self.state.text.size = size;
        if let Some(Object::Name(name)) = operands.first() {
            let font = self.load_font_by_name(name);
            self.state.text.font = Some(font);
            self.state.text.font_name = Some(name.clone());
        }
    }

    fn move_line(&mut self, x: f64, y: f64) {
        self.line_matrix = Matrix::translate(x, y).then(&self.line_matrix);
        self.text_matrix = self.line_matrix;
        self.new_line_reset();
    }

    fn next_line(&mut self) {
        let leading = self.state.text.leading;
        self.move_line(0.0, -leading);
    }

    /// a new line starts from a known place again.
    fn new_line_reset(&mut self) {
        self.uncertain = None;
        self.drop_rest_of_line = false;
    }

    fn current_font(&self) -> Rc<Font> {
        self.state
            .text
            .font
            .clone()
            .unwrap_or_else(|| Rc::new(Font::unknown("no font")))
    }

    fn show_text(&mut self, op: Op) {
        let doc = self.doc();
        let operands = op.operands().to_vec();
        let mut prefix = Vec::new();
        let shown = match op.name() {
            "TJ" => match operands.first() {
                Some(Object::Array(items)) => items.clone(),
                _ => Vec::new(),
            },
            "'" => {
                self.next_line();
                prefix.push(Op::new("T*", vec![]));
                operands.first().cloned().into_iter().collect()
            }
            "\"" => {
                let word = operands.first().and_then(|o| number(doc, o)).unwrap_or(0.0);
                let char = operands.get(1).and_then(|o| number(doc, o)).unwrap_or(0.0);
                self.state.text.word_spacing = word;
                self.state.text.char_spacing = char;
                self.next_line();
                prefix.push(Op::new(
                    "Tw",
                    vec![operands.first().cloned().unwrap_or(Object::Integer(0))],
                ));
                prefix.push(Op::new(
                    "Tc",
                    vec![operands.get(1).cloned().unwrap_or(Object::Integer(0))],
                ));
                prefix.push(Op::new("T*", vec![]));
                operands.get(2).cloned().into_iter().collect()
            }
            _ => operands.first().cloned().into_iter().collect(),
        };
        let font = self.current_font();
        if !font.metrics_known || self.uncertain.is_some() || self.drop_rest_of_line {
            self.show_uncertain(op, prefix, &shown, &font);
            return;
        }
        let pieces = self.lay_out(&shown, &font, None);
        let removed = pieces
            .iter()
            .filter(|piece| matches!(piece, Piece::Glyph { removed: true, .. }))
            .count();
        if removed == 0 {
            self.emit(op);
            return;
        }
        self.ctx.findings.counts.glyphs_removed += removed;
        self.touch();
        for op in prefix {
            self.emit(op);
        }
        self.emit_rebuilt(pieces, &font);
    }

    /// moves the pen across the shown strings, working out every glyph's
    /// box and whether an area removes it. `forced` overrides the area
    /// test (used when positions are only guesses).
    fn lay_out(
        &mut self,
        shown: &[Object],
        font: &Font,
        forced: Option<Option<usize>>,
    ) -> Vec<Piece> {
        let doc = self.doc();
        let mut pieces = Vec::new();
        for item in shown {
            match item {
                Object::String(bytes, format) => {
                    for (code, length, start) in codes_with_offsets(font, bytes) {
                        let glyph_bytes = bytes[start..start + length].to_vec();
                        let advance = self.glyph_advance(font, code, length);
                        let quad = font.glyph_box(code).transform(&self.glyph_matrix());
                        let area = forced.unwrap_or_else(|| self.first_area_hit(&quad));
                        self.record_glyph(font.text(code), quad, area);
                        self.move_pen(font, advance);
                        pieces.push(Piece::Glyph {
                            bytes: glyph_bytes,
                            format: *format,
                            removed: area.is_some(),
                            advance,
                        });
                    }
                }
                other => {
                    if let Some(amount) = number(doc, other) {
                        let advance = self.spacing_advance(font, amount);
                        self.move_pen(font, advance);
                        pieces.push(Piece::Spacing(amount));
                    }
                }
            }
        }
        pieces
    }

    fn record_glyph(&mut self, text: String, quad: crate::geometry::Quad, area: Option<usize>) {
        let collect = self.settings().mode == WalkMode::Collect;
        if collect || area.is_some() {
            self.ctx
                .findings
                .glyphs
                .push(GlyphRecord { text, quad, area });
        }
    }

    /// maps a glyph box (font size 1, pen at the origin) to page space.
    fn glyph_matrix(&self) -> Matrix {
        let text = &self.state.text;
        Matrix::new(text.size * text.scale, 0.0, 0.0, text.size, 0.0, text.rise)
            .then(&self.text_matrix)
            .then(&self.state.ctm)
    }

    /// how far a glyph moves the pen: its width at the font size, plus
    /// character spacing, plus word spacing for a space, all scaled
    /// horizontally. in vertical writing the pen moves down instead.
    fn glyph_advance(&self, font: &Font, code: u32, length: usize) -> f64 {
        let text = &self.state.text;
        let mut spacing = text.char_spacing;
        if font.is_word_space(code, length) {
            spacing += text.word_spacing;
        }
        if font.vertical {
            -text.size + spacing
        } else {
            (font.advance(code) * text.size + spacing) * text.scale
        }
    }

    /// a TJ number moves the pen back by thousandths of the font size.
    fn spacing_advance(&self, font: &Font, amount: f64) -> f64 {
        let text = &self.state.text;
        if font.vertical {
            -amount / 1000.0 * text.size
        } else {
            -amount / 1000.0 * text.size * text.scale
        }
    }

    fn move_pen(&mut self, font: &Font, advance: f64) {
        let step = if font.vertical {
            Matrix::translate(0.0, advance)
        } else {
            Matrix::translate(advance, 0.0)
        };
        self.text_matrix = step.then(&self.text_matrix);
    }

    /// the number that moves the pen as far as `advance`, when the font
    /// size and scaling allow it.
    fn spacing_for(&self, font: &Font, advance: f64) -> Option<f64> {
        let text = &self.state.text;
        let unit = if font.vertical {
            text.size
        } else {
            text.size * text.scale
        };
        (unit.abs() > 1e-9).then(|| -advance * 1000.0 / unit)
    }

    /// writes the shown strings again without the removed glyphs.
    fn emit_rebuilt(&mut self, pieces: Vec<Piece>, font: &Font) {
        let text = &self.state.text;
        let unit = if font.vertical {
            text.size
        } else {
            text.size * text.scale
        };
        if unit.abs() <= 1e-9 {
            self.emit_rebuilt_without_size(pieces, font);
            return;
        }
        let mut array = Vec::new();
        let mut run: Vec<u8> = Vec::new();
        let mut run_format = StringFormat::Literal;
        let mut spacing = 0.0;
        for piece in pieces {
            match piece {
                Piece::Glyph {
                    bytes,
                    format,
                    removed: false,
                    ..
                } => {
                    if spacing != 0.0 {
                        push_run(&mut array, &mut run, run_format);
                        array.push(spacing_number(spacing));
                        spacing = 0.0;
                    }
                    run_format = format;
                    run.extend_from_slice(&bytes);
                }
                Piece::Glyph { advance, .. } => {
                    push_run(&mut array, &mut run, run_format);
                    spacing += self.spacing_for(font, advance).unwrap_or(0.0);
                }
                Piece::Spacing(amount) => {
                    push_run(&mut array, &mut run, run_format);
                    spacing += amount;
                }
            }
        }
        push_run(&mut array, &mut run, run_format);
        if spacing != 0.0 {
            array.push(spacing_number(spacing));
        }
        self.emit(Op::new("TJ", vec![Object::Array(array)]));
    }

    /// with a font size of 0 (or no horizontal scaling) a TJ number moves
    /// nothing, yet character and word spacing still move the pen. the
    /// font size is set to 1 just long enough to make up that distance.
    fn emit_rebuilt_without_size(&mut self, pieces: Vec<Piece>, font: &Font) {
        let text = self.state.text.clone();
        let font_name = text.font_name.clone();
        let mut gap = 0.0;
        let mut kept = Vec::new();
        for piece in pieces {
            match piece {
                Piece::Glyph {
                    bytes,
                    format,
                    removed: false,
                    ..
                } => {
                    self.make_up_gap(&mut gap, font, &text, font_name.as_deref());
                    kept.push(Object::String(bytes, format));
                    self.emit(Op::new(
                        "TJ",
                        vec![Object::Array(std::mem::take(&mut kept))],
                    ));
                }
                Piece::Glyph { advance, .. } => gap += advance,
                Piece::Spacing(_) => {}
            }
        }
        self.make_up_gap(&mut gap, font, &text, font_name.as_deref());
    }

    fn make_up_gap(
        &mut self,
        gap: &mut f64,
        font: &Font,
        text: &crate::redact::walker::TextParams,
        font_name: Option<&[u8]>,
    ) {
        if gap.abs() < 1e-12 {
            return;
        }
        let scale = if font.vertical { 1.0 } else { text.scale };
        match font_name {
            Some(name) if scale.abs() > 1e-9 => {
                let amount = -*gap * 1000.0 / scale;
                self.emit(Op::new("Tf", vec![Object::Name(name.to_vec()), Object::Integer(1)]));
                self.emit(Op::new("TJ", vec![Object::Array(vec![spacing_number(amount)])]));
                self.emit(Op::new("Tf", vec![Object::Name(name.to_vec()), Object::Real(text.size as f32)]));
            }
            _ => self
                .ctx
                .findings
                .wholesale
                .push("the spacing after some removed letters could not be kept, so text after them may have moved".into()),
        }
        *gap = 0.0;
    }

    /// text whose glyph positions are not known exactly.
    fn show_uncertain(&mut self, op: Op, prefix: Vec<Op>, shown: &[Object], font: &Font) {
        let reach = self.possible_reach(shown, font);
        let line = UncertainLine {
            start: self
                .uncertain
                .map(|line| line.start)
                .unwrap_or(self.text_matrix),
            length: self.uncertain.map(|line| line.length).unwrap_or(0.0) + reach,
        };
        self.uncertain = Some(line);
        let text = &self.state.text;
        let low = font.descent.min(-0.25) * text.size.abs() - text.rise.abs();
        let high = font.ascent.max(1.0) * text.size.abs() + text.rise.abs();
        let extent = Rect::from_corners(-line.length, low, line.length, high);
        let quad = extent.transform(&line.start.then(&self.state.ctm));
        let area = self.first_area_hit(&quad);
        let remove = self.drop_rest_of_line || area.is_some();
        // the glyphs are reported under the area that reached them, or the
        // page's first area when they go because their line went
        let tag = area.or_else(|| self.settings().area_ids.first().copied());
        let pieces = self.lay_out(shown, font, Some(if remove { tag } else { None }));
        if !remove {
            self.emit(op);
            return;
        }
        let removed = pieces
            .iter()
            .filter(|piece| matches!(piece, Piece::Glyph { .. }))
            .count();
        self.ctx.findings.counts.glyphs_removed += removed;
        if !self.drop_rest_of_line {
            self.ctx.findings.wholesale.push(format!(
                "a line of text in the font {} was removed from where the box starts to the end of the line, because the file does not say how wide its letters are",
                font.name
            ));
        }
        self.drop_rest_of_line = true;
        self.touch();
        for op in prefix {
            self.emit(op);
        }
    }

    /// the furthest a string could move the pen, using the larger of each
    /// glyph's known width and a full em.
    fn possible_reach(&self, shown: &[Object], font: &Font) -> f64 {
        let doc = self.doc();
        let text = &self.state.text;
        let size = text.size.abs();
        let spacing = text.char_spacing.abs() + text.word_spacing.abs();
        let mut reach = 0.0;
        for item in shown {
            match item {
                Object::String(bytes, _) => {
                    for (code, _) in font.codes(bytes) {
                        reach += (font.advance(code).abs().max(1.0) * size + spacing)
                            * text.scale.abs().max(1.0);
                    }
                }
                other => {
                    if let Some(amount) = number(doc, other) {
                        reach += amount.abs() / 1000.0 * size * text.scale.abs().max(1.0);
                    }
                }
            }
        }
        reach
    }
}

/// codes of a string with the byte offset each starts at.
fn codes_with_offsets(font: &Font, bytes: &[u8]) -> Vec<(u32, usize, usize)> {
    let mut offset = 0;
    font.codes(bytes)
        .into_iter()
        .map(|(code, length)| {
            let start = offset;
            offset += length;
            (code, length, start)
        })
        .collect()
}

fn push_run(array: &mut Vec<Object>, run: &mut Vec<u8>, format: StringFormat) {
    if !run.is_empty() {
        array.push(Object::String(std::mem::take(run), format));
    }
}

/// spacing numbers are written with enough precision that the pen lands
/// within a millionth of a point of where it was.
fn spacing_number(amount: f64) -> Object {
    if (amount - amount.round()).abs() < 1e-6 {
        Object::Integer(amount.round() as i64)
    } else {
        Object::Real(amount as f32)
    }
}
