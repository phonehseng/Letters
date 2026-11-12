//! drawing an annotation's appearance stream (/AP /N).
//!
//! the appearance is a small drawing stored with the annotation. viewers
//! paint it as it is, which is why a mark made here looks the same in
//! every pdf reader instead of each reader guessing how a highlight or
//! a text box should look.
//!
//! code that draws an appearance works with points on the page as the
//! person sees it (y going down, see geometry.rs). the canvas turns those
//! into the upright local frame the stream is written in, and the
//! stream's /Matrix (from page_space.rs) turns that frame to follow the
//! page's rotation. so drawing code never has to think about rotation.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::color::Color;
use crate::error::AnnotateError;
use crate::fonts::StandardFont;
use crate::geometry::{Point, Rect};
use crate::page_space::PageSpace;
use crate::pdf_values::{format_number, name, number_array, real};

/// how far a cubic bezier's handles sit along the tangent to draw a
/// quarter circle: 4/3 * (sqrt(2) - 1).
const QUARTER_CIRCLE_HANDLE: f32 = 0.552_284_8;

/// a finished drawing, ready to be stored with an annotation.
pub(crate) struct Appearance {
    /// where the drawing sits on the visible page. it becomes the
    /// annotation's /Rect
    pub frame: Rect,
    pub content: Vec<u8>,
    pub resources: Dictionary,
}

/// collects drawing operators for one appearance stream.
pub(crate) struct Canvas {
    frame: Rect,
    content: String,
    graphics_states: Dictionary,
    fonts: Dictionary,
    images: Dictionary,
}

impl Canvas {
    /// a canvas covering `frame` on the visible page.
    pub fn new(frame: Rect) -> Self {
        Self {
            frame,
            content: String::new(),
            graphics_states: Dictionary::new(),
            fonts: Dictionary::new(),
            images: Dictionary::new(),
        }
    }

    /// a visible point in the stream's own upright frame (y goes up).
    fn local(&self, point: Point) -> String {
        format!(
            "{} {}",
            format_number(point.x - self.frame.x),
            format_number(self.frame.bottom() - point.y)
        )
    }

    fn push(&mut self, operator: &str) {
        self.content.push_str(operator);
        self.content.push('\n');
    }

    pub fn save(&mut self) {
        self.push("q");
    }

    pub fn restore(&mut self) {
        self.push("Q");
    }

    pub fn fill_color(&mut self, color: Color) {
        self.push(&color.fill_operator());
    }

    pub fn stroke_color(&mut self, color: Color) {
        self.push(&color.stroke_operator());
    }

    pub fn line_width(&mut self, width: f32) {
        self.push(&format!("{} w", format_number(width)));
    }

    /// dashes as alternating on and off lengths; empty means solid.
    pub fn dash(&mut self, pattern: &[f32]) {
        let lengths: Vec<String> = pattern
            .iter()
            .map(|length| format_number(*length))
            .collect();
        self.push(&format!("[{}] 0 d", lengths.join(" ")));
    }

    /// round ends and corners, which is how a pen or marker looks.
    pub fn round_line_ends(&mut self) {
        self.push("1 J 1 j");
    }

    /// sets how see-through strokes and fills are. `multiply` darkens
    /// what is underneath instead of covering it, which keeps text under
    /// a highlight black and readable.
    pub fn opacity(&mut self, opacity: f32, multiply: bool) {
        if opacity >= 1.0 && !multiply {
            return;
        }
        let mut state = Dictionary::new();
        state.set("Type", name("ExtGState"));
        state.set("CA", real(opacity));
        state.set("ca", real(opacity));
        if multiply {
            state.set("BM", name("Multiply"));
        }
        let state_name = format!("GS{}", self.graphics_states.len());
        self.graphics_states
            .set(state_name.clone(), Object::Dictionary(state));
        self.push(&format!("/{state_name} gs"));
    }

    pub fn move_to(&mut self, point: Point) {
        let local = self.local(point);
        self.push(&format!("{local} m"));
    }

    pub fn line_to(&mut self, point: Point) {
        let local = self.local(point);
        self.push(&format!("{local} l"));
    }

    pub fn curve_to(&mut self, handle_1: Point, handle_2: Point, end: Point) {
        let text = format!(
            "{} {} {} c",
            self.local(handle_1),
            self.local(handle_2),
            self.local(end)
        );
        self.push(&text);
    }

    pub fn close_path(&mut self) {
        self.push("h");
    }

    pub fn rect(&mut self, rect: Rect) {
        let corner = self.local(Point::new(rect.x, rect.bottom()));
        self.push(&format!(
            "{corner} {} {} re",
            format_number(rect.width),
            format_number(rect.height)
        ));
    }

    /// a path through the points, closed back to the first when asked.
    pub fn polyline(&mut self, points: &[Point], closed: bool) {
        let Some((first, rest)) = points.split_first() else {
            return;
        };
        self.move_to(*first);
        for point in rest {
            self.line_to(*point);
        }
        if closed {
            self.close_path();
        }
    }

    /// an ellipse filling `rect`, made of four bezier quarter circles.
    pub fn ellipse(&mut self, rect: Rect) {
        let center = rect.center();
        let (rx, ry) = (rect.width / 2.0, rect.height / 2.0);
        let (hx, hy) = (rx * QUARTER_CIRCLE_HANDLE, ry * QUARTER_CIRCLE_HANDLE);
        let at = |dx: f32, dy: f32| Point::new(center.x + dx, center.y + dy);
        self.move_to(at(rx, 0.0));
        self.curve_to(at(rx, hy), at(hx, ry), at(0.0, ry));
        self.curve_to(at(-hx, ry), at(-rx, hy), at(-rx, 0.0));
        self.curve_to(at(-rx, -hy), at(-hx, -ry), at(0.0, -ry));
        self.curve_to(at(hx, -ry), at(rx, -hy), at(rx, 0.0));
        self.close_path();
    }

