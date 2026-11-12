//! walks a page's drawing instructions and notes where each letter lands.
//!
//! a content stream is a little program: "move here, pick this font, draw
//! these letters". this follows just the parts that decide where text goes
//! and what it looks like (the transformation matrices, the text state,
//! the fill colour, and forms drawn inside the page) and ignores the rest.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId};

use super::fonts::Font;
use super::page_frame::{number, PageFrame};
use crate::geometry::{round_tenth, Matrix};
use crate::source::{Colour, Glyph};

/// forms may draw other forms; real files nest a few deep. the limit stops
/// a form that draws itself from looping forever.
const MAX_FORM_DEPTH: usize = 12;

/// the most saved graphics states. a file that saves more without restoring
/// is damaged, and the extra saves are ignored.
const MAX_SAVED_STATES: usize = 64;

/// the most letters read from one page, so a hostile file can't use
/// unbounded memory. a dense page of small text has about 10,000.
const MAX_GLYPHS_PER_PAGE: usize = 200_000;

/// fonts already read, shared by every page of a document.
pub(crate) type FontCache = Mutex<HashMap<ObjectId, Arc<Font>>>;

/// what text drawing needs from the graphics state.
#[derive(Clone)]
struct State {
    ctm: Matrix,
    fill: Colour,
    stroke: Colour,
    fill_components: usize,
    stroke_components: usize,
    font: Option<Arc<Font>>,
    font_size: f32,
    char_spacing: f32,
    word_spacing: f32,
    /// horizontal scaling, 1.0 for 100%
    scale: f32,
    leading: f32,
    rise: f32,
    render_mode: i64,
}

impl State {
    fn new(ctm: Matrix) -> State {
        State {
            ctm,
            fill: Colour::BLACK,
            stroke: Colour::BLACK,
            fill_components: 1,
            stroke_components: 1,
            font: None,
            font_size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            scale: 1.0,
            leading: 0.0,
            rise: 0.0,
            render_mode: 0,
        }
    }
}

/// reads the letters of one page.
pub(crate) struct Interpreter<'a> {
    document: &'a Document,
    frame: PageFrame,
    fonts: &'a FontCache,
    glyphs: Vec<Glyph>,
    state: State,
    saved: Vec<State>,
    text_matrix: Matrix,
    line_matrix: Matrix,
    /// forms being drawn right now, to notice a form that draws itself
    open_forms: Vec<ObjectId>,
}

impl<'a> Interpreter<'a> {
    pub fn new(document: &'a Document, frame: PageFrame, fonts: &'a FontCache) -> Self {
        Self {
            document,
            frame,
            fonts,
            glyphs: Vec::new(),
            state: State::new(Matrix::IDENTITY),
            saved: Vec::new(),
            text_matrix: Matrix::IDENTITY,
            line_matrix: Matrix::IDENTITY,
            open_forms: Vec::new(),
        }
    }

    /// runs a content stream and returns every letter it drew.
    pub fn read_page(mut self, content: &[u8], resources: Option<&'a Dictionary>) -> Vec<Glyph> {
        self.run(content, resources);
        self.glyphs
    }

    fn run(&mut self, content: &[u8], resources: Option<&'a Dictionary>) {
        // a damaged stream still gives the instructions read before the damage
        let Ok(content) = Content::decode(content) else {
            return;
        };
        for operation in &content.operations {
            if self.glyphs.len() >= MAX_GLYPHS_PER_PAGE {
                return;
            }
            self.apply(operation, resources);
        }
    }

