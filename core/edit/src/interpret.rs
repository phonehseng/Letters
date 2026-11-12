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

/// where a content stream comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Origin {
    /// the page's own /Contents, by position in the list
    Page { index: usize },
    /// a form XObject, drawn by operator `op` (named `name`) of `parent`
    Form {
        parent: usize,
        op: usize,
        name: Vec<u8>,
    },
}

/// one content stream that was run.
#[derive(Clone, Debug)]
pub(crate) struct Source {
    pub origin: Origin,
    pub object: ObjectId,
    pub content: Vec<u8>,
    pub operations: Vec<Operation>,
    /// the resources this stream's names refer to: a dictionary object, or
    /// none for a page whose resources are written in (or inherited by) it
    pub resources: Resources,
    /// a short fingerprint of the content, to notice stale ids
    pub fingerprint: u32,
    /// the matrix in effect when the stream starts (page space)
    pub base_ctm: Matrix,
}

/// where a stream's resource dictionary lives.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Resources {
    /// the page's own /Resources (possibly inherited)
    Page,
    /// a /Resources dictionary object
    Object(ObjectId),
    /// a dictionary written inside a form's stream dictionary
    InForm(ObjectId),
}

/// the text state when a show operator starts, which an edit needs to
/// write text that continues seamlessly.
#[derive(Clone, Debug)]
pub(crate) struct TextState {
    pub text_matrix: Matrix,
    pub line_matrix: Matrix,
    pub font_name: Vec<u8>,
    pub font: Option<usize>,
    pub size: f32,
    pub char_spacing: f32,
    pub word_spacing: f32,
    /// horizontal scaling as a fraction (Tz 100 is 1.0)
    pub h_scale: f32,
    pub leading: f32,
    pub rise: f32,
    pub render_mode: i64,
    pub fill: FillColor,
}

/// the fill colour and the operators that set it, to set it back exactly.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FillColor {
    pub color: Color,
    /// the colour-space operator ("/Cs1 cs"), when one was used
    pub space_operator: Option<Vec<u8>>,
    pub color_operator: Option<Vec<u8>>,
    /// the stream the operators came from, since named colour spaces only
    /// mean something there
    pub source: Option<usize>,
}

/// one piece of a show operator: a glyph or a TJ position adjustment.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ShowItem {
    Glyph(usize),
    Adjust(f32),
}

/// a text show operator (Tj, TJ, ' or ").
#[derive(Clone, Debug)]
pub(crate) struct ShowOp {
    pub source: usize,
    pub op: usize,
    /// the state just before the glyphs ("'" and '"' already applied)
    pub before: TextState,
    /// text and line matrices after the operator
    pub text_matrix_after: Matrix,
    pub line_matrix_after: Matrix,
    pub ctm: Matrix,
    pub items: Vec<ShowItem>,
}

/// one drawn glyph.
#[derive(Clone, Debug)]
pub(crate) struct GlyphRecord {
    pub show: usize,
    pub text: String,
    pub code: Vec<u8>,
    /// the glyph's width in thousandths of the font size
    pub width: f32,
    pub font: Option<usize>,
    /// the text matrix just before the glyph
    pub text_matrix: Matrix,
    /// how far the glyph moves the text position, in text space
    pub advance: f32,
    pub is_space: bool,
    /// baseline start and end, and the glyph box, in page user space
    pub origin: Point,
    pub end: Point,
    pub corners: [Point; 4],
    pub size: f32,
    pub color: Color,
    pub invisible: bool,
}

/// one image drawn with Do or written inline (BI ... EI).
#[derive(Clone, Debug)]
pub(crate) struct ImageRecord {
    pub source: usize,
    pub op: usize,
    /// the image object, none for an inline image
    pub object: Option<ObjectId>,
    /// maps the unit square onto the page (page user space)
    pub ctm: Matrix,
    pub inline: Option<Dictionary>,
}

/// everything found on a page.
pub(crate) struct Interpretation {
    pub page_id: ObjectId,
    pub space: PageSpace,
    pub sources: Vec<Source>,
    pub fonts: Vec<PdfFont>,
    pub shows: Vec<ShowOp>,
    pub glyphs: Vec<GlyphRecord>,
    pub images: Vec<ImageRecord>,
}

#[derive(Clone)]
struct GraphicsState {
    ctm: Matrix,
    text: TextState,
}

