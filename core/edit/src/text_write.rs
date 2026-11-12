//! writing re-set text into a content stream.
//!
//! an edit never moves the operators around it. instead:
//! - every glyph being replaced is taken out of its show operator and
//!   replaced by an equal move (a TJ number), so the operator still ends
//!   exactly where it ended before and nothing after it shifts;
//! - the new glyphs are written right after the operator where the edit
//!   starts, each group placed with an absolute text matrix (Tm);
//! - then the text state is set back exactly as the original operator
//!   left it: font, colour and rise, and both text matrices. setting the
//!   line matrix with Tm and then moving with a TJ of numbers only puts
//!   back the text matrix too, which a plain Tm can't.
//!
//! so whatever comes after the edit in the stream finds the very state
//! it found before, and the rest of the page is untouched.

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId, StringFormat};

use crate::colors::Color;
use crate::error::EditError;
use crate::geometry::{Matrix, Point};
use crate::interpret::{Interpretation, ShowItem, ShowOp, TextState};
use crate::pdf_values::format_number;
use crate::rewrite::Plan;
use crate::writer;

/// how a placed glyph names its font.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FontRef {
    /// a font object, given a resource name where the text is written
    Object(ObjectId),
    /// a font only known by its name in one stream's resources
    Named { source: usize, name: Vec<u8> },
}

/// one glyph of re-set text, at an exact place.
#[derive(Clone, Debug)]
pub(crate) struct Placed {
    /// position along the line, in the anchor's text space
    pub x: f32,
    /// which line, for text re-set over several lines
    pub line: usize,
    pub font: FontRef,
    /// the font size (Tf), not the size on screen
    pub size: f32,
    pub rise: f32,
    pub color: Color,
    /// none for a space written as a plain move
    pub code: Option<Vec<u8>>,
    /// thousandths of the font size
    pub width: f32,
    pub is_word_space: bool,
}

/// where re-set text is written and measured from.
#[derive(Clone, Debug)]
pub(crate) struct Anchor {
    /// the show operator the new text is written after
    pub show: usize,
    /// the text matrix at x = 0 of the first line
    pub text_matrix: Matrix,
    /// the distance from one line to the next, in text space
    pub line_step: f32,
}

/// how far a glyph moves the pen when written in this state.
pub(crate) fn advance(state: &TextState, width: f32, size: f32, is_word_space: bool) -> f32 {
    let word = if is_word_space {
        state.word_spacing
    } else {
        0.0
    };
    (width / 1000.0 * size + state.char_spacing + word) * state.h_scale
}

/// turns a move in text space into a TJ number for this font size.
fn adjustment(state: &TextState, size: f32, distance: f32) -> Option<f32> {
    let scale = size * state.h_scale;
    (scale.abs() > 1e-9).then(|| -distance * 1000.0 / scale)
}

fn matrix_operator(matrix: Matrix) -> String {
    let numbers: Vec<String> = matrix
        .to_array()
        .iter()
        .map(|value| format_number(*value))
        .collect();
    format!("{} Tm", numbers.join(" "))
}

fn font_operator(name: &[u8], size: f32) -> String {
    format!("{} {} Tf", writer::name(name), format_number(size))
}

