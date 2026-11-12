//! reads a page's drawing instructions and reports what they put where.
//!
//! a page's content is a list of operators: set a colour, move the pen,
//! show some text, draw a picture. this reader follows them the way a pdf
//! viewer does (keeping track of the current transformation, the text
//! position and the fill colour) but instead of drawing it records:
//! - each piece of text: its position on the visible page, size, font,
//!   colour and characters
//! - each picture and each drawn shape, with its position
//! - which marked-content sequence (`BDC` ... `EMC`) each of them sits in,
//!   because that is how tagged content is linked to the structure tree
//!
//! everything is tied back to the operator that drew it, by its index in
//! the page's top level operator list, so the fixes can wrap or change
//! exactly those operators later. content drawn inside a form xobject
//! (a reusable drawing) is tied to the `Do` operator that drew the form.

use std::collections::HashMap;
use std::rc::Rc;

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::colour::Rgb;
use crate::fonts::{FontInfo, UnicodeSource};
use crate::geometry::{Matrix, PageFrame, Rect};
use crate::objects::{get, get_dict, get_name, inherited, name_text, resolve};

/// pages with more operators than this are only read this far, so a
/// pathological file can't stall the checker.
const OPERATOR_LIMIT: usize = 2_000_000;

/// form xobjects can contain forms; deeper nesting than this is cut off.
const FORM_DEPTH_LIMIT: usize = 12;

/// which marked-content sequence a piece of content belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// not inside any sequence with an mcid, nor an artifact
    Untagged,
    /// inside a sequence with this marked-content id, so it is tagged
    Mcid(i64),
    /// marked as decoration that assistive technology should skip
    Artifact,
}

/// the colour space a colour was given in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColourSpace {
    Gray,
    Rgb,
    Cmyk,
    Pattern,
    /// a space this reader can't turn into rgb (spot colours, lab, indexed)
    Other,
}

/// the fill or stroke colour at some point, with how it was set, so a fix
/// can set it back after changing one piece of text.
#[derive(Clone, Debug, PartialEq)]
pub struct ColourState {
    pub space: ColourSpace,
    pub components: Vec<f32>,
    /// the name given to `cs`, when the space was chosen that way
    pub space_name: Option<Vec<u8>>,
    pub pattern_name: Option<Vec<u8>>,
}

impl Default for ColourState {
    fn default() -> Self {
        Self {
            space: ColourSpace::Gray,
            components: vec![0.0],
            space_name: None,
            pattern_name: None,
        }
    }
}

impl ColourState {
    fn device(space: ColourSpace, components: Vec<f32>) -> Self {
        Self {
            space,
            components,
            space_name: None,
            pattern_name: None,
        }
    }

    /// the colour as rgb, when it can be worked out.
    pub fn rgb(&self) -> Option<Rgb> {
        let c = |index: usize| self.components.get(index).copied().unwrap_or(0.0);
        match self.space {
            ColourSpace::Gray => Some(Rgb::from_unit(c(0), c(0), c(0))),
            ColourSpace::Rgb => Some(Rgb::from_unit(c(0), c(1), c(2))),
            // the simple conversion; real printing inks differ, but this is
            // what most viewers show on screen without colour management
            ColourSpace::Cmyk => {
                let k = 1.0 - c(3);
                Some(Rgb::from_unit(
                    (1.0 - c(0)) * k,
                    (1.0 - c(1)) * k,
                    (1.0 - c(2)) * k,
                ))
            }
            ColourSpace::Pattern | ColourSpace::Other => None,
        }
    }

    /// operators that set this colour as the fill colour again.
    pub fn fill_operations(&self) -> Vec<Operation> {
        let numbers: Vec<Object> = self.components.iter().map(|&v| Object::Real(v)).collect();
        if let Some(space) = &self.space_name {
            let mut operands = numbers;
            if let Some(pattern) = &self.pattern_name {
                operands.push(Object::Name(pattern.clone()));
            }
            return vec![
                Operation::new("cs", vec![Object::Name(space.clone())]),
                Operation::new("scn", operands),
            ];
        }
        let operator = match self.space {
            ColourSpace::Rgb => "rg",
            ColourSpace::Cmyk => "k",
            _ => "g",
        };
        vec![Operation::new(operator, numbers)]
    }
}

