//! a small reader for page drawings: where the lines, boxes and letters are.
//!
//! a page's content is a list of drawing instructions ("move here, draw a
//! line to there, stroke it", "use this font, show these letters"). this
//! reader follows those instructions far enough to know where every line,
//! box, curve and letter ends up on the page. it does not draw anything
//! and ignores colour, clipping and images.

use std::collections::HashMap;

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

use super::cmap::{parse_to_unicode, CodeMap};
use crate::fonts::{FontMetrics, StandardFont};
use crate::geometry::{Matrix, PdfRect, Point};
use crate::objects::{
    get, get_array, get_dict, get_name, inherited_page_value, number_of, resolve,
};
use crate::text::decode_win_ansi_byte;

/// how deep form xobjects may nest before the reader stops following them.
const MAX_XOBJECT_DEPTH: usize = 6;
/// a page with more instructions than this is read only this far, so a
/// huge drawing can't stall the app.
const MAX_OPERATIONS: usize = 400_000;

/// one straight piece of a drawn path, in user space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub from: Point,
    pub to: Point,
}

/// what a drawn subpath turned out to be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    /// four straight sides at right angles
    Rectangle,
    /// any shape with curves, such as a circle
    Curved,
    /// straight lines that are not a rectangle
    Lines,
}

/// one drawn subpath.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub kind: ShapeKind,
    pub bounds: PdfRect,
    pub segments: Vec<Segment>,
    pub filled: bool,
    pub stroked: bool,
}

/// one letter on the page, with the box it takes up in user space.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub character: char,
    pub bounds: PdfRect,
    /// the height of the font as drawn, for telling lines of text apart
    pub size: f32,
}

/// everything found on one page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageMarks {
    pub shapes: Vec<Shape>,
    pub glyphs: Vec<Glyph>,
}

/// reads where everything is drawn on a page.
pub fn read_page_marks(doc: &Document, page_id: ObjectId) -> PageMarks {
    let resources = inherited_page_value(doc, page_id, b"Resources")
        .and_then(|value| value.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    let content = doc.get_page_content(page_id);
    let mut reader = Reader {
        doc,
        marks: PageMarks::default(),
        fonts: HashMap::new(),
        operations: 0,
    };
    reader.run(&content, &resources, Matrix::IDENTITY, 0);
    reader.marks
}

/// what the reader knows about a font: widths, how many bytes a letter
/// takes, and how codes turn into characters.
struct FontInfo {
    metrics: FontMetrics,
    two_byte: bool,
    /// widths of two-byte fonts (/W), by code
    cid_widths: HashMap<u32, f32>,
    default_width: f32,
    to_unicode: Option<CodeMap>,
    descent: f32,
    ascent: f32,
}

#[derive(Clone)]
struct GraphicsState {
    ctm: Matrix,
}

struct TextState {
    font: Option<std::rc::Rc<FontInfo>>,
    size: f32,
    char_spacing: f32,
    word_spacing: f32,
    horizontal_scale: f32,
    leading: f32,
    rise: f32,
    matrix: Matrix,
    line_matrix: Matrix,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            font: None,
            size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal_scale: 1.0,
            leading: 0.0,
            rise: 0.0,
            matrix: Matrix::IDENTITY,
            line_matrix: Matrix::IDENTITY,
        }
    }
}

/// a subpath while it is being built: its points in user space.
#[derive(Default)]
struct Subpath {
    points: Vec<Point>,
    curved: bool,
    closed: bool,
    from_rectangle: bool,
}

struct Reader<'a> {
    doc: &'a Document,
    marks: PageMarks,
    fonts: HashMap<ObjectId, std::rc::Rc<FontInfo>>,
    operations: usize,
}

