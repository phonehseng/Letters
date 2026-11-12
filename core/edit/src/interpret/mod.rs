//! running a page's content streams to find every glyph and image.
//!
//! this follows the drawing state the way a pdf reader does: the current
//! transformation (cm, q, Q), the text state (Tf, Tc, Tw, Tz, TL, Ts,
//! Tr), the text position (BT, Td, TD, Tm, T*), the fill colour, and
//! form XObjects drawn with Do (whose content is run in turn). nothing is
//! drawn; each glyph and image is recorded with its exact position and
//! with the operator that drew it, so an edit can find it again.

use std::collections::HashMap;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::colors::Color;
use crate::error::EditError;
use crate::fonts::pdf_font::PdfFont;
use crate::geometry::{Matrix, Point};
use crate::lexer::{self, Operation};
use crate::page_space::{inherited, PageSpace};
use crate::pdf_values::{dict_dict, dict_name, dict_numbers, resolve, stream_bytes};

/// form XObjects inside form XObjects deeper than this are not followed;
/// real files nest a few levels, and a loop must not hang the app.
const MAX_FORM_DEPTH: usize = 12;

mod paths;
mod records;
mod state;
mod text;

pub(crate) use paths::segment_points;
pub(crate) use records::*;
use state::set_fill;
use text::move_line;

#[derive(Clone)]
struct GraphicsState {
    ctm: Matrix,
    text: TextState,
    stroke: FillColor,
    line: LineStyle,
    fill_alpha: f32,
    stroke_alpha: f32,
    clip_bounds: Option<[f32; 4]>,
    /// where the latest clip path in this q block started, so a shading
    /// that fills it can be moved together with it
    clip_start: Option<(usize, usize, usize)>,
    block: usize,
}

/// a path being built, before its painting operator.
#[derive(Default)]
struct PendingPath {
    first_op: Option<usize>,
    segments: Vec<Segment>,
    clip: bool,
    even_odd_clip: bool,
}

struct Runner<'a> {
    doc: &'a Document,
    page: &'a Dictionary,
    result: Interpretation,
    font_index: HashMap<ObjectId, usize>,
    form_stack: Vec<ObjectId>,
    pending: PendingPath,
    /// the open marked-content sequences, innermost last
    open_marked: Vec<usize>,
    blocks_made: usize,
}

/// runs every content stream of a page.
pub(crate) fn interpret(doc: &Document, page_id: ObjectId) -> Result<Interpretation, EditError> {
    let page = doc
        .get_dictionary(page_id)
        .map_err(|_| EditError::Damaged("the page is missing".into()))?;
    let space = PageSpace::of_page(doc, page_id)?;
    let mut runner = Runner {
        doc,
        page,
        result: Interpretation {
            page_id,
            space,
            sources: Vec::new(),
            fonts: Vec::new(),
            shows: Vec::new(),
            glyphs: Vec::new(),
            images: Vec::new(),
            paths: Vec::new(),
            marked: Vec::new(),
        },
        font_index: HashMap::new(),
        form_stack: Vec::new(),
        pending: PendingPath::default(),
        open_marked: Vec::new(),
        blocks_made: 0,
    };
    let mut state = GraphicsState {
        ctm: Matrix::IDENTITY,
        text: TextState {
            text_matrix: Matrix::IDENTITY,
            line_matrix: Matrix::IDENTITY,
            font_name: Vec::new(),
            font: None,
            size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            h_scale: 1.0,
            leading: 0.0,
            rise: 0.0,
            render_mode: 0,
            fill: FillColor::black(),
        },
        stroke: FillColor::black(),
        line: LineStyle {
            width: 1.0,
            dash: Vec::new(),
            dash_phase: 0.0,
            cap: 0,
            join: 0,
            miter: 10.0,
        },
        fill_alpha: 1.0,
        stroke_alpha: 1.0,
        clip_bounds: None,
        clip_start: None,
        block: 0,
    };
    // the streams of /Contents are one drawing split in parts: the state
    // carries on from one into the next
    for (index, object) in page_content_streams(doc, page).into_iter().enumerate() {
        let content = doc
            .get_object(object)
            .ok()
            .and_then(|stream| stream_bytes(doc, stream))
            .unwrap_or_default();
        let source = runner.add_source(
            Origin::Page { index },
            object,
            content,
            Resources::Page,
            state.ctm,
        );
        state = runner.run(source, state);
    }
    Ok(runner.result)
}

/// the content stream objects of a page, in order.
pub(crate) fn page_content_streams(doc: &Document, page: &Dictionary) -> Vec<ObjectId> {
    match page.get(b"Contents") {
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(parts)) => parts
                .iter()
                .filter_map(|part| part.as_reference().ok())
                .collect(),
            _ => vec![*id],
        },
        Ok(Object::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.as_reference().ok())
            .collect(),
        _ => Vec::new(),
    }
}

