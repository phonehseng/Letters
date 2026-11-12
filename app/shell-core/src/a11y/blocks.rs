//! where the tagged blocks of a page are, for the reading order step of
//! the fix wizard: the page drawn with a numbered box on each block.
//!
//! the engine proposes an order as a list of tags (`auto_reading_order`)
//! but doesn't say where each tag sits on the page, so this file works it
//! out: it follows each tag down to its marked content (the mcids on the
//! page), then walks the page's drawing instructions and gathers what each
//! mcid draws. the boxes are close, not exact: text widths are estimated
//! from the number of letters, because the fonts' widths are not read.

use std::collections::{HashMap, HashSet};

use letters_a11y::{ObjectRef, Rect};
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;

/// how deep a chain of tags is followed; real documents are far shallower.
const MAX_TAG_DEPTH: usize = 40;

/// one block of a page as the wizard shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingBlock {
    pub element: ObjectRef,
    /// the tag's kind as written in the file ("P", "H1", "Figure")
    pub role: String,
    /// where it is, in points on the visible page from its top left; none
    /// when nothing it draws could be found
    pub rect: Option<Rect>,
    /// the start of its words, for the list
    pub text: String,
}

/// the blocks named in `order`, in that order, with their places on `page`.
pub fn reading_blocks(doc: &Document, page: u32, order: &[ObjectRef]) -> Vec<ReadingBlock> {
    let Some(&page_id) = doc.get_pages().get(&(page + 1)) else {
        return Vec::new();
    };
    let drawn = marked_content(doc, page_id);
    order
        .iter()
        .map(|&element| {
            let mut mcids = Vec::new();
            collect_mcids(
                doc,
                element.id(),
                page_id,
                None,
                &mut mcids,
                &mut HashSet::new(),
                0,
            );
            let mut rect: Option<Rect> = None;
            let mut text = String::new();
            for mcid in mcids {
                if let Some(found) = drawn.get(&mcid) {
                    if let Some(drawn_rect) = found.rect {
                        rect = Some(rect.map_or(drawn_rect, |known| known.union(&drawn_rect)));
                    }
                    push_words(&mut text, &found.text);
                }
            }
            ReadingBlock {
                element,
                role: role_of(doc, element.id()),
                rect,
                text: shorten(&text, 80),
            }
        })
        .collect()
}

fn role_of(doc: &Document, element: ObjectId) -> String {
    doc.get_dictionary(element)
        .ok()
        .and_then(|dict| dict.get(b"S").ok())
        .and_then(|name| name.as_name().ok())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .unwrap_or_default()
}

/// the mcids an element (and the elements inside it) own on one page.
fn collect_mcids(
    doc: &Document,
    element: ObjectId,
    page: ObjectId,
    inherited_page: Option<ObjectId>,
    out: &mut Vec<i64>,
    seen: &mut HashSet<ObjectId>,
    depth: usize,
) {
    if depth > MAX_TAG_DEPTH || !seen.insert(element) {
        return;
    }
    let Ok(dict) = doc.get_dictionary(element) else {
        return;
    };
    let own_page = dict
        .get(b"Pg")
        .and_then(Object::as_reference)
        .ok()
        .or(inherited_page);
    if let Ok(kids) = dict.get(b"K") {
        collect_kid(doc, kids, page, own_page, out, seen, depth);
    }
}

fn collect_kid(
    doc: &Document,
    kid: &Object,
    page: ObjectId,
    element_page: Option<ObjectId>,
    out: &mut Vec<i64>,
    seen: &mut HashSet<ObjectId>,
    depth: usize,
) {
    match kid {
        Object::Integer(mcid) if element_page == Some(page) => out.push(*mcid),
        Object::Array(items) => {
            for item in items {
                collect_kid(doc, item, page, element_page, out, seen, depth + 1);
            }
        }
        Object::Dictionary(dict) => collect_reference(dict, page, element_page, out),
        Object::Reference(id) => match doc.get_dictionary(*id) {
            // a marked content reference stored on its own, or a child tag
            Ok(dict) if dict.has(b"MCID") => collect_reference(dict, page, element_page, out),
            Ok(dict) if dict.has(b"S") => {
                collect_mcids(doc, *id, page, element_page, out, seen, depth + 1)
            }
            _ => {}
        },
        _ => {}
    }
}