impl Reader<'_> {
    fn run(&mut self, content: &[u8], resources: &Dictionary, base: Matrix, depth: usize) {
        let Ok(content) = Content::decode(content) else {
            return;
        };
        let mut state = GraphicsState { ctm: base };
        let mut saved: Vec<GraphicsState> = Vec::new();
        let mut text = TextState::default();
        let mut path: Vec<Subpath> = Vec::new();

        for operation in &content.operations {
            self.operations += 1;
            if self.operations > MAX_OPERATIONS {
                return;
            }
            let operands = &operation.operands;
            let numbers: Vec<f32> = operands.iter().filter_map(number_of).collect();
            let at = |x: f32, y: f32, ctm: &Matrix| ctm.apply(Point::new(x, y));
            match operation.operator.as_str() {
                "q" => saved.push(state.clone()),
                "Q" => {
                    if let Some(previous) = saved.pop() {
                        state = previous;
                    }
                }
                "cm" if numbers.len() == 6 => {
                    let m = Matrix::new(
                        numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5],
                    );
                    state.ctm = m.then(&state.ctm);
                }
                "m" if numbers.len() == 2 => {
                    path.push(Subpath {
                        points: vec![at(numbers[0], numbers[1], &state.ctm)],
                        ..Subpath::default()
                    });
                }
                "l" if numbers.len() == 2 => {
                    if let Some(current) = path.last_mut() {
                        current.points.push(at(numbers[0], numbers[1], &state.ctm));
                    }
                }
                "c" | "v" | "y" if numbers.len() >= 4 => {
                    // only the end point and the control points' extent matter here
                    if let Some(current) = path.last_mut() {
                        for pair in numbers.chunks(2) {
                            current.points.push(at(pair[0], pair[1], &state.ctm));
                        }
                        current.curved = true;
                    }
                }
                "h" => {
                    if let Some(current) = path.last_mut() {
                        current.closed = true;
                    }
                }
                "re" if numbers.len() == 4 => {
                    let (x, y, w, h) = (numbers[0], numbers[1], numbers[2], numbers[3]);
                    path.push(Subpath {
                        points: vec![
                            at(x, y, &state.ctm),
                            at(x + w, y, &state.ctm),
                            at(x + w, y + h, &state.ctm),
                            at(x, y + h, &state.ctm),
                        ],
                        curved: false,
                        closed: true,
                        from_rectangle: true,
                    });
                }
                "S" | "s" => self.paint(&mut path, false, true, operation.operator == "s"),
                "f" | "F" | "f*" => self.paint(&mut path, true, false, true),
                "B" | "B*" => self.paint(&mut path, true, true, false),
                "b" | "b*" => self.paint(&mut path, true, true, true),
                "n" => path.clear(),
                "BT" => {
                    text.matrix = Matrix::IDENTITY;
                    text.line_matrix = Matrix::IDENTITY;
                }
                "Tf" if operands.len() == 2 => {
                    text.font = operands[0]
                        .as_name()
                        .ok()
                        .and_then(|font_name| self.font(resources, font_name));
                    text.size = number_of(&operands[1]).unwrap_or(0.0);
                }
                "Tc" if numbers.len() == 1 => text.char_spacing = numbers[0],
                "Tw" if numbers.len() == 1 => text.word_spacing = numbers[0],
                "Tz" if numbers.len() == 1 => text.horizontal_scale = numbers[0] / 100.0,
                "TL" if numbers.len() == 1 => text.leading = numbers[0],
                "Ts" if numbers.len() == 1 => text.rise = numbers[0],
                "Td" if numbers.len() == 2 => move_line(&mut text, numbers[0], numbers[1]),
                "TD" if numbers.len() == 2 => {
                    text.leading = -numbers[1];
                    move_line(&mut text, numbers[0], numbers[1]);
                }
                "Tm" if numbers.len() == 6 => {
                    text.matrix = Matrix::new(
                        numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5],
                    );
                    text.line_matrix = text.matrix;
                }
                "T*" => {
                    let leading = text.leading;
                    move_line(&mut text, 0.0, -leading);
                }
                "Tj" => {
                    if let Some(Object::String(bytes, _)) = operands.first() {
                        self.show(&mut text, &state, bytes);
                    }
                }
                "'" => {
                    let leading = text.leading;
                    move_line(&mut text, 0.0, -leading);
                    if let Some(Object::String(bytes, _)) = operands.first() {
                        self.show(&mut text, &state, bytes);
                    }
                }
                "\"" if operands.len() == 3 => {
                    text.word_spacing = number_of(&operands[0]).unwrap_or(0.0);
                    text.char_spacing = number_of(&operands[1]).unwrap_or(0.0);
                    let leading = text.leading;
                    move_line(&mut text, 0.0, -leading);
                    if let Object::String(bytes, _) = &operands[2] {
                        self.show(&mut text, &state, bytes);
                    }
                }
                "TJ" => {
                    if let Some(Object::Array(items)) = operands.first() {
                        for item in items {
                            match item {
                                Object::String(bytes, _) => self.show(&mut text, &state, bytes),
                                other => {
                                    if let Some(adjust) = number_of(other) {
                                        let shift =
                                            -adjust / 1000.0 * text.size * text.horizontal_scale;
                                        text.matrix =
                                            Matrix::translate(shift, 0.0).then(&text.matrix);
                                    }
                                }
                            }
                        }
                    }
                }
                "Do" if depth < MAX_XOBJECT_DEPTH => {
                    if let Some(xobject_name) =
                        operands.first().and_then(|operand| operand.as_name().ok())
                    {
                        self.form_xobject(resources, xobject_name, &state, depth);
                    }
                }
                _ => {}
            }
        }
    }

    /// turns the current path into shapes when it is painted.
    fn paint(&mut self, path: &mut Vec<Subpath>, filled: bool, stroked: bool, close: bool) {
        for mut subpath in path.drain(..) {
            if subpath.points.len() < 2 {
                continue;
            }
            if close {
                subpath.closed = true;
            }
            let shape = shape_of(&subpath, filled, stroked);
            self.marks.shapes.push(shape);
        }
    }

    fn form_xobject(
        &mut self,
        resources: &Dictionary,
        xobject_name: &[u8],
        state: &GraphicsState,
        depth: usize,
    ) {
        let doc = self.doc;
        let Some(xobjects) = get_dict(doc, resources, b"XObject") else {
            return;
        };
        let Some(Object::Stream(stream)) = xobjects
            .get(xobject_name)
            .ok()
            .map(|value| resolve(doc, value))
        else {
            return;
        };
        if get_name(doc, &stream.dict, b"Subtype").as_deref() != Some(b"Form") {
            return;
        }
        let matrix = get_array(doc, &stream.dict, b"Matrix")
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| number_of(resolve(doc, value)))
                    .collect::<Vec<_>>()
            })
            .filter(|values| values.len() == 6)
            .map(|v| Matrix::new(v[0], v[1], v[2], v[3], v[4], v[5]))
            .unwrap_or(Matrix::IDENTITY);
        let inner_resources = get_dict(doc, &stream.dict, b"Resources")
            .cloned()
            .unwrap_or_else(|| resources.clone());
        let content = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        self.run(
            &content,
            &inner_resources,
            matrix.then(&state.ctm),
            depth + 1,
        );
    }

    /// shows a string of letters, recording each one's box.
    fn show(&mut self, text: &mut TextState, state: &GraphicsState, bytes: &[u8]) {
        let Some(font) = text.font.clone() else {
            return;
        };
        let codes: Vec<u32> = if font.two_byte {
            bytes
                .chunks(2)
                .map(|pair| {
                    pair.iter()
                        .fold(0u32, |code, byte| code << 8 | *byte as u32)
                })
                .collect()
        } else {
            bytes.iter().map(|byte| *byte as u32).collect()
        };
        for code in codes {
            let width = if font.two_byte {
                font.cid_widths
                    .get(&code)
                    .copied()
                    .unwrap_or(font.default_width)
            } else {
                font.metrics.byte_width(code as u8)
            } / 1000.0;
            let text_to_page = Matrix::new(
                text.size * text.horizontal_scale,
                0.0,
                0.0,
                text.size,
                0.0,
                text.rise,
            )
            .then(&text.matrix)
            .then(&state.ctm);
            let glyph_box = PdfRect::from_corners(0.0, font.descent, width.max(0.1), font.ascent);
            let bounds = text_to_page.apply_rect(&glyph_box);
            let character = character_of(&font, code);
            if !character.is_control() {
                self.marks.glyphs.push(Glyph {
                    character,
                    bounds,
                    size: bounds.height(),
                });
            }
            let spacing = if !font.two_byte && code == 32 {
                text.word_spacing
            } else {
                0.0
            };
            let advance = (width * text.size + text.char_spacing + spacing) * text.horizontal_scale;
            text.matrix = Matrix::translate(advance, 0.0).then(&text.matrix);
        }
    }

    /// reads a font from the resources once and keeps it for later.
    fn font(&mut self, resources: &Dictionary, font_name: &[u8]) -> Option<std::rc::Rc<FontInfo>> {
        let fonts = get_dict(self.doc, resources, b"Font")?;
        let entry = fonts.get(font_name).ok()?;
        let key = entry.as_reference().unwrap_or((0, 0));
        if key != (0, 0) {
            if let Some(info) = self.fonts.get(&key) {
                return Some(info.clone());
            }
        }
        let dict = resolve(self.doc, entry).as_dict().ok()?;
        let info = std::rc::Rc::new(font_info(self.doc, dict));
        if key != (0, 0) {
            self.fonts.insert(key, info.clone());
        }
        Some(info)
    }
}