struct Runner<'a> {
    doc: &'a Document,
    page: &'a Dictionary,
    result: Interpretation,
    font_index: HashMap<ObjectId, usize>,
    form_stack: Vec<ObjectId>,
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
        },
        font_index: HashMap::new(),
        form_stack: Vec::new(),
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
            fill: FillColor {
                color: Color::BLACK,
                space_operator: None,
                color_operator: None,
                source: None,
            },
        },
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

    fn run(&mut self, source: usize, mut state: GraphicsState) -> GraphicsState {
        let mut stack: Vec<GraphicsState> = Vec::new();
        let operations = self.result.sources[source].operations.clone();
        for (op_index, operation) in operations.iter().enumerate() {
            let operands = &operation.operands;
            let numbers = numbers_of(operands);
            match operation.operator.as_str() {
                "q" => stack.push(state.clone()),
                "Q" => {
                    if let Some(saved) = stack.pop() {
                        state = saved;
                    }
                }
                "cm" => {
                    if let Some(matrix) = Matrix::from_slice(&numbers) {
                        state.ctm = matrix.then(&state.ctm);
                    }
                }
                "BT" => {
                    state.text.text_matrix = Matrix::IDENTITY;
                    state.text.line_matrix = Matrix::IDENTITY;
                }
                "Tf" => {
                    if let (Some(Object::Name(name)), Some(size)) =
                        (operands.first(), numbers.last())
                    {
                        state.text.font_name = name.clone();
                        state.text.size = *size;
                        state.text.font = self.font(source, name);
                    }
                }
                "Tc" => state.text.char_spacing = numbers.first().copied().unwrap_or(0.0),
                "Tw" => state.text.word_spacing = numbers.first().copied().unwrap_or(0.0),
                "Tz" => state.text.h_scale = numbers.first().copied().unwrap_or(100.0) / 100.0,
                "TL" => state.text.leading = numbers.first().copied().unwrap_or(0.0),
                "Ts" => state.text.rise = numbers.first().copied().unwrap_or(0.0),
                "Tr" => state.text.render_mode = numbers.first().copied().unwrap_or(0.0) as i64,
                "Td" | "TD" => {
                    if let [x, y] = numbers[..] {
                        if operation.operator == "TD" {
                            state.text.leading = -y;
                        }
                        move_line(&mut state.text, x, y);
                    }
                }
                "Tm" => {
                    if let Some(matrix) = Matrix::from_slice(&numbers) {
                        state.text.text_matrix = matrix;
                        state.text.line_matrix = matrix;
                    }
                }
                "T*" => {
                    let leading = state.text.leading;
                    move_line(&mut state.text, 0.0, -leading);
                }
                "Tj" | "TJ" | "'" | "\"" => {
                    self.show(source, op_index, operation, &mut state);
                }
                "g" | "rg" | "k" => {
                    set_fill(&mut state.text.fill, source, operation, &numbers, None)
                }
                "cs" => {
                    state.text.fill.space_operator =
                        Some(self.result.sources[source].content[operation.span.clone()].to_vec());
                    state.text.fill.color_operator = None;
                    state.text.fill.source = Some(source);
                    state.text.fill.color = Color::BLACK;
                }
                "sc" | "scn" => {
                    let space = state.text.fill.space_operator.clone();
                    set_fill(&mut state.text.fill, source, operation, &numbers, space);
                }
                "Do" => {
                    if let Some(Object::Name(name)) = operands.first() {
                        self.draw_object(source, op_index, name, &state);
                    }
                }
                "BI" => {
                    self.result.images.push(ImageRecord {
                        source,
                        op: op_index,
                        object: None,
                        ctm: state.ctm,
                        inline: operands
                            .first()
                            .and_then(|dict| dict.as_dict().ok())
                            .cloned(),
                    });
                }
                _ => {}
            }
        }
        state
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
                self.form_stack.push(id);
                self.run(form_source, inner);
                self.form_stack.pop();
            }
            _ => {}
        }
    }

    /// records the glyphs of one show operator and moves the text position.
    fn show(&mut self, source: usize, op: usize, operation: &Operation, state: &mut GraphicsState) {
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
        });
    }

    fn glyph(
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

fn move_line(text: &mut TextState, x: f32, y: f32) {
    text.line_matrix = Matrix::translate(x, y).then(&text.line_matrix);
    text.text_matrix = text.line_matrix;
}

fn set_fill(
    fill: &mut FillColor,
    source: usize,
    operation: &Operation,
    numbers: &[f32],
    space: Option<Vec<u8>>,
) {
    fill.color = match numbers {
        [grey] => Color::from_fractions(*grey, *grey, *grey),
        [r, g, b] => Color::from_fractions(*r, *g, *b),
        [c, m, y, k] => Color::from_fractions(
            1.0 - (c + k).min(1.0),
            1.0 - (m + k).min(1.0),
            1.0 - (y + k).min(1.0),
        ),
        // a pattern or a colour space Letters doesn't read
        _ => Color::BLACK,
    };
    if matches!(operation.operator.as_str(), "g" | "rg" | "k") {
        fill.space_operator = None;
    } else {
        fill.space_operator = space;
    }
    fill.color_operator = Some(operation_bytes_placeholder(operation));
    fill.source = Some(source);
}

/// the operator as text, rebuilt from its operands. (the original bytes
/// are equivalent; rebuilding keeps this independent of the source.)
fn operation_bytes_placeholder(operation: &Operation) -> Vec<u8> {
    crate::writer::operation(&operation.operator, &operation.operands)
}

/// the text state at the start of an operation in a stream, as plain data
/// for tests.
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