/// a `/Type /MCR` dictionary: an mcid, on its own page or the element's.
fn collect_reference(
    dict: &Dictionary,
    page: ObjectId,
    element_page: Option<ObjectId>,
    out: &mut Vec<i64>,
) {
    let on_page = dict
        .get(b"Pg")
        .and_then(Object::as_reference)
        .ok()
        .or(element_page);
    if let (Some(on_page), Ok(mcid)) = (on_page, dict.get(b"MCID").and_then(Object::as_i64)) {
        if on_page == page {
            out.push(mcid);
        }
    }
}

/// what one piece of marked content draws.
#[derive(Default)]
struct Drawn {
    rect: Option<Rect>,
    text: String,
}

/// the state of the drawing instructions that matters for finding places.
#[derive(Clone, Copy)]
struct Graphics {
    ctm: [f32; 6],
}

struct TextState {
    matrix: [f32; 6],
    line: [f32; 6],
    size: f32,
    leading: f32,
}

/// an average letter is about half as wide as the font size; the fonts'
/// own widths are not read, so text boxes are an estimate
const AVERAGE_LETTER_WIDTH: f32 = 0.5;
/// how far letters reach below and above the baseline, as shares of the size
const DESCENT: f32 = 0.2;
const ASCENT: f32 = 0.9;

/// walks a page's drawing instructions and gathers, per mcid, where its
/// text, pictures and filled shapes are and what its words are.
fn marked_content(doc: &Document, page: ObjectId) -> HashMap<i64, Drawn> {
    let bytes = doc.get_page_content(page);
    let Ok(content) = Content::decode(&bytes) else {
        return HashMap::new();
    };
    let mut walker = Walker::new(doc, VisibleFrame::of(doc, page));
    for operation in &content.operations {
        walker.step(&operation.operator, &operation.operands);
    }
    walker.found
}

/// the state of one walk through a page's drawing instructions.
struct Walker<'a> {
    doc: &'a Document,
    frame: VisibleFrame,
    stack: Vec<Graphics>,
    graphics: Graphics,
    text: TextState,
    /// the open marked content, innermost last; none for marks without an mcid
    marks: Vec<Option<i64>>,
    /// the rectangles of the path being built, until it is painted
    path_box: Option<Rect>,
    found: HashMap<i64, Drawn>,
}

impl<'a> Walker<'a> {
    fn new(doc: &'a Document, frame: VisibleFrame) -> Self {
        Self {
            doc,
            frame,
            stack: Vec::new(),
            graphics: Graphics { ctm: IDENTITY },
            text: TextState {
                matrix: IDENTITY,
                line: IDENTITY,
                size: 12.0,
                leading: 0.0,
            },
            marks: Vec::new(),
            path_box: None,
            found: HashMap::new(),
        }
    }

    /// the innermost open mcid, which owns what is drawn now.
    fn current_mcid(&self) -> Option<i64> {
        self.marks.iter().rev().flatten().next().copied()
    }