/// a small fingerprint of some bytes (FNV-1a), to notice changed content.
pub(crate) fn fingerprint(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
    })
}

fn numbers_of(operands: &[Object]) -> Vec<f32> {
    operands
        .iter()
        .filter_map(|operand| operand.as_float().ok())
        .collect()
}

impl<'a> Runner<'a> {
    fn add_source(
        &mut self,
        origin: Origin,
        object: ObjectId,
        content: Vec<u8>,
        resources: Resources,
        base_ctm: Matrix,
    ) -> usize {
        let operations = lexer::parse(&content);
        self.result.sources.push(Source {
            origin,
            object,
            fingerprint: fingerprint(&content),
            content,
            operations,
            resources,
            base_ctm,
        });
        self.result.sources.len() - 1
    }

    /// the resource dictionary a stream's names are looked up in.
    fn resources(&self, source: usize) -> Option<&'a Dictionary> {
        match &self.result.sources[source].resources {
            Resources::Page => inherited(self.doc, self.page, b"Resources")
                .and_then(|r| resolve(self.doc, r).as_dict().ok()),
            Resources::Object(id) => self.doc.get_dictionary(*id).ok(),
            Resources::InForm(form) => {
                let form = self.doc.get_object(*form).ok()?.as_stream().ok()?;
                dict_dict(self.doc, &form.dict, b"Resources")
            }
        }
    }

    fn resource(&self, source: usize, category: &[u8], name: &[u8]) -> Option<&'a Object> {
        let resources = self.resources(source)?;
        let category = dict_dict(self.doc, resources, category)?;
        category.get(name).ok()
    }

    fn font(&mut self, source: usize, name: &[u8]) -> Option<usize> {
        let object = self.resource(source, b"Font", name)?;
        let id = object.as_reference().ok();
        if let Some(index) = id.and_then(|id| self.font_index.get(&id)) {
            return Some(*index);
        }
        let dictionary = resolve(self.doc, object).as_dict().ok()?;
        self.result
            .fonts
            .push(PdfFont::load(self.doc, id, dictionary));
        let index = self.result.fonts.len() - 1;
        if let Some(id) = id {
            self.font_index.insert(id, index);
        }
        Some(index)
    }

    /// runs one stream's operators, starting from `state`; returns the
    /// state it leaves, which the page's next stream carries on from.
    fn run(&mut self, source: usize, mut state: GraphicsState) -> GraphicsState {
        let mut saved: Vec<GraphicsState> = Vec::new();
        let operations = self.result.sources[source].operations.clone();
        for (op, operation) in operations.iter().enumerate() {
            match operation.operator.as_str() {
                "q" => {
                    saved.push(state.clone());
                    self.blocks_made += 1;
                    state.block = self.blocks_made;
                }
                "Q" => {
                    if let Some(previous) = saved.pop() {
                        state = previous;
                    }
                }
                _ => self.dispatch(source, op, operation, &mut state),
            }
        }
        state
    }

    /// hands an operator to the family of operators it belongs to.
    fn dispatch(
        &mut self,
        source: usize,
        op: usize,
        operation: &Operation,
        state: &mut GraphicsState,
    ) {
        let numbers = numbers_of(&operation.operands);
        if self.path_operation(source, op, operation, &numbers, state) {
            return;
        }
        if self.state_operation(source, op, operation, &numbers, state) {
            return;
        }
        if self.text_operation(source, op, operation, &numbers, state) {
            return;
        }
        if self.color_operation(source, operation, &numbers, state) {
            return;
        }
        self.object_operation(source, op, operation, state);
    }

    /// marked content, the matrix, line style and ExtGState.
    fn state_operation(
        &mut self,
        source: usize,
        op: usize,
        operation: &Operation,
        numbers: &[f32],
        state: &mut GraphicsState,
    ) -> bool {
        let operands = &operation.operands;
        let first = numbers.first().copied();
        match operation.operator.as_str() {
            "BMC" | "BDC" => self.begin_marked(source, op, operands),
            "EMC" => {
                self.open_marked.pop();
            }
            "cm" => {
                if let Some(matrix) = Matrix::from_slice(numbers) {
                    state.ctm = matrix.then(&state.ctm);
                }
            }
            "w" => state.line.width = first.unwrap_or(1.0),
            "J" => state.line.cap = first.unwrap_or(0.0) as i64,
            "j" => state.line.join = first.unwrap_or(0.0) as i64,
            "M" => state.line.miter = first.unwrap_or(10.0),
            "d" => {
                if let (Some(Object::Array(dash)), Some(phase)) =
                    (operands.first(), operands.get(1))
                {
                    state.line.dash = numbers_of(dash);
                    state.line.dash_phase = phase.as_float().unwrap_or(0.0);
                }
            }
            "gs" => {
                if let Some(Object::Name(name)) = operands.first() {
                    self.graphics_state(source, name, state);
                }
            }
            _ => return false,
        }
        true
    }

    /// the text state, the text position and the show operators.
    fn text_operation(
        &mut self,
        source: usize,
        op: usize,
        operation: &Operation,
        numbers: &[f32],
        state: &mut GraphicsState,
    ) -> bool {
        let first = numbers.first().copied();
        let text = &mut state.text;
        match operation.operator.as_str() {
            "BT" => {
                text.text_matrix = Matrix::IDENTITY;
                text.line_matrix = Matrix::IDENTITY;
            }
            "Tf" => {
                if let (Some(Object::Name(name)), Some(size)) =
                    (operation.operands.first(), numbers.last())
                {
                    text.font_name = name.clone();
                    text.size = *size;
                    text.font = self.font(source, name);
                }
            }
            "Tc" => text.char_spacing = first.unwrap_or(0.0),
            "Tw" => text.word_spacing = first.unwrap_or(0.0),
            "Tz" => text.h_scale = first.unwrap_or(100.0) / 100.0,
            "TL" => text.leading = first.unwrap_or(0.0),
            "Ts" => text.rise = first.unwrap_or(0.0),
            "Tr" => text.render_mode = first.unwrap_or(0.0) as i64,
            "Td" | "TD" => {
                if let [x, y] = numbers[..] {
                    // TD also sets the leading, as if "-y TL" came first
                    if operation.operator == "TD" {
                        text.leading = -y;
                    }
                    move_line(text, x, y);
                }
            }
            "Tm" => {
                if let Some(matrix) = Matrix::from_slice(numbers) {
                    text.text_matrix = matrix;
                    text.line_matrix = matrix;
                }
            }
            "T*" => {
                let leading = text.leading;
                move_line(text, 0.0, -leading);
            }
            "Tj" | "TJ" | "'" | "\"" => self.show(source, op, operation, state),
            _ => return false,
        }
        true
    }

    /// fill and stroke colours and their colour spaces.
    fn color_operation(
        &mut self,
        source: usize,
        operation: &Operation,
        numbers: &[f32],
        state: &mut GraphicsState,
    ) -> bool {
        match operation.operator.as_str() {
            "g" | "rg" | "k" => set_fill(&mut state.text.fill, source, operation, numbers, None),
            "G" | "RG" | "K" => set_fill(&mut state.stroke, source, operation, numbers, None),
            "cs" => state.text.fill = self.color_space(source, operation),
            "CS" => state.stroke = self.color_space(source, operation),
            "sc" | "scn" => {
                let space = state.text.fill.space_operator.clone();
                set_fill(&mut state.text.fill, source, operation, numbers, space);
            }
            "SC" | "SCN" => {
                let space = state.stroke.space_operator.clone();
                set_fill(&mut state.stroke, source, operation, numbers, space);
            }
            _ => return false,
        }
        true
    }

    /// images and forms: Do, and inline images.
    fn object_operation(
        &mut self,
        source: usize,
        op: usize,
        operation: &Operation,
        state: &GraphicsState,
    ) {
        match operation.operator.as_str() {
            "Do" => {
                if let Some(Object::Name(name)) = operation.operands.first() {
                    self.draw_object(source, op, name, state);
                }
            }
            "BI" => {
                let inline = operation
                    .operands
                    .first()
                    .and_then(|dict| dict.as_dict().ok())
                    .cloned();
                self.result.images.push(ImageRecord {
                    source,
                    op,
                    object: None,
                    ctm: state.ctm,
                    inline,
                    clip_bounds: state.clip_bounds,
                    fill_alpha: state.fill_alpha,
                });
            }
            _ => {}
        }
    }

    fn draw_object(&mut self, source: usize, op: usize, name: &[u8], state: &GraphicsState) {
        let Some(object) = self.resource(source, b"XObject", name) else {
            return;
        };
        let Ok(id) = object.as_reference() else {
            return;
        };
        let Ok(stream) = self.doc.get_object(id).and_then(Object::as_stream) else {
            return;
        };
        match dict_name(self.doc, &stream.dict, b"Subtype") {
            Some(b"Image") => self.result.images.push(ImageRecord {
                source,
                op,
                object: Some(id),
                ctm: state.ctm,
                inline: None,
                clip_bounds: state.clip_bounds,
                fill_alpha: state.fill_alpha,
            }),
            Some(b"Form")
                if self.form_stack.len() < MAX_FORM_DEPTH && !self.form_stack.contains(&id) =>
            {
                let matrix = dict_numbers(self.doc, &stream.dict, b"Matrix")
                    .and_then(|values| Matrix::from_slice(&values))
                    .unwrap_or(Matrix::IDENTITY);
                let resources = match stream.dict.get(b"Resources") {
                    Ok(Object::Reference(resources)) => Resources::Object(*resources),
                    Ok(Object::Dictionary(_)) => Resources::InForm(id),
                    // old files let a form use the resources of what draws it
                    _ => self.result.sources[source].resources.clone(),
                };
                let content =
                    stream_bytes(self.doc, &Object::Stream(stream.clone())).unwrap_or_default();
                let mut inner = state.clone();
                inner.ctm = matrix.then(&state.ctm);
                let origin = Origin::Form {
                    parent: source,
                    op,
                    name: name.to_vec(),
                };
                let form_source = self.add_source(origin, id, content, resources, inner.ctm);
                // a form starts with its own marked content, and a path
                // left half built in the page can't continue inside it
                let outer_marked = std::mem::take(&mut self.open_marked);
                let outer_path = std::mem::take(&mut self.pending);
                self.form_stack.push(id);
                self.run(form_source, inner);
                self.form_stack.pop();
                self.open_marked = outer_marked;
                self.pending = outer_path;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_values::name;
    use lopdf::Stream;

    fn page_with(content: &str, rotate: i64) -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.7");
        let mut font = Dictionary::new();
        font.set("Type", name("Font"));
        font.set("Subtype", name("Type1"));
        font.set("BaseFont", name("Helvetica"));
        font.set("Encoding", name("WinAnsiEncoding"));
        let font_id = doc.add_object(font);
        let content_id =
            doc.add_object(Stream::new(Dictionary::new(), content.as_bytes().to_vec()));
        let mut fonts = Dictionary::new();
        fonts.set("F1", Object::Reference(font_id));
        let mut resources = Dictionary::new();
        resources.set("Font", Object::Dictionary(fonts));
        let mut page = Dictionary::new();
        page.set("Type", name("Page"));
        page.set(
            "MediaBox",
            crate::pdf_values::number_array(&[0.0, 0.0, 612.0, 792.0]),
        );
        page.set("Rotate", Object::Integer(rotate));
        page.set("Contents", Object::Reference(content_id));
        page.set("Resources", Object::Dictionary(resources));
        let page_id = doc.add_object(page);
        (doc, page_id)
    }

    #[test]
    fn glyphs_are_placed_with_the_text_and_graphics_state() {
        let (doc, page) = page_with(
            "q 2 0 0 2 0 0 cm BT /F1 10 Tf 1 0 0 1 36 300 Tm 5 Tc (AV) Tj [(A) -1000 (V)] TJ ET Q",
            0,
        );
        let run = interpret(&doc, page).unwrap();
        let texts: String = run.glyphs.iter().map(|glyph| glyph.text.as_str()).collect();
        assert_eq!(texts, "AVAV");
        // A is 667 wide in helvetica: 6.67 + 5 spacing, all doubled by cm
        let first = &run.glyphs[0];
        assert!((first.origin.x - 72.0).abs() < 1e-3 && (first.origin.y - 600.0).abs() < 1e-3);
        assert!((run.glyphs[1].origin.x - (72.0 + 2.0 * 11.67)).abs() < 1e-3);
        assert!((first.size - 20.0).abs() < 1e-3);
        // the TJ adjustment of -1000 moves V one font size to the right
        let gap = run.glyphs[3].origin.x - run.glyphs[2].origin.x;
        assert!((gap - 2.0 * (11.67 + 10.0)).abs() < 1e-3, "{gap}");
        assert_eq!(run.shows.len(), 2);
    }

    #[test]
    fn line_moves_and_leading_work() {
        let (doc, page) = page_with(
            "BT /F1 12 Tf 14 TL 50 700 Td (a) Tj T* (b) Tj 0 -20 TD (c) ' ET",
            0,
        );
        let run = interpret(&doc, page).unwrap();
        let heights: Vec<f32> = run.glyphs.iter().map(|glyph| glyph.origin.y).collect();
        // TD also sets the leading to 20, which ' then uses
        assert_eq!(heights, [700.0, 686.0, 646.0]);
    }

    #[test]
    fn fill_colours_are_followed() {
        let (doc, page) = page_with("1 0 0 rg BT /F1 12 Tf (a) Tj 0 g (b) Tj ET", 0);
        let run = interpret(&doc, page).unwrap();
        assert_eq!(run.glyphs[0].color, Color::new(255, 0, 0));
        assert_eq!(run.glyphs[1].color, Color::BLACK);
    }
}