/// the operator rewritten without the removed glyphs, each replaced by a
/// move of the same length.
pub(crate) fn blanked_operator(
    run: &Interpretation,
    show: &ShowOp,
    removed: &HashSet<usize>,
    operands: &[Object],
    operator: &str,
) -> Vec<u8> {
    let state = &show.before;
    let mut items: Vec<Object> = Vec::new();
    let mut codes: Vec<u8> = Vec::new();
    let mut pending_move = 0.0f32;
    let flush_codes = |codes: &mut Vec<u8>, items: &mut Vec<Object>| {
        if !codes.is_empty() {
            items.push(Object::String(
                std::mem::take(codes),
                StringFormat::Hexadecimal,
            ));
        }
    };
    for item in &show.items {
        match item {
            ShowItem::Glyph(glyph) if removed.contains(glyph) => {
                flush_codes(&mut codes, &mut items);
                pending_move += run.glyphs[*glyph].advance;
            }
            ShowItem::Glyph(glyph) => {
                if pending_move != 0.0 {
                    items.extend(adjustment(state, state.size, pending_move).map(Object::Real));
                    pending_move = 0.0;
                }
                codes.extend_from_slice(&run.glyphs[*glyph].code);
            }
            ShowItem::Adjust(number) => {
                flush_codes(&mut codes, &mut items);
                if pending_move != 0.0 {
                    items.extend(adjustment(state, state.size, pending_move).map(Object::Real));
                    pending_move = 0.0;
                }
                items.push(Object::Real(*number));
            }
        }
    }
    flush_codes(&mut codes, &mut items);
    if pending_move != 0.0 {
        items.extend(adjustment(state, state.size, pending_move).map(Object::Real));
    }
    let shown = writer::operation("TJ", &[Object::Array(items)]);
    // ' and " also move to the next line (and " sets spacing): keep that
    let prefix = match operator {
        "'" => b"T* ".to_vec(),
        "\"" => {
            let mut prefix = Vec::new();
            if let [word, char, ..] = operands {
                prefix.extend(writer::operation("Tw", std::slice::from_ref(word)));
                prefix.push(b' ');
                prefix.extend(writer::operation("Tc", std::slice::from_ref(char)));
                prefix.push(b' ');
            }
            prefix.extend_from_slice(b"T* ");
            prefix
        }
        _ => Vec::new(),
    };
    [prefix, shown].concat()
}

/// takes glyphs out of every operator that drew them.
pub(crate) fn blank(run: &Interpretation, plan: &mut Plan, removed: &HashSet<usize>) {
    let shows: HashSet<usize> = removed
        .iter()
        .map(|glyph| run.glyphs[*glyph].show)
        .collect();
    for show_index in shows {
        let show = &run.shows[show_index];
        let operation = &run.sources[show.source].operations[show.op];
        let bytes = blanked_operator(run, show, removed, &operation.operands, &operation.operator);
        plan.op(show.source, show.op).replace = Some(bytes);
    }
}

/// the bytes that set the fill colour back to what the anchor had.
fn color_restore(fill: &crate::interpret::FillColor, source: usize) -> String {
    match &fill.color_operator {
        Some(operator) => {
            let same_stream = fill.source == Some(source);
            match (&fill.space_operator, same_stream) {
                // a named colour space only exists in its own stream's resources
                (Some(_), false) => fill.color.fill_operator(),
                (Some(space), true) => format!(
                    "{} {}",
                    String::from_utf8_lossy(space),
                    String::from_utf8_lossy(operator)
                ),
                (None, _) => String::from_utf8_lossy(operator).into_owned(),
            }
        }
        None => "0 g".into(),
    }
}

/// whether two placed glyphs can share one TJ.
fn same_group(a: &Placed, b: &Placed) -> bool {
    a.font == b.font
        && a.size == b.size
        && a.color == b.color
        && a.line == b.line
        && a.rise == b.rise
}

/// one TJ for the glyphs from `start` on that share a font, size,
/// colour and line; small differences in position become TJ numbers.
/// returns the operator and where the next group starts.
pub(crate) fn show_group(state: &TextState, glyphs: &[Placed], start: usize) -> (String, usize) {
    let first = &glyphs[start];
    let mut items: Vec<Object> = Vec::new();
    let mut codes: Vec<u8> = Vec::new();
    let mut pen = first.x;
    let mut index = start;
    while index < glyphs.len() && same_group(first, &glyphs[index]) {
        let glyph = &glyphs[index];
        let distance = glyph.x - pen;
        if distance.abs() > 1e-4 {
            if !codes.is_empty() {
                items.push(Object::String(
                    std::mem::take(&mut codes),
                    StringFormat::Hexadecimal,
                ));
            }
            items.extend(adjustment(state, glyph.size, distance).map(Object::Real));
        }
        match &glyph.code {
            Some(code) => {
                codes.extend_from_slice(code);
                pen = glyph.x + advance(state, glyph.width, glyph.size, glyph.is_word_space);
            }
            // a space the font can't draw: the next glyph's position moves the pen
            None => pen = glyph.x,
        }
        index += 1;
    }
    if !codes.is_empty() {
        items.push(Object::String(codes, StringFormat::Hexadecimal));
    }
    let operator = writer::operation("TJ", &[Object::Array(items)]);
    (String::from_utf8_lossy(&operator).into_owned(), index)
}

