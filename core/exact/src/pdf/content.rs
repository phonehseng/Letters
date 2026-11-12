//! a content stream interpreter that records glyphs, paths and pictures
//! (recipe 2).
//!
//! pdfium's text api gives unicode and pen origins, but neither the char
//! codes nor the font program behind each glyph, so the text operators are
//! interpreted here (pdf 1.7, 9.4): every glyph gets its true pen origin
//! from the text rendering matrix, its code, its font and its advance.
//! paths and pictures come from the same pass, so their order relative to
//! text is the page's own. coordinates are the page's user space in points,
//! with the origin at the bottom left of the media box.

use std::collections::HashMap;
use std::sync::Arc;

use lopdf::{Dictionary, Document, Object};

use super::fonts::{read_font, PdfFont};
use super::images::{decode_image, inline_image, PictureData};
use super::lexer::{parse, Operand, Operation};
use super::matrix::{apply, multiply, scale_of, translation, Matrix, IDENTITY};
use super::objects::{get, get_dict, get_number, numbers, resolve, stream_bytes};

pub type Rgb = [f64; 3];
pub type Point = (f64, f64);

/// one shown glyph.
#[derive(Clone, Debug)]
pub struct Glyph {
    pub page: usize,
    pub font: Arc<PdfFont>,
    pub code: u32,
    pub text: String,
    /// pen origin in page space
    pub x: f64,
    pub y: f64,
    /// font size in page space, along the glyph's vertical axis
    pub size: f64,
    /// horizontal over vertical scale, 1 unless `Tz` or a stretched matrix
    pub horizontal_ratio: f64,
    /// degrees, counter-clockwise
    pub angle: f64,
    /// pen advance in page space
    pub advance: f64,
    pub color: Rgb,
    pub render_mode: i64,
    /// order of appearance on the page
    pub sequence: usize,
}

impl Glyph {
    /// render modes 3 and 7 draw nothing: invisible text stays invisible.
    pub fn invisible(&self) -> bool {
        matches!(self.render_mode, 3 | 7)
    }
}

/// one piece of a path, in page space.
#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Close,
    /// a rectangle from `re`: its four corners in order
    Rect([Point; 4]),
}

impl Segment {
    pub fn points(&self) -> Vec<Point> {
        match self {
            Segment::Move(p) | Segment::Line(p) => vec![*p],
            Segment::Curve(a, b, c) => vec![*a, *b, *c],
            Segment::Close => Vec::new(),
            Segment::Rect(corners) => corners.to_vec(),
        }
    }
}

/// one painted path, or a shading (`sh`).
#[derive(Clone, Debug)]
pub struct PathItem {
    pub page: usize,
    pub segments: Vec<Segment>,
    pub fill: bool,
    pub stroke: bool,
    pub even_odd: bool,
    pub fill_color: Option<Rgb>,
    pub stroke_color: Option<Rgb>,
    pub line_width: f64,
    pub dashed: bool,
    pub alpha: f64,
    /// every clip path in force, in page space
    pub clip: Vec<Vec<Segment>>,
    pub pattern: bool,
    /// operators that redraw exactly this item on an empty page
    pub raw: Vec<u8>,
    pub sequence: usize,
}

impl PathItem {
    pub fn has_curves(&self) -> bool {
        self.segments
            .iter()
            .any(|s| matches!(s, Segment::Curve(..)))
    }

    pub fn bbox(&self) -> (f64, f64, f64, f64) {
        let points: Vec<Point> = self.segments.iter().flat_map(Segment::points).collect();
        let half = if self.stroke {
            self.line_width / 2.0
        } else {
            0.0
        };
        let (mut left, mut bottom) = (f64::INFINITY, f64::INFINITY);
        let (mut right, mut top) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (x, y) in points {
            left = left.min(x);
            right = right.max(x);
            bottom = bottom.min(y);
            top = top.max(y);
        }
        (left - half, bottom - half, right + half, top + half)
    }
}

/// one placed picture.
#[derive(Clone, Debug)]
pub struct ImageItem {
    pub page: usize,
    /// the unit square to page space
    pub matrix: Matrix,
    pub picture: PictureData,
    pub sequence: usize,
}

