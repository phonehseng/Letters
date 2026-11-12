//! the box a widget draws into (size, turn, border and background), and
//! storing a finished drawing as the widget's appearance stream.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::error::FormResult;
use crate::geometry::{number, Matrix, PdfRect};
use crate::objects::{
    get, get_array, get_dict, get_integer, get_name, get_rect, name, number_of, rect_object,
    resolve,
};

/// the box a widget draws into, in its own (possibly turned) space.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetBox {
    pub width: f32,
    pub height: f32,
    /// turns the drawing to match /MK /R, so text reads upright on turned pages
    pub matrix: Matrix,
    pub border_width: f32,
    /// stroke colour operator, when the widget has a border colour
    pub border_color: Option<String>,
    /// fill colour operator, when the widget has a background
    pub background: Option<String>,
    /// /BS /S: solid, dashed, beveled, inset or underline
    pub border_style: u8,
}

impl WidgetBox {
    pub fn read(doc: &Document, widget: ObjectId) -> Option<Self> {
        let dict = doc.get_dictionary(widget).ok()?;
        let rect = get_rect(doc, dict, b"Rect")?;
        let characteristics = get_dict(doc, dict, b"MK");
        let rotation = characteristics
            .and_then(|mk| get_integer(doc, mk, b"R"))
            .unwrap_or(0)
            .rem_euclid(360) as i32;
        let (width, height) = if rotation % 180 == 0 {
            (rect.width(), rect.height())
        } else {
            (rect.height(), rect.width())
        };
        let color_of = |key: &[u8], operators: [&str; 3]| {
            let values = characteristics.and_then(|mk| get_array(doc, mk, key))?;
            color_operator(doc, values, operators)
        };
        let border_style = get_dict(doc, dict, b"BS");
        let border_width = border_style
            .and_then(|bs| get(doc, bs, b"W").and_then(number_of))
            .or_else(|| {
                let border = get_array(doc, dict, b"Border")?;
                border
                    .get(2)
                    .and_then(|value| number_of(resolve(doc, value)))
            })
            .unwrap_or(1.0)
            .max(0.0);
        let style = border_style
            .and_then(|bs| get_name(doc, bs, b"S"))
            .and_then(|style| style.first().copied())
            .unwrap_or(b'S');
        let border_color = color_of(b"BC", ["G", "RG", "K"]);
        Some(Self {
            width,
            height,
            matrix: upright_matrix(rotation, width, height),
            border_width: if border_color.is_some() {
                border_width
            } else {
                0.0
            },
            border_color,
            background: color_of(b"BG", ["g", "rg", "k"]),
            border_style: style,
        })
    }

    /// the bounding box of the drawing, [0 0 width height].
    pub fn bbox(&self) -> PdfRect {
        PdfRect::from_corners(0.0, 0.0, self.width, self.height)
    }
}

/// the /Matrix that turns a drawing of `width` by `height` counterclockwise
/// by `rotation` degrees and moves it back so it starts at 0, 0.
pub(crate) fn upright_matrix(rotation: i32, width: f32, height: f32) -> Matrix {
    let turn = Matrix::quarter_turns(rotation);
    let moved = turn.apply_rect(&PdfRect::from_corners(0.0, 0.0, width, height));
    turn.then(&Matrix::translate(-moved.left, -moved.bottom))
}

/// a colour from /MK as an operator. no components means "no colour".
fn color_operator(doc: &Document, values: &[Object], operators: [&str; 3]) -> Option<String> {
    let numbers: Vec<String> = values
        .iter()
        .filter_map(|value| number_of(resolve(doc, value)))
        .map(number)
        .collect();
    let operator = match numbers.len() {
        1 => operators[0],
        3 => operators[1],
        4 => operators[2],
        _ => return None,
    };
    Some(format!("{} {operator}", numbers.join(" ")))
}

/// a drawing to be stored as an appearance stream.
#[derive(Clone, Debug)]
pub struct Appearance {
    pub content: String,
    pub fonts: Vec<(String, Object)>,
}