    fn apply(&mut self, operation: &Operation, resources: Option<&'a Dictionary>) {
        let operands = &operation.operands;
        let numbers: Vec<f32> = operands.iter().filter_map(number).collect();
        match operation.operator.as_str() {
            "q" => {
                if self.saved.len() < MAX_SAVED_STATES {
                    self.saved.push(self.state.clone());
                }
            }
            "Q" => {
                if let Some(state) = self.saved.pop() {
                    self.state = state;
                }
            }
            "cm" => {
                if let Some(matrix) = matrix_from(&numbers) {
                    self.state.ctm = matrix.then(&self.state.ctm);
                }
            }
            "BT" => {
                self.text_matrix = Matrix::IDENTITY;
                self.line_matrix = Matrix::IDENTITY;
            }
            "Tf" => self.set_font(operands, resources),
            "Tc" => self.state.char_spacing = first(&numbers),
            "Tw" => self.state.word_spacing = first(&numbers),
            "Tz" => self.state.scale = first(&numbers) / 100.0,
            "TL" => self.state.leading = first(&numbers),
            "Ts" => self.state.rise = first(&numbers),
            "Tr" => self.state.render_mode = first(&numbers) as i64,
            "Td" => self.move_line(&numbers),
            "TD" => {
                if numbers.len() == 2 {
                    self.state.leading = -numbers[1];
                }
                self.move_line(&numbers);
            }
            "Tm" => {
                if let Some(matrix) = matrix_from(&numbers) {
                    self.text_matrix = matrix;
                    self.line_matrix = matrix;
                }
            }
            "T*" => self.next_line(),
            "Tj" => self.show_operand(operands.first()),
            "'" => {
                self.next_line();
                self.show_operand(operands.first());
            }
            "\"" => {
                if numbers.len() >= 2 {
                    self.state.word_spacing = numbers[0];
                    self.state.char_spacing = numbers[1];
                }
                self.next_line();
                self.show_operand(operands.get(2));
            }
            "TJ" => self.show_array(operands.first()),
            "Do" => self.draw_object(operands.first(), resources),
            _ => self.apply_colour(operation.operator.as_str(), operands, &numbers, resources),
        }
    }

    fn move_line(&mut self, numbers: &[f32]) {
        if numbers.len() == 2 {
            self.line_matrix = Matrix::translation(numbers[0], numbers[1]).then(&self.line_matrix);
            self.text_matrix = self.line_matrix;
        }
    }

    fn next_line(&mut self) {
        self.move_line(&[0.0, -self.state.leading]);
    }

    fn set_font(&mut self, operands: &[Object], resources: Option<&'a Dictionary>) {
        if let Some(size) = operands.get(1).and_then(number) {
            self.state.font_size = size;
        }
        let Some(name) = operands.first().and_then(|name| name.as_name().ok()) else {
            return;
        };
        self.state.font = self.load_font(name, resources);
    }

    fn load_font(&self, name: &[u8], resources: Option<&'a Dictionary>) -> Option<Arc<Font>> {
        let fonts = self.resource_dictionary(resources, b"Font")?;
        let entry = fonts.get(name).ok()?;
        match entry {
            Object::Reference(id) => {
                let mut cache = self
                    .fonts
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                if let Some(font) = cache.get(id) {
                    return Some(font.clone());
                }
                let dict = self.document.get_dictionary(*id).ok()?;
                let font = Arc::new(Font::load(self.document, dict));
                cache.insert(*id, font.clone());
                Some(font)
            }
            Object::Dictionary(dict) => Some(Arc::new(Font::load(self.document, dict))),
            _ => None,
        }
    }