/// one shown string (one `Tj`, `TJ`, `'` or `"`).
#[derive(Clone, Debug)]
pub struct TextRun {
    /// index of the showing operator, or of the `Do` that drew the form it is in
    pub op_index: usize,
    pub in_form: bool,
    pub rect: Rect,
    /// the size it appears at, in points
    pub font_size: f32,
    pub font: usize,
    pub bold: bool,
    pub text: String,
    pub colour: Option<Rgb>,
    /// the fill colour state when the text was shown, for putting it back
    pub fill_state: ColourState,
    /// drawn in an invisible render mode, like the text layer of a scan
    pub invisible: bool,
    pub mark: Mark,
    pub lang: Option<String>,
    /// the font can turn these codes into real characters
    pub unicode_reliable: bool,
}

/// one picture: an image xobject or an inline image.
#[derive(Clone, Debug)]
pub struct ImageUse {
    pub op_index: usize,
    pub in_form: bool,
    pub rect: Rect,
    pub mark: Mark,
}

/// a straight line on the visible page, used to find ruled tables.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// one painted shape: lines, boxes, backgrounds.
#[derive(Clone, Debug)]
pub struct PathUse {
    /// the first operator of the path and the operator that paints it
    pub op_start: usize,
    pub op_end: usize,
    pub in_form: bool,
    pub rect: Rect,
    pub mark: Mark,
    pub segments: Vec<Segment>,
}

/// a font used by text on the page.
#[derive(Clone, Debug)]
pub struct FontSummary {
    pub id: Option<ObjectId>,
    pub name: String,
    pub embedded: bool,
    pub unicode: UnicodeSource,
    /// used by at least one piece of text that is actually drawn
    pub drawn: bool,
}

/// everything the reader found on one page.
#[derive(Clone, Debug, Default)]
pub struct PageContent {
    pub operations: Vec<Operation>,
    pub texts: Vec<TextRun>,
    pub images: Vec<ImageUse>,
    pub paths: Vec<PathUse>,
    /// every marked-content id used at the top level of the page
    pub mcids: Vec<i64>,
    pub fonts: Vec<FontSummary>,
    /// an `EMC` without a start, or a sequence left open at the end
    pub unbalanced: bool,
}

impl PageContent {
    /// the highest marked-content id used on the page.
    pub fn max_mcid(&self) -> Option<i64> {
        self.mcids.iter().copied().max()
    }
}

/// reads one page. fails only when the content can't be parsed at all.
pub fn read_page(doc: &Document, page: ObjectId, frame: &PageFrame) -> Result<PageContent, ()> {
    let bytes = doc.get_page_content(page);
    let content = Content::decode(&bytes).map_err(|_| ())?;
    let resources = inherited(doc, page, b"Resources").and_then(|r| r.as_dict().ok());
    let mut reader = Reader::new(doc, *frame);
    reader.run(&content.operations, resources, None);
    reader.finish(content.operations)
}

#[derive(Clone)]
struct TextState<'a> {
    font: Option<Rc<FontInfo<'a>>>,
    font_index: usize,
    size: f32,
    char_spacing: f32,
    word_spacing: f32,
    horizontal_scale: f32,
    leading: f32,
    rise: f32,
    render_mode: i64,
}

#[derive(Clone)]
struct GraphicsState<'a> {
    ctm: Matrix,
    fill: ColourState,
    stroke: ColourState,
    text: TextState<'a>,
}

impl Default for GraphicsState<'_> {
    fn default() -> Self {
        Self {
            ctm: Matrix::IDENTITY,
            fill: ColourState::default(),
            stroke: ColourState::default(),
            text: TextState {
                font: None,
                font_index: 0,
                size: 0.0,
                char_spacing: 0.0,
                word_spacing: 0.0,
                horizontal_scale: 1.0,
                leading: 0.0,
                rise: 0.0,
                render_mode: 0,
            },
        }
    }
}

struct MarkEntry {
    mcid: Option<i64>,
    artifact: bool,
    lang: Option<String>,
}

/// a path being built, before it is painted.
#[derive(Default)]
struct PathInProgress {
    start: Option<usize>,
    points: Vec<(f32, f32)>,
    segments: Vec<((f32, f32), (f32, f32))>,
    current: Option<(f32, f32)>,
    subpath_start: Option<(f32, f32)>,
}