/// writes the placed glyphs after the anchor operator, then sets the text
/// state back.
pub(crate) fn write(
    doc: &Document,
    run: &Interpretation,
    plan: &mut Plan,
    anchor: &Anchor,
    glyphs: &[Placed],
) -> Result<(), EditError> {
    let show = &run.shows[anchor.show];
    let state = &show.before;
    let source = show.source;
    let mut out: Vec<String> = Vec::new();
    let mut font: (Vec<u8>, f32) = (state.font_name.clone(), state.size);
    let mut color = state.fill.color;
    let mut rise = state.rise;
    let (mut font_changed, mut color_changed, mut rise_changed) = (false, false, false);

    let mut index = 0;
    while index < glyphs.len() {
        let first = &glyphs[index];
        let name = match &first.font {
            FontRef::Object(object) => plan.resource_name(doc, run, Some(source), "Font", *object),
            FontRef::Named {
                source: named_in,
                name,
            } if *named_in == source => name.clone(),
            FontRef::Named { .. } => {
                return Err(EditError::Damaged(
                    "a font can’t be found where the text is written".into(),
                ))
            }
        };
        if (name.clone(), first.size) != font {
            out.push(font_operator(&name, first.size));
            font = (name.clone(), first.size);
            font_changed = true;
        }
        if first.color != color {
            out.push(first.color.fill_operator());
            color = first.color;
            color_changed = true;
        }
        if (first.rise - rise).abs() > 1e-6 {
            out.push(format!("{} Ts", format_number(first.rise)));
            rise = first.rise;
            rise_changed = true;
        }
        let start = Matrix::translate(first.x, -(first.line as f32) * anchor.line_step)
            .then(&anchor.text_matrix);
        // the corners new text can reach, for a form whose box must hold it
        let to_page = start.then(&show.ctm);
        let reach_end = glyphs[index..]
            .iter()
            .filter(|glyph| glyph.line == first.line)
            .map(|glyph| glyph.x - first.x + glyph.size)
            .fold(0.0f32, f32::max);
        let reach = [
            (0.0, -first.size),
            (reach_end, -first.size),
            (0.0, first.size * 1.2),
            (reach_end, first.size * 1.2),
        ]
        .map(|(x, y)| to_page.apply(Point::new(x, y)));
        plan.reach(source, &reach);
        out.push(matrix_operator(start));
        let (shown, next) = show_group(state, glyphs, index);
        out.push(shown);
        index = next;
    }

    if font_changed {
        out.push(font_operator(&state.font_name, state.size));
    }
    if color_changed {
        out.push(color_restore(&state.fill, source));
    }
    if rise_changed {
        out.push(format!("{} Ts", format_number(state.rise)));
    }
    out.push(matrix_operator(show.line_matrix_after));
    let offset = show
        .text_matrix_after
        .then(&show.line_matrix_after.invert().unwrap_or(Matrix::IDENTITY));
    if offset.e.abs() > 1e-6 {
        if let Some(number) = adjustment(state, state.size, offset.e) {
            out.push(format!("[{}] TJ", format_number(number)));
        }
    }
    let edit = plan.op(source, show.op);
    let addition = out.join("\n");
    if edit.after.is_empty() {
        edit.after = addition.into_bytes();
    } else {
        edit.after.push(b'\n');
        edit.after.extend_from_slice(addition.as_bytes());
    }
    Ok(())
}