    /// one kind of resource (fonts, forms, colour spaces) from a resource dictionary.
    fn resource_dictionary(
        &self,
        resources: Option<&'a Dictionary>,
        kind: &[u8],
    ) -> Option<&'a Dictionary> {
        let document: &'a Document = self.document;
        let (_, object) = document.dereference(resources?.get(kind).ok()?).ok()?;
        object.as_dict().ok()
    }

    fn show_operand(&mut self, operand: Option<&Object>) {
        if let Some(Object::String(bytes, _)) = operand {
            let bytes = bytes.clone();
            self.show_string(&bytes);
        }
    }

    fn show_array(&mut self, operand: Option<&Object>) {
        let Some(Object::Array(items)) = operand else {
            return;
        };
        for item in items {
            match item {
                Object::String(bytes, _) => {
                    let bytes = bytes.clone();
                    self.show_string(&bytes);
                }
                other => {
                    if let Some(adjustment) = number(other) {
                        // a number in TJ moves the pen back by thousandths of the size
                        let shift = -adjustment / 1000.0 * self.state.font_size * self.state.scale;
                        self.text_matrix = Matrix::translation(shift, 0.0).then(&self.text_matrix);
                    }
                }
            }
        }
    }

    /// places every code of a string and moves the pen past it.
    fn show_string(&mut self, bytes: &[u8]) {
        let Some(font) = self.state.font.clone() else {
            return;
        };
        let size = self.state.font_size;
        // text space to user space, then the size, scaling and rise
        let font_matrix = Matrix::new(
            size * self.state.scale,
            0.0,
            0.0,
            size,
            0.0,
            self.state.rise,
        );
        for (code, length) in font.codes(bytes) {
            let text = font.text(code, length);
            let advance = font.advance(code, &text);
            let to_user = font_matrix.then(&self.text_matrix).then(&self.state.ctm);
            self.place_letters(&font, &text, advance, &to_user);
            let word_spacing = if font.is_word_space(code, length) {
                self.state.word_spacing
            } else {
                0.0
            };
            let pen = (advance * size + self.state.char_spacing + word_spacing) * self.state.scale;
            self.text_matrix = Matrix::translation(pen, 0.0).then(&self.text_matrix);
        }
    }

    /// adds one glyph per letter of a code. a ligature shares its width
    /// equally between its letters, which is close enough to find words.
    fn place_letters(&mut self, font: &Font, text: &str, advance: f32, to_user: &Matrix) {
        let letters: Vec<char> = text.chars().filter(|ch| !ch.is_control()).collect();
        if letters.is_empty() {
            return;
        }
        let share = advance / letters.len() as f32;
        let bottom = font.descent / 1000.0;
        let top = font.ascent / 1000.0;
        let size = round_tenth(to_user.vertical_scale());
        let colour = match self.state.render_mode {
            1 | 5 => self.state.stroke,
            _ => self.state.fill,
        };
        // fill and stroke together is the usual way to fake bold
        let fake_bold = matches!(self.state.render_mode, 2 | 6);
        for (index, ch) in letters.into_iter().enumerate() {
            let left = share * index as f32;
            let right = left + share.max(0.0);
            let corners = [
                to_user.apply(left, bottom),
                to_user.apply(right, bottom),
                to_user.apply(right, top),
                to_user.apply(left, top),
            ];
            let rect = self.frame.bounds_on_visible(&corners);
            if !self.is_on_page(rect.center_x(), rect.center_y()) {
                continue;
            }
            self.glyphs.push(Glyph {
                ch,
                rect,
                font_name: font.name.clone(),
                font_size: size,
                colour,
                bold: font.bold || fake_bold,
                italic: font.italic,
            });
        }
    }

    fn is_on_page(&self, x: f32, y: f32) -> bool {
        let size = self.frame.visible_size();
        x >= 0.0 && y >= 0.0 && x <= size.width && y <= size.height
    }

    /// draws a form xobject: a little content stream of its own, with its
    /// own matrix and (often) its own resources. pictures are skipped.
    fn draw_object(&mut self, operand: Option<&Object>, resources: Option<&'a Dictionary>) {
        let Some(name) = operand.and_then(|name| name.as_name().ok()) else {
            return;
        };
        let Some(objects) = self.resource_dictionary(resources, b"XObject") else {
            return;
        };
        let Ok(Object::Reference(id)) = objects.get(name) else {
            return;
        };
        let id = *id;
        if self.open_forms.len() >= MAX_FORM_DEPTH || self.open_forms.contains(&id) {
            return;
        }
        let document: &'a Document = self.document;
        let Ok(stream) = document.get_object(id).and_then(Object::as_stream) else {
            return;
        };
        let is_form = stream
            .dict
            .get(b"Subtype")
            .and_then(Object::as_name)
            .map(|subtype| subtype == b"Form")
            .unwrap_or(false);
        if !is_form {
            return;
        }
        let Ok(content) = stream.get_plain_content() else {
            return;
        };
        let form_matrix = stream
            .dict
            .get(b"Matrix")
            .and_then(Object::as_array)
            .ok()
            .and_then(|items| matrix_from(&items.iter().filter_map(number).collect::<Vec<_>>()))
            .unwrap_or(Matrix::IDENTITY);
        // a form without resources of its own uses its page's
        let form_resources = stream
            .dict
            .get(b"Resources")
            .ok()
            .and_then(|object| document.dereference(object).ok())
            .and_then(|(_, object)| object.as_dict().ok())
            .or(resources);

        // a form runs in a saved state, as if wrapped in q and Q
        let outer_state = self.state.clone();
        let outer_text = (self.text_matrix, self.line_matrix);
        self.state.ctm = form_matrix.then(&self.state.ctm);
        self.open_forms.push(id);
        self.run(&content, form_resources);
        self.open_forms.pop();
        self.state = outer_state;
        (self.text_matrix, self.line_matrix) = outer_text;
    }

    /// keeps track of the fill and stroke colours. colours in other spaces
    /// (patterns, spot colours) are read as well as they can be from the
    /// number of components given.
    fn apply_colour(
        &mut self,
        operator: &str,
        operands: &[Object],
        numbers: &[f32],
        resources: Option<&'a Dictionary>,
    ) {
        match operator {
            "g" => self.set_fill(numbers, 1),
            "G" => self.set_stroke(numbers, 1),
            "rg" => self.set_fill(numbers, 3),
            "RG" => self.set_stroke(numbers, 3),
            "k" => self.set_fill(numbers, 4),
            "K" => self.set_stroke(numbers, 4),
            "cs" => {
                self.state.fill_components = self.colour_space_components(operands, resources);
                self.state.fill = Colour::BLACK;
            }
            "CS" => {
                self.state.stroke_components = self.colour_space_components(operands, resources);
                self.state.stroke = Colour::BLACK;
            }
            "sc" | "scn" => self.set_fill(numbers, self.state.fill_components),
            "SC" | "SCN" => self.set_stroke(numbers, self.state.stroke_components),
            _ => {}
        }
    }

    fn set_fill(&mut self, numbers: &[f32], components: usize) {
        if let Some(colour) = colour_from(numbers, components) {
            self.state.fill = colour;
        }
    }

    fn set_stroke(&mut self, numbers: &[f32], components: usize) {
        if let Some(colour) = colour_from(numbers, components) {
            self.state.stroke = colour;
        }
    }

    /// how many numbers a colour in the named colour space has.
    fn colour_space_components(
        &self,
        operands: &[Object],
        resources: Option<&'a Dictionary>,
    ) -> usize {
        let Some(name) = operands.first().and_then(|name| name.as_name().ok()) else {
            return 1;
        };
        match name {
            b"DeviceGray" | b"CalGray" | b"G" => 1,
            b"DeviceRGB" | b"CalRGB" | b"RGB" | b"Lab" => 3,
            b"DeviceCMYK" | b"CMYK" => 4,
            other => self
                .resource_dictionary(resources, b"ColorSpace")
                .and_then(|spaces| spaces.get(other).ok())
                .map(|space| self.components_of(space))
                .unwrap_or(1),
        }
    }

    fn components_of(&self, space: &Object) -> usize {
        let Ok((_, space)) = self.document.dereference(space) else {
            return 1;
        };
        let Ok(parts) = space.as_array() else {
            return 1;
        };
        let family = parts
            .first()
            .and_then(|name| name.as_name().ok())
            .unwrap_or(b"");
        match family {
            b"ICCBased" => parts
                .get(1)
                .and_then(|stream| self.document.dereference(stream).ok())
                .and_then(|(_, stream)| stream.as_stream().ok())
                .and_then(|stream| stream.dict.get(b"N").ok())
                .and_then(|count| count.as_i64().ok())
                .map(|count| count.clamp(1, 4) as usize)
                .unwrap_or(3),
            b"CalRGB" | b"Lab" => 3,
            // separation and other spot colours have one tint value: read it as grey
            _ => 1,
        }
    }
}