    pub fn fill(&mut self) {
        self.push("f");
    }

    pub fn stroke(&mut self) {
        self.push("S");
    }

    pub fn fill_and_stroke(&mut self) {
        self.push("B");
    }

    /// forgets the path just built without painting it.
    pub fn end_path(&mut self) {
        self.push("n");
    }

    /// limits everything drawn after this (until `restore`) to `rect`.
    pub fn clip(&mut self, rect: Rect) {
        self.rect(rect);
        self.push("W n");
    }

    /// draws one line of encoded text starting at `baseline_start`.
    pub fn text(&mut self, font: StandardFont, size: f32, baseline_start: Point, encoded: &[u8]) {
        let font_name = font.resource_name();
        if !self.fonts.has(font_name.as_bytes()) {
            self.fonts
                .set(font_name, Object::Dictionary(font.dictionary()));
        }
        let hex: String = encoded.iter().map(|byte| format!("{byte:02X}")).collect();
        let text = format!(
            "BT /{font_name} {} Tf {} Td <{hex}> Tj ET",
            format_number(size),
            self.local(baseline_start)
        );
        self.push(&text);
    }

    /// draws an image object stretched over `rect`.
    pub fn image(&mut self, image: ObjectId, rect: Rect) {
        let image_name = format!("Im{}", self.images.len());
        self.images
            .set(image_name.clone(), Object::Reference(image));
        let corner = self.local(Point::new(rect.x, rect.bottom()));
        self.push(&format!(
            "q {} 0 0 {} {corner} cm /{image_name} Do Q",
            format_number(rect.width),
            format_number(rect.height)
        ));
    }

    pub fn finish(self) -> Appearance {
        let mut resources = Dictionary::new();
        if !self.graphics_states.is_empty() {
            resources.set("ExtGState", Object::Dictionary(self.graphics_states));
        }
        if !self.fonts.is_empty() {
            resources.set("Font", Object::Dictionary(self.fonts));
        }
        if !self.images.is_empty() {
            resources.set("XObject", Object::Dictionary(self.images));
        }
        Appearance {
            frame: self.frame,
            content: self.content.into_bytes(),
            resources,
        }
    }
}

/// stores an appearance as the annotation's normal appearance and moves
/// its /Rect to match. returns the appearance objects it replaced, so
/// the caller can remove the ones nothing else uses.
pub(crate) fn install(
    doc: &mut Document,
    annotation: ObjectId,
    appearance: Appearance,
    space: &PageSpace,
) -> Result<Vec<ObjectId>, AnnotateError> {
    let frame = appearance.frame;
    let mut form = Dictionary::new();
    form.set("Type", name("XObject"));
    form.set("Subtype", name("Form"));
    form.set("FormType", Object::Integer(1));
    form.set("BBox", number_array(&[0.0, 0.0, frame.width, frame.height]));
    form.set(
        "Matrix",
        number_array(&space.appearance_matrix().to_array()),
    );
    form.set("Resources", Object::Dictionary(appearance.resources));
    let mut stream = Stream::new(form, appearance.content);
    // compression is optional; a stream that fails to compress is still valid
    let _ = stream.compress();
    let stream_id = doc.add_object(Object::Stream(stream));

    let annotation = doc
        .get_dictionary_mut(annotation)
        .map_err(|_| AnnotateError::UnknownAnnotation)?;
    let replaced = appearance_objects(annotation);
    let mut states = Dictionary::new();
    states.set("N", Object::Reference(stream_id));
    annotation.set("AP", Object::Dictionary(states));
    // a single appearance needs no state name
    annotation.remove(b"AS");
    annotation.set("Rect", number_array(&space.user_rect(frame)));
    Ok(replaced)
}

/// every stream object an annotation's /AP points to, in all its
/// states (normal, rollover, down, and on/off variants).
pub(crate) fn appearance_objects(annotation: &Dictionary) -> Vec<ObjectId> {
    let Ok(Object::Dictionary(states)) = annotation.get(b"AP") else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (_, state) in states.iter() {
        match state {
            Object::Reference(id) => found.push(*id),
            Object::Dictionary(variants) => found.extend(
                variants
                    .iter()
                    .filter_map(|(_, variant)| variant.as_reference().ok()),
            ),
            _ => {}
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_are_written_in_the_upright_local_frame() {
        let mut canvas = Canvas::new(Rect::new(100.0, 200.0, 50.0, 20.0));
        // the visible top left of the frame is the local top left: (0, 20)
        canvas.move_to(Point::new(100.0, 200.0));
        canvas.line_to(Point::new(150.0, 220.0));
        let appearance = canvas.finish();
        assert_eq!(appearance.content, b"0 20 m\n50 0 l\n");
    }

    #[test]
    fn opacity_adds_a_graphics_state() {
        let mut canvas = Canvas::new(Rect::new(0.0, 0.0, 10.0, 10.0));
        canvas.opacity(0.5, true);
        let appearance = canvas.finish();
        let states = appearance
            .resources
            .get(b"ExtGState")
            .unwrap()
            .as_dict()
            .unwrap();
        let state = states.get(b"GS0").unwrap().as_dict().unwrap();
        assert_eq!(state.get(b"BM").unwrap().as_name().unwrap(), b"Multiply");
        assert_eq!(appearance.content, b"/GS0 gs\n");
    }
}