    fn step(&mut self, operator: &str, operands: &[Object]) {
        let number = |index: usize| operands.get(index).and_then(as_number).unwrap_or(0.0);
        let matrix = || {
            [
                number(0),
                number(1),
                number(2),
                number(3),
                number(4),
                number(5),
            ]
        };
        match operator {
            "q" => self.stack.push(self.graphics),
            "Q" => self.graphics = self.stack.pop().unwrap_or(Graphics { ctm: IDENTITY }),
            "cm" => self.graphics.ctm = multiply(&matrix(), &self.graphics.ctm),
            "BDC" => {
                let mcid = operands
                    .get(1)
                    .and_then(|properties| mcid_of(self.doc, properties));
                self.marks.push(mcid);
            }
            "BMC" => self.marks.push(None),
            "EMC" => {
                self.marks.pop();
            }
            "BT" => {
                self.text.matrix = IDENTITY;
                self.text.line = IDENTITY;
            }
            "Tf" => self.text.size = number(1),
            "TL" => self.text.leading = number(0),
            "Tm" => {
                self.text.matrix = matrix();
                self.text.line = self.text.matrix;
            }
            "Td" => self.move_line(number(0), number(1)),
            "TD" => {
                self.text.leading = -number(1);
                self.move_line(number(0), number(1));
            }
            "T*" => next_line(&mut self.text),
            "Tj" | "TJ" => self.show_text(operands),
            "'" | "\"" => {
                next_line(&mut self.text);
                self.show_text(operands);
            }
            "Do" => self.draw_object(),
            "re" => self.add_rectangle(number(0), number(1), number(2), number(3)),
            "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "S" | "s" => self.paint_path(),
            "n" => self.path_box = None,
            _ => {}
        }
    }

    fn move_line(&mut self, x: f32, y: f32) {
        self.text.line = multiply(&translation(x, y), &self.text.line);
        self.text.matrix = self.text.line;
    }

    /// text: an estimated box from the baseline, and the words read.
    fn show_text(&mut self, operands: &[Object]) {
        let words = shown_text(operands);
        let size = self.text.size;
        let width = words.chars().count() as f32 * size * AVERAGE_LETTER_WIDTH;
        let space = multiply(&self.text.matrix, &self.graphics.ctm);
        let corners = [
            (0.0, -DESCENT * size),
            (width, -DESCENT * size),
            (0.0, ASCENT * size),
            (width, ASCENT * size),
        ];
        self.text.matrix = multiply(&translation(width, 0.0), &self.text.matrix);
        if let Some(mcid) = self.current_mcid() {
            let rect = self.frame.rect_of(&space, &corners);
            let entry = self.found.entry(mcid).or_default();
            add_box(entry, rect);
            push_words(&mut entry.text, &words);
        }
    }

    /// a picture or form fills the unit square of the current matrix.
    fn draw_object(&mut self) {
        let Some(mcid) = self.current_mcid() else {
            return;
        };
        let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)];
        let rect = self.frame.rect_of(&self.graphics.ctm, &corners);
        add_box(self.found.entry(mcid).or_default(), rect);
    }

    fn add_rectangle(&mut self, x: f32, y: f32, width: f32, height: f32) {
        let corners = [
            (x, y),
            (x + width, y),
            (x, y + height),
            (x + width, y + height),
        ];
        let rect = self.frame.rect_of(&self.graphics.ctm, &corners);
        self.path_box = Some(self.path_box.map_or(rect, |known| known.union(&rect)));
    }

    /// a painted path belongs to the open mcid; a path only used to clip doesn't.
    fn paint_path(&mut self) {
        let painted = self.path_box.take();
        if let (Some(mcid), Some(rect)) = (self.current_mcid(), painted) {
            add_box(self.found.entry(mcid).or_default(), rect);
        }
    }
}

fn next_line(text: &mut TextState) {
    text.line = multiply(&translation(0.0, -text.leading), &text.line);
    text.matrix = text.line;
}

fn add_box(entry: &mut Drawn, rect: Rect) {
    entry.rect = Some(entry.rect.map_or(rect, |known| known.union(&rect)));
}

/// the mcid of a `BDC` property list written in place (as taggers write them).
fn mcid_of(doc: &Document, properties: &Object) -> Option<i64> {
    let dict = match properties {
        Object::Dictionary(dict) => dict,
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        _ => return None,
    };
    dict.get(b"MCID").and_then(Object::as_i64).ok()
}