struct Reader<'a> {
    doc: &'a Document,
    frame: PageFrame,
    state: GraphicsState<'a>,
    saved: Vec<GraphicsState<'a>>,
    text_matrix: Matrix,
    line_matrix: Matrix,
    marks: Vec<MarkEntry>,
    path: PathInProgress,
    forms: Vec<ObjectId>,
    fonts: HashMap<ObjectId, (Rc<FontInfo<'a>>, usize)>,
    out: PageContent,
    operator_count: usize,
}

impl<'a> Reader<'a> {
    fn new(doc: &'a Document, frame: PageFrame) -> Self {
        Self {
            doc,
            frame,
            state: GraphicsState::default(),
            saved: Vec::new(),
            text_matrix: Matrix::IDENTITY,
            line_matrix: Matrix::IDENTITY,
            marks: Vec::new(),
            path: PathInProgress::default(),
            forms: Vec::new(),
            fonts: HashMap::new(),
            out: PageContent::default(),
            operator_count: 0,
        }
    }

    fn finish(mut self, operations: Vec<Operation>) -> Result<PageContent, ()> {
        if !self.marks.is_empty() {
            self.out.unbalanced = true;
        }
        self.out.operations = operations;
        Ok(self.out)
    }

    /// follows a list of operators. `form_op` is the top level `Do` when
    /// these operators belong to a form xobject.
    fn run(
        &mut self,
        operations: &[Operation],
        resources: Option<&'a Dictionary>,
        form_op: Option<usize>,
    ) {
        for (index, operation) in operations.iter().enumerate() {
            self.operator_count += 1;
            if self.operator_count > OPERATOR_LIMIT {
                return;
            }
            let top_index = form_op.unwrap_or(index);
            self.step(operation, resources, top_index, form_op.is_some());
        }
    }

    fn step(
        &mut self,
        op: &Operation,
        resources: Option<&'a Dictionary>,
        top: usize,
        in_form: bool,
    ) {
        let numbers = numbers(&op.operands);
        let n = |i: usize| numbers.get(i).copied().unwrap_or(0.0);
        match op.operator.as_str() {
            "q" => self.saved.push(self.state.clone()),
            "Q" => {
                if let Some(state) = self.saved.pop() {
                    self.state = state;
                }
            }
            "cm" if numbers.len() == 6 => {
                let matrix = Matrix::new([n(0), n(1), n(2), n(3), n(4), n(5)]);
                self.state.ctm = matrix.then(&self.state.ctm);
            }
            // colours
            "g" => self.state.fill = ColourState::device(ColourSpace::Gray, numbers.clone()),
            "G" => self.state.stroke = ColourState::device(ColourSpace::Gray, numbers.clone()),
            "rg" => self.state.fill = ColourState::device(ColourSpace::Rgb, numbers.clone()),
            "RG" => self.state.stroke = ColourState::device(ColourSpace::Rgb, numbers.clone()),
            "k" => self.state.fill = ColourState::device(ColourSpace::Cmyk, numbers.clone()),
            "K" => self.state.stroke = ColourState::device(ColourSpace::Cmyk, numbers.clone()),
            "cs" | "CS" => {
                let colour = self.colour_space(op, resources);
                if op.operator == "cs" {
                    self.state.fill = colour;
                } else {
                    self.state.stroke = colour;
                }
            }
            "sc" | "scn" => set_components(&mut self.state.fill, op, numbers.clone()),
            "SC" | "SCN" => set_components(&mut self.state.stroke, op, numbers.clone()),
            // text objects and state
            "BT" => {
                self.text_matrix = Matrix::IDENTITY;
                self.line_matrix = Matrix::IDENTITY;
            }
            "Tf" => self.set_font(op, resources),
            "Tc" => self.state.text.char_spacing = n(0),
            "Tw" => self.state.text.word_spacing = n(0),
            "Tz" => self.state.text.horizontal_scale = n(0) / 100.0,
            "TL" => self.state.text.leading = n(0),
            "Ts" => self.state.text.rise = n(0),
            "Tr" => self.state.text.render_mode = n(0) as i64,
            "Td" => self.move_line(n(0), n(1)),
            "TD" => {
                self.state.text.leading = -n(1);
                self.move_line(n(0), n(1));
            }
            "Tm" if numbers.len() == 6 => {
                self.text_matrix = Matrix::new([n(0), n(1), n(2), n(3), n(4), n(5)]);
                self.line_matrix = self.text_matrix;
            }
            "T*" => self.next_line(),
            "Tj" => self.show(&op.operands, top, in_form),
            "TJ" => self.show(&op.operands, top, in_form),
            "'" => {
                self.next_line();
                self.show(&op.operands, top, in_form);
            }
            "\"" => {
                self.state.text.word_spacing = n(0);
                self.state.text.char_spacing = n(1);
                self.next_line();
                self.show(&op.operands[op.operands.len().min(2)..], top, in_form);
            }
            // marked content
            "BMC" | "BDC" => self.begin_marked(op, resources, in_form),
            "EMC" => self.end_marked(),
            // pictures and forms
            "Do" => self.draw_xobject(op, resources, top, in_form),
            "BI" => self.draw_image(top, in_form),
            "sh" => {
                let (width, height) = self.frame.visible_size();
                let mark = self.current_mark();
                self.out.paths.push(PathUse {
                    op_start: top,
                    op_end: top,
                    in_form,
                    rect: Rect::new(0.0, 0.0, width, height),
                    mark,
                    segments: Vec::new(),
                });
            }
            // paths
            "m" | "l" | "c" | "v" | "y" | "h" | "re" => {
                self.build_path(op.operator.as_str(), &numbers, top)
            }
            "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" => {
                self.paint_path(top, in_form);
            }
            "n" => self.path = PathInProgress::default(),
            _ => {}
        }
    }