fn first(numbers: &[f32]) -> f32 {
    numbers.first().copied().unwrap_or(0.0)
}

fn matrix_from(numbers: &[f32]) -> Option<Matrix> {
    (numbers.len() == 6).then(|| {
        Matrix::new(
            numbers[0], numbers[1], numbers[2], numbers[3], numbers[4], numbers[5],
        )
    })
}

/// a colour from grey, rgb or cmyk numbers.
fn colour_from(numbers: &[f32], components: usize) -> Option<Colour> {
    match (components, numbers) {
        (1, [grey, ..]) => Some(Colour::from_unit(*grey, *grey, *grey)),
        (3, [r, g, b, ..]) => Some(Colour::from_unit(*r, *g, *b)),
        (4, [c, m, y, k, ..]) => Some(Colour::from_unit(
            (1.0 - c) * (1.0 - k),
            (1.0 - m) * (1.0 - k),
            (1.0 - y) * (1.0 - k),
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmyk_black_and_rgb_red_are_read() {
        assert_eq!(colour_from(&[0.0, 0.0, 0.0, 1.0], 4), Some(Colour::BLACK));
        assert_eq!(
            colour_from(&[1.0, 0.0, 0.0], 3),
            Some(Colour::new(255, 0, 0))
        );
        assert_eq!(colour_from(&[0.5], 1), Some(Colour::new(128, 128, 128)));
        assert_eq!(colour_from(&[], 3), None);
    }
}