fn move_line(text: &mut TextState, x: f32, y: f32) {
    text.line_matrix = Matrix::translate(x, y).then(&text.line_matrix);
    text.matrix = text.line_matrix;
}

fn character_of(font: &FontInfo, code: u32) -> char {
    if let Some(map) = &font.to_unicode {
        if let Some(text) = map.get(&code) {
            return text.chars().next().unwrap_or('\u{FFFD}');
        }
    }
    if font.two_byte {
        '\u{FFFD}'
    } else {
        decode_win_ansi_byte(code as u8)
    }
}

fn font_info(doc: &Document, dict: &Dictionary) -> FontInfo {
    let subtype = get_name(doc, dict, b"Subtype").unwrap_or_default();
    let to_unicode = match get(doc, dict, b"ToUnicode") {
        Some(Object::Stream(stream)) => stream
            .decompressed_content()
            .ok()
            .map(|bytes| parse_to_unicode(&bytes)),
        _ => None,
    };
    if subtype.as_slice() == b"Type0" {
        let descendant = get_array(doc, dict, b"DescendantFonts")
            .and_then(|fonts| fonts.first())
            .and_then(|font| resolve(doc, font).as_dict().ok());
        let default_width = descendant
            .and_then(|font| get(doc, font, b"DW").and_then(number_of))
            .unwrap_or(1000.0);
        let cid_widths = descendant
            .map(|font| cid_widths(doc, font))
            .unwrap_or_default();
        return FontInfo {
            metrics: FontMetrics::standard(StandardFont::Helvetica),
            two_byte: true,
            cid_widths,
            default_width,
            to_unicode,
            descent: -0.2,
            ascent: 0.8,
        };
    }
    let metrics = FontMetrics::from_font_dict(doc, dict);
    let (ascent, descent) = metrics.standard.ascent_descent();
    FontInfo {
        metrics,
        two_byte: false,
        cid_widths: HashMap::new(),
        default_width: 500.0,
        to_unicode,
        descent: descent / 1000.0,
        ascent: ascent / 1000.0,
    }
}