    // ---- marked content ----------------------------------------------------

    fn begin_marked(&mut self, op: &Operation, resources: Option<&'a Dictionary>, in_form: bool) {
        let tag = op
            .operands
            .first()
            .and_then(|tag| tag.as_name().ok())
            .map(name_text)
            .unwrap_or_default();
        let properties = op
            .operands
            .get(1)
            .and_then(|properties| self.properties(properties, resources));
        let mcid = properties
            .and_then(|p| get(self.doc, p, b"MCID"))
            .and_then(|m| m.as_i64().ok());
        let lang = properties.and_then(|p| crate::objects::get_text(self.doc, p, b"Lang"));
        // mcids inside forms are numbered separately, by the form
        if let (Some(mcid), false) = (mcid, in_form) {
            self.out.mcids.push(mcid);
        }
        self.marks.push(MarkEntry {
            mcid,
            artifact: tag == "Artifact",
            lang,
        });
    }

    /// the property list of a `BDC`: written inline, or named in the
    /// page's /Properties resources.
    fn properties(
        &self,
        operand: &'a Object,
        resources: Option<&'a Dictionary>,
    ) -> Option<&'a Dictionary> {
        match operand {
            Object::Dictionary(dict) => Some(dict),
            Object::Name(name) => {
                let list = get_dict(self.doc, resources?, b"Properties")?;
                resolve(self.doc, list.get(name).ok()?).as_dict().ok()
            }
            _ => None,
        }
    }

    fn end_marked(&mut self) {
        if self.marks.pop().is_none() {
            self.out.unbalanced = true;
        }
    }

    fn current_mark(&self) -> Mark {
        for entry in self.marks.iter().rev() {
            if entry.artifact {
                return Mark::Artifact;
            }
            if let Some(mcid) = entry.mcid {
                return Mark::Mcid(mcid);
            }
        }
        Mark::Untagged
    }

    fn current_lang(&self) -> Option<String> {
        self.marks.iter().rev().find_map(|entry| entry.lang.clone())
    }

    // ---- colours -----------------------------------------------------------

    fn colour_space(&self, op: &Operation, resources: Option<&'a Dictionary>) -> ColourState {
        let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) else {
            return ColourState::default();
        };
        let space = match name {
            b"DeviceGray" => ColourSpace::Gray,
            b"DeviceRGB" => ColourSpace::Rgb,
            b"DeviceCMYK" => ColourSpace::Cmyk,
            b"Pattern" => ColourSpace::Pattern,
            _ => resources
                .and_then(|r| get_dict(self.doc, r, b"ColorSpace"))
                .and_then(|spaces| spaces.get(name).ok())
                .map(|space| self.named_space(space))
                .unwrap_or(ColourSpace::Other),
        };
        // a new colour space starts at its initial colour: black, or no pattern
        let components = match space {
            ColourSpace::Rgb => vec![0.0; 3],
            ColourSpace::Cmyk => vec![0.0, 0.0, 0.0, 1.0],
            ColourSpace::Pattern => Vec::new(),
            _ => vec![0.0],
        };
        ColourState {
            space,
            components,
            space_name: Some(name.to_vec()),
            pattern_name: None,
        }
    }

    fn named_space(&self, space: &Object) -> ColourSpace {
        let space = resolve(self.doc, space);
        let family = match space {
            Object::Name(name) => name.as_slice(),
            Object::Array(items) => match items.first().and_then(|f| f.as_name().ok()) {
                Some(name) => name,
                None => return ColourSpace::Other,
            },
            _ => return ColourSpace::Other,
        };
        match family {
            b"DeviceGray" | b"CalGray" => ColourSpace::Gray,
            b"DeviceRGB" | b"CalRGB" => ColourSpace::Rgb,
            b"DeviceCMYK" => ColourSpace::Cmyk,
            b"Pattern" => ColourSpace::Pattern,
            b"ICCBased" => {
                // an icc profile says how many components it takes
                let count = space
                    .as_array()
                    .ok()
                    .and_then(|items| items.get(1))
                    .and_then(|profile| resolve(self.doc, profile).as_stream().ok())
                    .and_then(|profile| get(self.doc, &profile.dict, b"N"))
                    .and_then(|n| n.as_i64().ok());
                match count {
                    Some(1) => ColourSpace::Gray,
                    Some(3) => ColourSpace::Rgb,
                    Some(4) => ColourSpace::Cmyk,
                    _ => ColourSpace::Other,
                }
            }
            _ => ColourSpace::Other,
        }
    }

    // ---- text --------------------------------------------------------------

    fn set_font(&mut self, op: &Operation, resources: Option<&'a Dictionary>) {
        self.state.text.size = op
            .operands
            .get(1)
            .and_then(|s| s.as_float().ok())
            .unwrap_or(0.0);
        self.state.text.font = None;
        let Some(name) = op.operands.first().and_then(|n| n.as_name().ok()) else {
            return;
        };
        let Some(entry) = resources
            .and_then(|r| get_dict(self.doc, r, b"Font"))
            .and_then(|fonts| fonts.get(name).ok())
        else {
            return;
        };
        let id = entry.as_reference().ok();
        if let Some((font, index)) = id.and_then(|id| self.fonts.get(&id)) {
            self.state.text.font = Some(font.clone());
            self.state.text.font_index = *index;
            return;
        }
        let Ok(dict) = resolve(self.doc, entry).as_dict() else {
            return;
        };
        let font = Rc::new(FontInfo::read(self.doc, dict));
        self.out.fonts.push(FontSummary {
            id,
            name: font.display_name(),
            embedded: font.embedded,
            unicode: font.unicode,
            drawn: false,
        });
        let index = self.out.fonts.len() - 1;
        if let Some(id) = id {
            self.fonts.insert(id, (font.clone(), index));
        }
        self.state.text.font = Some(font);
        self.state.text.font_index = index;
    }

    fn move_line(&mut self, x: f32, y: f32) {
        self.line_matrix = Matrix::translate(x, y).then(&self.line_matrix);
        self.text_matrix = self.line_matrix;
    }

    fn next_line(&mut self) {
        let leading = self.state.text.leading;
        self.move_line(0.0, -leading);
    }

    /// records one shown string and moves the text position past it.
    fn show(&mut self, operands: &[Object], top: usize, in_form: bool) {
        let Some(font) = self.state.text.font.clone() else {
            return;
        };
        let text_state = &self.state.text;
        let size = text_state.size;
        let mut advance = 0.0;
        let mut text = String::new();
        let mut shown_any = false;
        let add_string = |bytes: &[u8], advance: &mut f32, text: &mut String| {
            for code in font.codes(bytes) {
                let spacing = if !font.two_byte && code == 32 {
                    text_state.word_spacing
                } else {
                    0.0
                };
                *advance += (font.advance(code) * size + text_state.char_spacing + spacing)
                    * text_state.horizontal_scale;
            }
            text.push_str(&font.decode(bytes));
        };
        for operand in operands {
            match operand {
                Object::String(bytes, _) => {
                    add_string(bytes, &mut advance, &mut text);
                    shown_any = true;
                }
                Object::Array(items) => {
                    for item in items {
                        match item {
                            Object::String(bytes, _) => {
                                add_string(bytes, &mut advance, &mut text);
                                shown_any = true;
                            }
                            Object::Integer(_) | Object::Real(_) => {
                                let adjust = item.as_float().unwrap_or(0.0);
                                advance -= adjust / 1000.0 * size * text_state.horizontal_scale;
                                // a big gap in a TJ array usually stands for a space
                                if adjust <= -250.0 && !text.ends_with(' ') {
                                    text.push(' ');
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        if !shown_any {
            return;
        }
        let rise = text_state.rise;
        let invisible = matches!(text_state.render_mode, 3 | 7);
        let stroke_only = matches!(text_state.render_mode, 1 | 5);
        let to_page = self.text_matrix.then(&self.state.ctm);
        let corners = [
            (0.0, rise - 0.22 * size),
            (advance, rise - 0.22 * size),
            (0.0, rise + 0.78 * size),
            (advance, rise + 0.78 * size),
        ];
        let page_points: Vec<(f32, f32)> =
            corners.iter().map(|&(x, y)| to_page.apply(x, y)).collect();
        let fill_state = if stroke_only {
            self.state.stroke.clone()
        } else {
            self.state.fill.clone()
        };
        let font_index = text_state.font_index;
        let run = TextRun {
            op_index: top,
            in_form,
            rect: self.frame.visible_rect(&page_points),
            font_size: size * to_page.vertical_scale(),
            font: font_index,
            bold: font.bold,
            text,
            colour: fill_state.rgb(),
            fill_state,
            invisible,
            mark: self.current_mark(),
            lang: self.current_lang(),
            unicode_reliable: font.unicode != UnicodeSource::Missing,
        };
        if !invisible {
            self.out.fonts[font_index].drawn = true;
        }
        self.out.texts.push(run);
        self.text_matrix = Matrix::translate(advance, 0.0).then(&self.text_matrix);
    }

    // ---- pictures and forms -------------------------------------------------

    fn draw_xobject(
        &mut self,
        op: &Operation,
        resources: Option<&'a Dictionary>,
        top: usize,
        in_form: bool,
    ) {
        let Some(name) = op.operands.first().and_then(|n| n.as_name().ok()) else {
            return;
        };
        let Some(entry) = resources
            .and_then(|r| get_dict(self.doc, r, b"XObject"))
            .and_then(|list| list.get(name).ok())
        else {
            return;
        };
        let id = entry.as_reference().ok();
        let Ok(stream) = resolve(self.doc, entry).as_stream() else {
            return;
        };
        match get_name(self.doc, &stream.dict, b"Subtype") {
            Some(b"Image") => self.draw_image(top, in_form),
            Some(b"Form") => self.draw_form(id, stream, resources, top),
            _ => {}
        }
    }

    fn draw_image(&mut self, top: usize, in_form: bool) {
        // an image fills the unit square of the current coordinates
        let ctm = self.state.ctm;
        let corners: Vec<(f32, f32)> = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
            .iter()
            .map(|&(x, y)| ctm.apply(x, y))
            .collect();
        let mark = self.current_mark();
        self.out.images.push(ImageUse {
            op_index: top,
            in_form,
            rect: self.frame.visible_rect(&corners),
            mark,
        });
    }

    fn draw_form(
        &mut self,
        id: Option<ObjectId>,
        stream: &'a lopdf::Stream,
        resources: Option<&'a Dictionary>,
        top: usize,
    ) {
        if self.forms.len() >= FORM_DEPTH_LIMIT || id.is_some_and(|id| self.forms.contains(&id)) {
            return;
        }
        let bytes = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        let Ok(content) = Content::decode(&bytes) else {
            return;
        };
        let form_resources = get_dict(self.doc, &stream.dict, b"Resources").or(resources);
        let matrix = get(self.doc, &stream.dict, b"Matrix")
            .map(|m| numbers(m.as_array().map(Vec::as_slice).unwrap_or_default()))
            .filter(|m| m.len() == 6)
            .map(|m| Matrix::new([m[0], m[1], m[2], m[3], m[4], m[5]]))
            .unwrap_or(Matrix::IDENTITY);

        let saved_state = self.state.clone();
        let saved_depth = self.saved.len();
        let saved_text = (self.text_matrix, self.line_matrix);
        self.state.ctm = matrix.then(&self.state.ctm);
        if let Some(id) = id {
            self.forms.push(id);
        }
        self.run(&content.operations, form_resources, Some(top));
        if id.is_some() {
            self.forms.pop();
        }
        self.saved.truncate(saved_depth);
        self.state = saved_state;
        (self.text_matrix, self.line_matrix) = saved_text;
    }

    // ---- paths -------------------------------------------------------------

    fn build_path(&mut self, operator: &str, numbers: &[f32], top: usize) {
        let ctm = self.state.ctm;
        let point = |i: usize| {
            ctm.apply(
                numbers.get(i).copied().unwrap_or(0.0),
                numbers.get(i + 1).copied().unwrap_or(0.0),
            )
        };
        let path = &mut self.path;
        path.start.get_or_insert(top);
        match operator {
            "m" => {
                let p = point(0);
                path.points.push(p);
                path.current = Some(p);
                path.subpath_start = Some(p);
            }
            "l" => {
                let p = point(0);
                if let Some(from) = path.current {
                    path.segments.push((from, p));
                }
                path.points.push(p);
                path.current = Some(p);
            }
            "c" | "v" | "y" => {
                // curves only count towards the outline; they are never table rules
                let count = if operator == "c" { 3 } else { 2 };
                let mut last = None;
                for i in 0..count {
                    let p = point(i * 2);
                    path.points.push(p);
                    last = Some(p);
                }
                path.current = last;
            }
            "h" => {
                if let (Some(from), Some(to)) = (path.current, path.subpath_start) {
                    path.segments.push((from, to));
                    path.current = Some(to);
                }
            }
            "re" => {
                let x = numbers.first().copied().unwrap_or(0.0);
                let y = numbers.get(1).copied().unwrap_or(0.0);
                let w = numbers.get(2).copied().unwrap_or(0.0);
                let h = numbers.get(3).copied().unwrap_or(0.0);
                let corners = [
                    ctm.apply(x, y),
                    ctm.apply(x + w, y),
                    ctm.apply(x + w, y + h),
                    ctm.apply(x, y + h),
                ];
                for i in 0..4 {
                    path.segments.push((corners[i], corners[(i + 1) % 4]));
                }
                path.points.extend_from_slice(&corners);
                path.current = Some(corners[0]);
                path.subpath_start = Some(corners[0]);
            }
            _ => {}
        }
    }

    fn paint_path(&mut self, top: usize, in_form: bool) {
        let path = std::mem::take(&mut self.path);
        if path.points.is_empty() {
            return;
        }
        let frame = self.frame;
        let segments = path
            .segments
            .iter()
            .map(|&((x0, y0), (x1, y1))| {
                let (x0, y0) = frame.to_visible(x0, y0);
                let (x1, y1) = frame.to_visible(x1, y1);
                Segment { x0, y0, x1, y1 }
            })
            .collect();
        let mark = self.current_mark();
        self.out.paths.push(PathUse {
            op_start: if in_form {
                top
            } else {
                path.start.unwrap_or(top)
            },
            op_end: top,
            in_form,
            rect: frame.visible_rect(&path.points),
            mark,
            segments,
        });
    }
}

/// the numeric operands of an operator.
fn numbers(operands: &[Object]) -> Vec<f32> {
    operands.iter().filter_map(|o| o.as_float().ok()).collect()
}

fn set_components(colour: &mut ColourState, op: &Operation, numbers: Vec<f32>) {
    colour.pattern_name = op
        .operands
        .last()
        .and_then(|last| last.as_name().ok())
        .map(<[u8]>::to_vec);
    if colour.pattern_name.is_some() && colour.space != ColourSpace::Pattern {
        colour.space = ColourSpace::Pattern;
    }
    colour.components = numbers;
}