/// the letters a text-showing instruction draws, read as latin-1, which is
/// right for the standard fonts and close enough to recognise a block.
fn shown_text(operands: &[Object]) -> String {
    let mut out = String::new();
    for operand in operands {
        match operand {
            Object::String(bytes, _) => out.extend(bytes.iter().map(|&b| b as char)),
            Object::Array(items) => {
                for item in items {
                    match item {
                        Object::String(bytes, _) => out.extend(bytes.iter().map(|&b| b as char)),
                        // a big step back in a TJ array is a space between words
                        other if as_number(other).is_some_and(|gap| gap < -200.0) => out.push(' '),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn push_words(out: &mut String, words: &str) {
    let words = words.trim();
    if words.is_empty() {
        return;
    }
    if !out.is_empty() {
        out.push(' ');
    }
    out.push_str(words);
}

fn shorten(text: &str, limit: usize) -> String {
    let clean: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() <= limit {
        return clean;
    }
    let cut: String = clean.chars().take(limit).collect();
    format!("{}…", cut.trim_end())
}

fn as_number(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn translation(x: f32, y: f32) -> [f32; 6] {
    [1.0, 0.0, 0.0, 1.0, x, y]
}

/// `first` then `second`, as pdf combines matrices.
fn multiply(first: &[f32; 6], second: &[f32; 6]) -> [f32; 6] {
    let [a, b, c, d, e, f] = *first;
    let [a2, b2, c2, d2, e2, f2] = *second;
    [
        a * a2 + b * c2,
        a * b2 + b * d2,
        c * a2 + d * c2,
        c * b2 + d * d2,
        e * a2 + f * c2 + e2,
        e * b2 + f * d2 + f2,
    ]
}

/// the visible page: its crop box and how far it is turned.
struct VisibleFrame {
    crop: [f32; 4],
    rotate: i64,
}

impl VisibleFrame {
    fn of(doc: &Document, page: ObjectId) -> Self {
        let read_box =
            |key: &[u8]| inherited(doc, page, key).and_then(|object| number_box(doc, object));
        let crop = read_box(b"CropBox")
            .or_else(|| read_box(b"MediaBox"))
            .unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let rotate = inherited(doc, page, b"Rotate")
            .and_then(|object| object.as_i64().ok())
            .unwrap_or(0)
            .rem_euclid(360);
        Self { crop, rotate }
    }

    /// a point in pdf space to the visible page, from its top left, y down.
    fn to_visible(&self, x: f32, y: f32) -> (f32, f32) {
        let [x0, y0, x1, y1] = self.crop;
        match self.rotate {
            90 => (y - y0, x - x0),
            180 => (x1 - x, y - y0),
            270 => (y1 - y, x1 - x),
            _ => (x - x0, y1 - y),
        }
    }

    /// the visible rectangle around some points of a space.
    fn rect_of(&self, space: &[f32; 6], points: &[(f32, f32)]) -> Rect {
        let placed: Vec<(f32, f32)> = points
            .iter()
            .map(|&(x, y)| {
                let [a, b, c, d, e, f] = *space;
                self.to_visible(a * x + c * y + e, b * x + d * y + f)
            })
            .collect();
        Rect::around(&placed)
    }
}

fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut node = page;
    for _ in 0..MAX_TAG_DEPTH {
        let dict = doc.get_dictionary(node).ok()?;
        if let Ok(value) = dict.get(key) {
            return Some(value);
        }
        node = dict.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

fn number_box(doc: &Document, object: &Object) -> Option<[f32; 4]> {
    let object = match object {
        Object::Reference(id) => doc.get_object(*id).ok()?,
        other => other,
    };
    let items = object.as_array().ok()?;
    let values: Vec<f32> = items.iter().filter_map(as_number).collect();
    let [a, b, c, d] = values[..] else {
        return None;
    };
    Some([a.min(c), b.min(d), a.max(c), b.max(d)])
}