/// reads a cid font's /W array: [first [w1 w2 ...]] or [first last w].
fn cid_widths(doc: &Document, font: &Dictionary) -> HashMap<u32, f32> {
    let mut widths = HashMap::new();
    let Some(entries) = get_array(doc, font, b"W") else {
        return widths;
    };
    let mut index = 0;
    while index + 1 < entries.len() {
        let first = number_of(resolve(doc, &entries[index])).unwrap_or(0.0) as u32;
        match resolve(doc, &entries[index + 1]) {
            Object::Array(list) => {
                for (offset, width) in list.iter().enumerate() {
                    widths.insert(
                        first + offset as u32,
                        number_of(resolve(doc, width)).unwrap_or(0.0),
                    );
                }
                index += 2;
            }
            last => {
                let last = number_of(last).unwrap_or(0.0) as u32;
                let width = entries
                    .get(index + 2)
                    .and_then(|value| number_of(resolve(doc, value)))
                    .unwrap_or(0.0);
                for code in first..=last.min(first + 65_535) {
                    widths.insert(code, width);
                }
                index += 3;
            }
        }
    }
    widths
}

/// decides what a painted subpath is.
fn shape_of(subpath: &Subpath, filled: bool, stroked: bool) -> Shape {
    let bounds = PdfRect::around(&subpath.points);
    let mut segments: Vec<Segment> = subpath
        .points
        .windows(2)
        .map(|pair| Segment {
            from: pair[0],
            to: pair[1],
        })
        .collect();
    if subpath.closed || filled {
        if let (Some(first), Some(last)) = (subpath.points.first(), subpath.points.last()) {
            if first != last {
                segments.push(Segment {
                    from: *last,
                    to: *first,
                });
            }
        }
    }
    let kind = if subpath.curved {
        ShapeKind::Curved
    } else if subpath.from_rectangle || is_axis_rectangle(&subpath.points) {
        ShapeKind::Rectangle
    } else {
        ShapeKind::Lines
    };
    Shape {
        kind,
        bounds,
        segments,
        filled,
        stroked,
    }
}