/// the background and border every kind of field starts with.
pub(super) fn frame_content(widget: &WidgetBox, round: bool) -> String {
    let mut out = String::new();
    let (w, h) = (widget.width, widget.height);
    if let Some(background) = &widget.background {
        out.push_str(&format!("{background}\n"));
        if round {
            out.push_str(&circle_path(w / 2.0, h / 2.0, w.min(h) / 2.0));
        } else {
            out.push_str(&format!("0 0 {} {} re\n", number(w), number(h)));
        }
        out.push_str("f\n");
    }
    if let Some(border) = &widget.border_color {
        let line = widget.border_width;
        if line > 0.0 {
            out.push_str(&format!("{border}\n{} w\n", number(line)));
            if widget.border_style == b'D' {
                out.push_str("[3] 0 d\n");
            }
            let half = line / 2.0;
            if round {
                out.push_str(&circle_path(w / 2.0, h / 2.0, w.min(h) / 2.0 - half));
            } else if widget.border_style == b'U' {
                out.push_str(&format!("0 {0} m {1} {0} l\n", number(half), number(w)));
            } else {
                out.push_str(&format!(
                    "{0} {0} {1} {2} re\n",
                    number(half),
                    number(w - line),
                    number(h - line)
                ));
            }
            out.push_str("S\n");
        }
    }
    out
}

/// a circle drawn with four curves, the usual way pdf draws circles.
pub(super) fn circle_path(cx: f32, cy: f32, r: f32) -> String {
    // how far the control points sit from the ends so curves look round
    let k = 0.5523 * r;
    let n = number;
    format!(
        "{} {} m {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c {} {} {} {} {} {} c h\n",
        n(cx + r), n(cy),
        n(cx + r), n(cy + k), n(cx + k), n(cy + r), n(cx), n(cy + r),
        n(cx - k), n(cy + r), n(cx - r), n(cy + k), n(cx - r), n(cy),
        n(cx - r), n(cy - k), n(cx - k), n(cy - r), n(cx), n(cy - r),
        n(cx + k), n(cy - r), n(cx + r), n(cy - k), n(cx + r), n(cy),
    )
}

/// turns a drawing into a form xobject stream sized for the widget.
pub fn appearance_stream(widget: &WidgetBox, appearance: &Appearance) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", name("XObject"));
    dict.set("Subtype", name("Form"));
    dict.set("BBox", rect_object(&widget.bbox()));
    if widget.matrix != Matrix::IDENTITY {
        let m = widget.matrix;
        dict.set(
            "Matrix",
            Object::Array(
                [m.a, m.b, m.c, m.d, m.e, m.f]
                    .iter()
                    .map(|v| crate::objects::real(*v))
                    .collect(),
            ),
        );
    }
    let mut resources = Dictionary::new();
    if !appearance.fonts.is_empty() {
        let mut fonts = Dictionary::new();
        for (font_name, object) in &appearance.fonts {
            fonts.set(font_name.as_bytes().to_vec(), object.clone());
        }
        resources.set("Font", Object::Dictionary(fonts));
    }
    dict.set("Resources", Object::Dictionary(resources));
    Stream::new(dict, appearance.content.as_bytes().to_vec())
}

/// stores a single normal appearance for a widget (text and choice fields).
pub fn store_appearance(
    doc: &mut Document,
    widget_id: ObjectId,
    widget: &WidgetBox,
    appearance: &Appearance,
) -> FormResult<()> {
    let stream = doc.add_object(appearance_stream(widget, appearance));
    let mut appearances = Dictionary::new();
    appearances.set("N", Object::Reference(stream));
    doc.get_dictionary_mut(widget_id)?
        .set("AP", Object::Dictionary(appearances));
    Ok(())
}

/// stores the on and off drawings of a checkbox or radio widget.
pub fn store_state_appearances(
    doc: &mut Document,
    widget_id: ObjectId,
    widget: &WidgetBox,
    state: &[u8],
    on: &Appearance,
    off: &Appearance,
) -> FormResult<()> {
    let on_stream = doc.add_object(appearance_stream(widget, on));
    let off_stream = doc.add_object(appearance_stream(widget, off));
    let mut states = Dictionary::new();
    states.set(state.to_vec(), Object::Reference(on_stream));
    states.set("Off", Object::Reference(off_stream));
    let mut appearances = Dictionary::new();
    appearances.set("N", Object::Dictionary(states));
    doc.get_dictionary_mut(widget_id)?
        .set("AP", Object::Dictionary(appearances));
    Ok(())
}