/// everything read from one page.
#[derive(Clone, Debug, Default)]
pub struct PageContent {
    pub index: usize,
    pub width: f64,
    pub height: f64,
    pub rotate: i64,
    pub glyphs: Vec<Glyph>,
    pub paths: Vec<PathItem>,
    pub images: Vec<ImageItem>,
    pub shadings: Vec<PathItem>,
    pub problems: Vec<String>,
}

#[derive(Clone, Debug)]
struct GraphicsState {
    ctm: Matrix,
    fill_color: Rgb,
    stroke_color: Rgb,
    fill_space: ColorSpace,
    stroke_space: ColorSpace,
    fill_pattern: bool,
    stroke_pattern: bool,
    line_width: f64,
    dashed: bool,
    fill_alpha: f64,
    stroke_alpha: f64,
    clip: Vec<Vec<Segment>>,
    // the text state survives BT/ET and is saved with q
    char_spacing: f64,
    word_spacing: f64,
    horizontal_scaling: f64,
    leading: f64,
    font: Option<Arc<PdfFont>>,
    font_size: f64,
    render_mode: i64,
    rise: f64,
    /// colour and gs operators, replayed when an item is rasterised
    raw_state: Vec<Vec<u8>>,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: IDENTITY,
            fill_color: [0.0; 3],
            stroke_color: [0.0; 3],
            fill_space: ColorSpace::Gray,
            stroke_space: ColorSpace::Gray,
            fill_pattern: false,
            stroke_pattern: false,
            line_width: 1.0,
            dashed: false,
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
            clip: Vec::new(),
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scaling: 1.0,
            leading: 0.0,
            font: None,
            font_size: 0.0,
            render_mode: 0,
            rise: 0.0,
            raw_state: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ColorSpace {
    Gray,
    Rgb,
    Cmyk,
    Pattern,
}

/// colour values as rgb: one value is gray, three rgb, four cmyk.
fn to_rgb(values: &[f64]) -> Rgb {
    match values {
        [gray] => [*gray; 3],
        [r, g, b] => [*r, *g, *b],
        [c, m, y, k] => [
            (1.0 - c) * (1.0 - k),
            (1.0 - m) * (1.0 - k),
            (1.0 - y) * (1.0 - k),
        ],
        _ => [0.0; 3],
    }
}

fn numbers_of(operands: &[Operand]) -> Vec<f64> {
    operands.iter().filter_map(Operand::number).collect()
}

/// reads glyphs, paths and pictures from pages, one font cache for the document.
pub struct Interpreter<'a> {
    document: &'a Document,
    fonts: HashMap<String, Arc<PdfFont>>,
}

/// state of one page being read.
struct PageRun<'p> {
    content: &'p mut PageContent,
    sequence: usize,
    origin_shift: Matrix,
}

impl<'a> Interpreter<'a> {
    pub fn new(document: &'a Document) -> Self {
        Self {
            document,
            fonts: HashMap::new(),
        }
    }

    /// reads one page whose media box starts at `origin`.
    pub fn read_page(
        &mut self,
        content: &mut PageContent,
        stream: &[u8],
        resources: Option<&Dictionary>,
        origin: (f64, f64),
    ) {
        let mut run = PageRun {
            content,
            sequence: 0,
            origin_shift: translation(-origin.0, -origin.1),
        };
        self.run(&mut run, stream, resources, GraphicsState::default(), 0);
    }

    fn font_for(&mut self, resources: Option<&Dictionary>, name: &[u8]) -> Option<Arc<PdfFont>> {
        let document = self.document;
        let fonts = get_dict(document, resources?, b"Font")?;
        let entry = fonts.get(name).ok()?;
        let key = match entry {
            Object::Reference(id) => format!("{} {}", id.0, id.1),
            _ => format!("direct {}", String::from_utf8_lossy(name)),
        };
        if let Some(font) = self.fonts.get(&key) {
            return Some(font.clone());
        }
        let Object::Dictionary(dictionary) = resolve(document, entry) else {
            return None;
        };
        let font = Arc::new(read_font(document, dictionary, key.clone()));
        self.fonts.insert(key, font.clone());
        Some(font)
    }