/// four corners (a fifth may repeat the first) joined by straight sides
/// that run along the page's axes.
fn is_axis_rectangle(points: &[Point]) -> bool {
    let mut corners = points.to_vec();
    if corners.len() == 5 && near(corners[0], corners[4]) {
        corners.pop();
    }
    if corners.len() != 4 {
        return false;
    }
    (0..4).all(|index| {
        let a = corners[index];
        let b = corners[(index + 1) % 4];
        (a.x - b.x).abs() < 0.5 || (a.y - b.y).abs() < 0.5
    })
}

fn near(a: Point, b: Point) -> bool {
    (a.x - b.x).abs() < 0.5 && (a.y - b.y).abs() < 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marks_of(content: &str) -> PageMarks {
        let mut doc = Document::with_version("1.7");
        let pages = doc.new_object_id();
        let font = doc.add_object(crate::appearance::standard_font_dict(
            StandardFont::Helvetica,
        ));
        let content_id = doc.add_object(lopdf::Stream::new(
            Dictionary::new(),
            content.as_bytes().to_vec(),
        ));
        let mut fonts = Dictionary::new();
        fonts.set("F1", Object::Reference(font));
        let mut resources = Dictionary::new();
        resources.set("Font", Object::Dictionary(fonts));
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages));
        page.set("Contents", Object::Reference(content_id));
        page.set("Resources", Object::Dictionary(resources));
        let page_id = doc.add_object(Object::Dictionary(page));
        let mut pages_dict = Dictionary::new();
        pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
        pages_dict.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages_dict.set("Count", Object::Integer(1));
        doc.objects.insert(pages, Object::Dictionary(pages_dict));
        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages));
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        read_page_marks(&doc, page_id)
    }

    #[test]
    fn finds_lines_and_rectangles_through_the_matrix() {
        let marks = marks_of("q 2 0 0 2 10 10 cm 0 0 m 50 0 l S 5 5 10 10 re f Q");
        assert_eq!(marks.shapes.len(), 2);
        let line = &marks.shapes[0];
        assert_eq!(line.kind, ShapeKind::Lines);
        assert_eq!(line.segments[0].to, Point::new(110.0, 10.0));
        let square = &marks.shapes[1];
        assert_eq!(square.kind, ShapeKind::Rectangle);
        assert_eq!(square.bounds, PdfRect::from_corners(20.0, 20.0, 40.0, 40.0));
    }

    #[test]
    fn places_letters_where_they_are_drawn() {
        let marks = marks_of("BT /F1 10 Tf 100 700 Td (Hi) Tj ET");
        let text: String = marks.glyphs.iter().map(|glyph| glyph.character).collect();
        assert_eq!(text, "Hi");
        // H is 7.22 points wide at size 10, so i starts there
        assert!((marks.glyphs[1].bounds.left - 107.22).abs() < 0.01);
        assert!((marks.glyphs[0].bounds.bottom - (700.0 - 2.07)).abs() < 0.01);
    }

    #[test]
    fn follows_text_moves_and_kerning() {
        let marks = marks_of("BT /F1 10 Tf 14 TL 100 700 Td (a) Tj T* [(b) -1000 (c)] TJ ET");
        assert_eq!(marks.glyphs[1].bounds.left, 100.0);
        assert!((marks.glyphs[1].bounds.bottom - (686.0 - 2.07)).abs() < 0.01);
        // b is 5.56 wide and the -1000 adds a full 10 points
        assert!((marks.glyphs[2].bounds.left - 115.56).abs() < 0.01);
    }

    #[test]
    fn curves_make_curved_shapes() {
        let marks = marks_of("10 0 m 10 5 5 10 0 10 c -5 10 -10 5 -10 0 c h S");
        assert_eq!(marks.shapes[0].kind, ShapeKind::Curved);
    }
}