    fn run(
        &mut self,
        page: &mut PageRun,
        stream: &[u8],
        resources: Option<&Dictionary>,
        mut state: GraphicsState,
        depth: usize,
    ) {
        if depth > 8 {
            page.content
                .problems
                .push("form xobjects nested too deep, rest skipped".into());
            return;
        }
        let mut stack: Vec<GraphicsState> = Vec::new();
        let mut text_matrix = IDENTITY;
        let mut line_matrix = IDENTITY;
        let mut path: Vec<(String, Vec<f64>)> = Vec::new();
        let mut path_raw: Vec<u8> = Vec::new();
        let mut pending_clip: Option<&str> = None;
        for operation in parse(stream) {
            let op = operation.operator.as_str();
            let values = numbers_of(&operation.operands);
            let raw = || stream[operation.span.clone()].to_vec();
            match op {
                "q" => stack.push(state.clone()),
                "Q" => {
                    if let Some(saved) = stack.pop() {
                        state = saved;
                    }
                }
                "cm" if values.len() == 6 => {
                    let m: Matrix = [
                        values[0], values[1], values[2], values[3], values[4], values[5],
                    ];
                    state.ctm = multiply(&m, &state.ctm);
                }
                "g" | "rg" | "k" => {
                    state.fill_color = to_rgb(&values);
                    state.fill_pattern = false;
                    state.raw_state.push(with_newline(raw()));
                }
                "G" | "RG" | "K" => {
                    state.stroke_color = to_rgb(&values);
                    state.stroke_pattern = false;
                    state.raw_state.push(with_newline(raw()));
                }
                "cs" | "CS" => {
                    let space = operation
                        .operands
                        .first()
                        .and_then(Operand::name)
                        .map(|name| self.color_space(resources, name));
                    if let Some(space) = space {
                        if op == "cs" {
                            state.fill_space = space;
                        } else {
                            state.stroke_space = space;
                        }
                    }
                    state.raw_state.push(with_newline(raw()));
                }
                "sc" | "scn" => {
                    state.raw_state.push(with_newline(raw()));
                    if state.fill_space == ColorSpace::Pattern {
                        state.fill_pattern = true;
                    } else {
                        state.fill_color = to_rgb(&values);
                    }
                }
                "SC" | "SCN" => {
                    state.raw_state.push(with_newline(raw()));
                    if state.stroke_space == ColorSpace::Pattern {
                        state.stroke_pattern = true;
                    } else {
                        state.stroke_color = to_rgb(&values);
                    }
                }
                "w" => {
                    if let Some(width) = values.first() {
                        state.line_width = *width;
                    }
                    state.raw_state.push(with_newline(raw()));
                }
                "J" | "j" | "M" => state.raw_state.push(with_newline(raw())),
                "d" => {
                    state.dashed = matches!(operation.operands.first(), Some(Operand::Array(items)) if !items.is_empty());
                    state.raw_state.push(with_newline(raw()));
                }
                "gs" => {
                    if let Some(name) = operation.operands.first().and_then(Operand::name) {
                        self.ext_g_state(resources, name, &mut state);
                    }
                    state.raw_state.push(with_newline(raw()));
                }
                // ------------------------------------------------ paths
                "m" | "l" | "c" | "v" | "y" | "h" | "re" => {
                    path.push((op.to_string(), values));
                    path_raw.extend(raw());
                    path_raw.push(b'\n');
                }
                "W" => pending_clip = Some("W"),
                "W*" => pending_clip = Some("W*"),
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                    if !path.is_empty() {
                        let segments =
                            page_segments(&path, &multiply(&state.ctm, &page.origin_shift));
                        if op != "n" {
                            self.record_path(page, segments.clone(), op, &state, &path_raw);
                        }
                        if pending_clip.is_some() {
                            state.clip.push(segments);
                        }
                    }
                    path.clear();
                    path_raw.clear();
                    pending_clip = None;
                }
                "sh" => self.record_shading(page, &operation, &state),
                // ------------------------------------------------ text
                "BT" => {
                    text_matrix = IDENTITY;
                    line_matrix = IDENTITY;
                }
                "Tf" => {
                    state.font = operation
                        .operands
                        .first()
                        .and_then(Operand::name)
                        .and_then(|name| self.font_for(resources, name));
                    state.font_size = values.first().copied().unwrap_or(0.0);
                }
                "Tc" => state.char_spacing = values.first().copied().unwrap_or(0.0),
                "Tw" => state.word_spacing = values.first().copied().unwrap_or(0.0),
                "Tz" => state.horizontal_scaling = values.first().copied().unwrap_or(100.0) / 100.0,
                "TL" => state.leading = values.first().copied().unwrap_or(0.0),
                "Tr" => state.render_mode = values.first().copied().unwrap_or(0.0) as i64,
                "Ts" => state.rise = values.first().copied().unwrap_or(0.0),
                "Td" | "TD" if values.len() == 2 => {
                    if op == "TD" {
                        state.leading = -values[1];
                    }
                    line_matrix = multiply(&translation(values[0], values[1]), &line_matrix);
                    text_matrix = line_matrix;
                }
                "Tm" if values.len() == 6 => {
                    line_matrix = [
                        values[0], values[1], values[2], values[3], values[4], values[5],
                    ];
                    text_matrix = line_matrix;
                }
                "T*" => {
                    line_matrix = multiply(&translation(0.0, -state.leading), &line_matrix);
                    text_matrix = line_matrix;
                }
                "Tj" | "'" | "\"" => {
                    if op != "Tj" {
                        if op == "\"" && values.len() >= 2 {
                            state.word_spacing = values[0];
                            state.char_spacing = values[1];
                        }
                        line_matrix = multiply(&translation(0.0, -state.leading), &line_matrix);
                        text_matrix = line_matrix;
                    }
                    if let Some(bytes) = operation.operands.last().and_then(Operand::string) {
                        text_matrix = self.show(page, bytes, text_matrix, &state);
                    }
                }
                "TJ" => {
                    if let Some(Operand::Array(items)) = operation.operands.first() {
                        for item in items {
                            match item {
                                Operand::String(bytes) => {
                                    text_matrix = self.show(page, bytes, text_matrix, &state);
                                }
                                Operand::Number(number) => {
                                    let shift = -number / 1000.0
                                        * state.font_size
                                        * state.horizontal_scaling;
                                    text_matrix = multiply(&translation(shift, 0.0), &text_matrix);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                // ------------------------------------------------ xobjects
                "Do" => {
                    if let Some(name) = operation.operands.first().and_then(Operand::name) {
                        self.do_xobject(page, name, resources, &state, depth);
                    }
                }
                "BI" => self.record_inline_image(page, &operation, &state),
                _ => {}
            }
        }
    }

    fn color_space(&self, resources: Option<&Dictionary>, name: &[u8]) -> ColorSpace {
        match name {
            b"DeviceGray" | b"G" => return ColorSpace::Gray,
            b"DeviceRGB" | b"RGB" => return ColorSpace::Rgb,
            b"DeviceCMYK" | b"CMYK" => return ColorSpace::Cmyk,
            b"Pattern" => return ColorSpace::Pattern,
            _ => {}
        }
        let document = self.document;
        let entry = resources
            .and_then(|r| get_dict(document, r, b"ColorSpace"))
            .and_then(|spaces| get(document, spaces, name));
        if let Some(Object::Array(items)) = entry {
            let family = items.first().map(|item| resolve(document, item));
            match family {
                Some(Object::Name(n)) if n == b"ICCBased" => {
                    let components = items
                        .get(1)
                        .and_then(|stream| match resolve(document, stream) {
                            Object::Stream(s) => get_number(document, &s.dict, b"N"),
                            _ => None,
                        })
                        .unwrap_or(3.0);
                    return match components as i64 {
                        1 => ColorSpace::Gray,
                        4 => ColorSpace::Cmyk,
                        _ => ColorSpace::Rgb,
                    };
                }
                Some(Object::Name(n)) if n == b"Pattern" => return ColorSpace::Pattern,
                _ => {}
            }
        }
        ColorSpace::Rgb
    }

    fn ext_g_state(&self, resources: Option<&Dictionary>, name: &[u8], state: &mut GraphicsState) {
        let document = self.document;
        let Some(entry) = resources
            .and_then(|r| get_dict(document, r, b"ExtGState"))
            .and_then(|states| get_dict(document, states, name))
        else {
            return;
        };
        if let Some(alpha) = get_number(document, entry, b"ca") {
            state.fill_alpha = alpha;
        }
        if let Some(alpha) = get_number(document, entry, b"CA") {
            state.stroke_alpha = alpha;
        }
        if let Some(width) = get_number(document, entry, b"LW") {
            state.line_width = width;
        }
    }

    // ---------------------------------------------------------------- text

    fn show(
        &self,
        page: &mut PageRun,
        data: &[u8],
        mut text_matrix: Matrix,
        state: &GraphicsState,
    ) -> Matrix {
        let Some(font) = state.font.clone() else {
            page.content
                .problems
                .push("text shown without a font".into());
            return text_matrix;
        };
        let (size, scaling) = (state.font_size, state.horizontal_scaling);
        for code in font.split_codes(data) {
            let page_matrix = multiply(&text_matrix, &state.ctm);
            let rendering = multiply(
                &[size * scaling, 0.0, 0.0, size, 0.0, state.rise],
                &page_matrix,
            );
            let rendering = multiply(&rendering, &page.origin_shift);
            let vertical = rendering[2].hypot(rendering[3]);
            let horizontal = rendering[0].hypot(rendering[1]);
            let mut advance_text = font.width(code) / 1000.0 * size + state.char_spacing;
            if code == 32 && font.code_bytes == 1 {
                advance_text += state.word_spacing;
            }
            advance_text *= scaling;
            page.sequence += 1;
            page.content.glyphs.push(Glyph {
                page: page.content.index,
                font: font.clone(),
                code,
                text: font.text(code),
                x: rendering[4],
                y: rendering[5],
                size: vertical,
                horizontal_ratio: if vertical != 0.0 {
                    horizontal / vertical
                } else {
                    1.0
                },
                angle: rendering[1].atan2(rendering[0]).to_degrees(),
                advance: advance_text * page_matrix[0].hypot(page_matrix[1]),
                color: state.fill_color,
                render_mode: state.render_mode,
                sequence: page.sequence,
            });
            text_matrix = multiply(&translation(advance_text, 0.0), &text_matrix);
        }
        text_matrix
    }

    // ---------------------------------------------------------------- paths

    fn record_path(
        &self,
        page: &mut PageRun,
        mut segments: Vec<Segment>,
        op: &str,
        state: &GraphicsState,
        path_raw: &[u8],
    ) {
        let fill = matches!(op, "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
        let stroke = matches!(op, "S" | "s" | "B" | "B*" | "b" | "b*");
        if matches!(op, "s" | "b" | "b*") {
            segments.push(Segment::Close);
        }
        let mut body = path_raw.to_vec();
        body.extend_from_slice(format!(" {op}\n").as_bytes());
        let raw = raw_item(state, &page.origin_shift, &body);
        page.sequence += 1;
        page.content.paths.push(PathItem {
            page: page.content.index,
            segments,
            fill,
            stroke,
            even_odd: matches!(op, "f*" | "B*" | "b*"),
            fill_color: fill.then_some(state.fill_color),
            stroke_color: stroke.then_some(state.stroke_color),
            line_width: state.line_width * scale_of(&state.ctm),
            dashed: state.dashed,
            alpha: (if fill { state.fill_alpha } else { 1.0 }).min(if stroke {
                state.stroke_alpha
            } else {
                1.0
            }),
            clip: state.clip.clone(),
            pattern: (fill && state.fill_pattern) || (stroke && state.stroke_pattern),
            raw,
            sequence: page.sequence,
        });
    }

    fn record_shading(&self, page: &mut PageRun, operation: &Operation, state: &GraphicsState) {
        let mut body = Vec::new();
        if let Some(name) = operation.operands.first().and_then(Operand::name) {
            body.extend_from_slice(b"/");
            body.extend_from_slice(name);
            body.extend_from_slice(b" sh\n");
        }
        let raw = raw_item(state, &page.origin_shift, &body);
        let (width, height) = (page.content.width, page.content.height);
        let page_box = vec![Segment::Rect([
            (0.0, 0.0),
            (width, 0.0),
            (width, height),
            (0.0, height),
        ])];
        let segments = state.clip.last().cloned().unwrap_or(page_box);
        page.sequence += 1;
        page.content.shadings.push(PathItem {
            page: page.content.index,
            segments,
            fill: true,
            stroke: false,
            even_odd: false,
            fill_color: None,
            stroke_color: None,
            line_width: 0.0,
            dashed: false,
            alpha: 1.0,
            clip: Vec::new(),
            pattern: true,
            raw,
            sequence: page.sequence,
        });
    }

    // ---------------------------------------------------------------- xobjects and pictures

    fn do_xobject(
        &mut self,
        page: &mut PageRun,
        name: &[u8],
        resources: Option<&Dictionary>,
        state: &GraphicsState,
        depth: usize,
    ) {
        let document = self.document;
        let Some(xobjects) = resources.and_then(|r| get_dict(document, r, b"XObject")) else {
            return;
        };
        let Some(Object::Stream(xobject)) = get(document, xobjects, name) else {
            return;
        };
        match get(document, &xobject.dict, b"Subtype") {
            Some(Object::Name(subtype)) if subtype == b"Image" => {
                let matrix = multiply(&state.ctm, &page.origin_shift);
                match decode_image(document, xobject) {
                    Ok(picture) => {
                        page.sequence += 1;
                        let sequence = page.sequence;
                        page.content.images.push(ImageItem {
                            page: page.content.index,
                            matrix,
                            picture,
                            sequence,
                        });
                    }
                    // a picture that can't be decoded is reported, not guessed
                    Err(reason) => page
                        .content
                        .problems
                        .push(format!("picture skipped: {reason}")),
                }
            }
            Some(Object::Name(subtype)) if subtype == b"Form" => {
                let mut inner = state.clone();
                let matrix = get(document, &xobject.dict, b"Matrix")
                    .and_then(|m| match m {
                        Object::Array(items) => Some(numbers(document, items)),
                        _ => None,
                    })
                    .filter(|values| values.len() == 6)
                    .map(|v| [v[0], v[1], v[2], v[3], v[4], v[5]])
                    .unwrap_or(IDENTITY);
                inner.ctm = multiply(&matrix, &state.ctm);
                let form_resources = get_dict(document, &xobject.dict, b"Resources").or(resources);
                match stream_bytes(xobject) {
                    Ok(bytes) => self.run(page, &bytes, form_resources, inner, depth + 1),
                    Err(reason) => page
                        .content
                        .problems
                        .push(format!("form skipped: {reason}")),
                }
            }
            _ => {}
        }
    }

    fn record_inline_image(
        &self,
        page: &mut PageRun,
        operation: &Operation,
        state: &GraphicsState,
    ) {
        let dictionary = operation.operands.first().cloned().unwrap_or(Operand::Null);
        let data = operation.inline_data.clone().unwrap_or_default();
        match inline_image(&dictionary, &data) {
            Ok(picture) => {
                page.sequence += 1;
                let sequence = page.sequence;
                let matrix = multiply(&state.ctm, &page.origin_shift);
                page.content.images.push(ImageItem {
                    page: page.content.index,
                    matrix,
                    picture,
                    sequence,
                });
            }
            Err(reason) => page
                .content
                .problems
                .push(format!("picture skipped: {reason}")),
        }
    }
}

fn with_newline(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.push(b'\n');
    bytes
}

/// a path's operators turned into page-space segments.
fn page_segments(path: &[(String, Vec<f64>)], to_page: &Matrix) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current = (0.0, 0.0);
    let mut start = (0.0, 0.0);
    let at = |p: Point| apply(to_page, p.0, p.1);
    for (op, v) in path {
        match (op.as_str(), v.as_slice()) {
            ("re", [x, y, w, h]) => {
                let corners = [(*x, *y), (x + w, *y), (x + w, y + h), (*x, y + h)];
                segments.push(Segment::Rect(corners.map(at)));
                current = (*x, *y);
                start = current;
            }
            ("m", [x, y]) => {
                current = (*x, *y);
                start = current;
                segments.push(Segment::Move(at(current)));
            }
            ("l", [x, y]) => {
                current = (*x, *y);
                segments.push(Segment::Line(at(current)));
            }
            ("c", [x1, y1, x2, y2, x3, y3]) => {
                segments.push(Segment::Curve(
                    at((*x1, *y1)),
                    at((*x2, *y2)),
                    at((*x3, *y3)),
                ));
                current = (*x3, *y3);
            }
            ("v", [x2, y2, x3, y3]) => {
                segments.push(Segment::Curve(at(current), at((*x2, *y2)), at((*x3, *y3))));
                current = (*x3, *y3);
            }
            ("y", [x1, y1, x3, y3]) => {
                segments.push(Segment::Curve(
                    at((*x1, *y1)),
                    at((*x3, *y3)),
                    at((*x3, *y3)),
                ));
                current = (*x3, *y3);
            }
            ("h", _) => {
                current = start;
                segments.push(Segment::Close);
            }
            _ => {}
        }
    }
    segments
}

fn matrix_bytes(m: &Matrix) -> Vec<u8> {
    m.iter()
        .map(|v| format!("{v:.6}"))
        .collect::<Vec<_>>()
        .join(" ")
        .into_bytes()
}

/// a path written back as operators in page space.
pub fn segments_bytes(segments: &[Segment]) -> Vec<u8> {
    let mut parts = Vec::new();
    for segment in segments {
        match segment {
            Segment::Rect([(x0, y0), (x1, _), _, (_, y3)]) => parts.push(format!(
                "{x0:.4} {y0:.4} m {x1:.4} {y0:.4} l {x1:.4} {y3:.4} l {x0:.4} {y3:.4} l h"
            )),
            Segment::Move((x, y)) => parts.push(format!("{x:.4} {y:.4} m")),
            Segment::Line((x, y)) => parts.push(format!("{x:.4} {y:.4} l")),
            Segment::Curve(a, b, c) => parts.push(format!(
                "{:.4} {:.4} {:.4} {:.4} {:.4} {:.4} c",
                a.0, a.1, b.0, b.1, c.0, c.1
            )),
            Segment::Close => parts.push("h".into()),
        }
    }
    parts.join(" ").into_bytes()
}

/// operators that redraw one item on its own: clips, then the item's
/// matrix, colours and states, then the item. clips are replayed in page
/// space, so each keeps working after the item's `cm`.
fn raw_item(state: &GraphicsState, origin_shift: &Matrix, body: &[u8]) -> Vec<u8> {
    let mut raw = b"q\n".to_vec();
    for clip in &state.clip {
        raw.extend(segments_bytes(clip));
        raw.extend_from_slice(b" W n\n");
    }
    raw.extend(matrix_bytes(&multiply(&state.ctm, origin_shift)));
    raw.extend_from_slice(b" cm\n");
    for bytes in &state.raw_state {
        raw.extend_from_slice(bytes);
    }
    raw.extend_from_slice(body);
    raw.extend_from_slice(b"Q\n");
    raw
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangles_and_curves_become_page_segments() {
        let path = vec![
            ("re".to_string(), vec![10.0, 10.0, 50.0, 10.0]),
            ("m".to_string(), vec![0.0, 0.0]),
            ("v".to_string(), vec![1.0, 1.0, 2.0, 0.0]),
            ("h".to_string(), vec![]),
        ];
        let segments = page_segments(&path, &translation(100.0, 0.0));
        assert_eq!(
            segments[0],
            Segment::Rect([(110.0, 10.0), (160.0, 10.0), (160.0, 20.0), (110.0, 20.0)])
        );
        assert_eq!(
            segments[2],
            Segment::Curve((100.0, 0.0), (101.0, 1.0), (102.0, 0.0))
        );
        assert_eq!(segments[3], Segment::Close);
    }

    #[test]
    fn colours_by_component_count() {
        assert_eq!(to_rgb(&[0.5]), [0.5; 3]);
        assert_eq!(to_rgb(&[0.0, 0.0, 0.0, 1.0]), [0.0; 3]);
        assert_eq!(to_rgb(&[1.0, 0.0, 0.0, 0.0]), [0.0, 1.0, 1.0]);
    }
}
