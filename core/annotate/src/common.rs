//! the parts every annotation shares: its basic dictionary entries,
//! reading its colour, opacity and border, and checking values from the ui.

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::annots::{self, PageContext};
use crate::color::{clamp_opacity, Color};
use crate::dates;
use crate::error::AnnotateError;
use crate::geometry::{Point, Rect};
use crate::ids::new_unique_name;
use crate::pdf_values::{
    dict_dict, dict_name, dict_number, dict_numbers, name, number_array, real,
};
use crate::read::{self, AnnotationInfo};
use crate::redraw;
use crate::text_string;

/// the /F flag that asks viewers to print the annotation. without it a
/// mark shows on screen but disappears on paper, which surprises people.
pub(crate) const PRINT_FLAG: i64 = 4;

/// /F flag bits that mean "don't draw this".
pub(crate) const HIDDEN_FLAG: i64 = 2;
pub(crate) const NO_VIEW_FLAG: i64 = 32;

/// the /F flag bit that asks apps not to let the annotation be moved or
/// restyled. Letters respects it; such an annotation can still be deleted.
pub(crate) const LOCKED_FLAG: i64 = 128;

/// the widest stroke or border the ui may ask for, in points. far wider
/// than any real pen; it only stops numbers that would break a page.
const MAX_STROKE_WIDTH: f32 = 200.0;

/// the shared settings of a new annotation.
pub(crate) struct Basics<'a> {
    pub color: Option<Color>,
    pub opacity: f32,
    pub contents: Option<&'a str>,
    pub author: Option<&'a str>,
}

/// the entries every annotation made by Letters starts with.
pub(crate) fn new_dictionary(subtype: &str, page: &PageContext, basics: &Basics) -> Dictionary {
    let now = dates::now();
    let mut annotation = Dictionary::new();
    annotation.set("Type", name("Annot"));
    annotation.set("Subtype", name(subtype));
    annotation.set("NM", text_string::encode(&new_unique_name()));
    annotation.set("P", Object::Reference(page.id));
    annotation.set("F", Object::Integer(PRINT_FLAG));
    annotation.set("CreationDate", text_string::encode(&now));
    annotation.set("M", text_string::encode(&now));
    // a placeholder until the appearance is drawn and sets the real one
    annotation.set("Rect", number_array(&[0.0, 0.0, 0.0, 0.0]));
    if let Some(color) = basics.color {
        annotation.set("C", color.to_object());
    }
    annotation.set("CA", real(clamp_opacity(basics.opacity)));
    set_text(&mut annotation, "Contents", basics.contents);
    set_text(&mut annotation, "T", basics.author);
    annotation
}

/// sets or removes a text entry such as /Contents or /T.
pub(crate) fn set_text(annotation: &mut Dictionary, key: &str, text: Option<&str>) {
    match text {
        Some(text) => annotation.set(key, text_string::encode(text)),
        None => {
            annotation.remove(key.as_bytes());
        }
    }
}

/// marks the annotation as changed just now.
pub(crate) fn touch(annotation: &mut Dictionary) {
    annotation.set("M", text_string::encode(&dates::now()));
}

/// stores a new annotation, draws its appearance and adds it to the page.
pub(crate) fn add(
    doc: &mut Document,
    page: &PageContext,
    annotation: Dictionary,
) -> Result<AnnotationInfo, AnnotateError> {
    let id = doc.add_object(annotation);
    if let Err(error) = redraw::redraw(doc, page, id) {
        doc.objects.remove(&id);
        return Err(error);
    }
    annots::append(doc, page.id, id)?;
    read::info(doc, page, id)
}

/// the annotation's colour from /C (or another colour entry).
pub(crate) fn color(doc: &Document, annotation: &Dictionary, key: &[u8]) -> Option<Color> {
    Color::from_object(doc, annotation.get(key).ok()?)
}

/// how solid the annotation is, from /CA. missing means fully solid.
pub(crate) fn opacity(doc: &Document, annotation: &Dictionary) -> f32 {
    dict_number(doc, annotation, b"CA")
        .map(clamp_opacity)
        .unwrap_or(1.0)
}

pub(crate) fn flags(doc: &Document, annotation: &Dictionary) -> i64 {
    dict_number(doc, annotation, b"F").unwrap_or(0.0) as i64
}

/// how a shape's outline is drawn.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Border {
    pub width: f32,
    /// alternating on and off lengths; empty for a solid line
    pub dash: Vec<f32>,
}

/// reads the border from /BS, or from the older /Border array.
pub(crate) fn border(doc: &Document, annotation: &Dictionary, default_width: f32) -> Border {
    if let Some(style) = dict_dict(doc, annotation, b"BS") {
        let width = dict_number(doc, style, b"W").unwrap_or(default_width);
        let dashed = dict_name(doc, style, b"S") == Some(b"D");
        let dash = if dashed {
            dict_numbers(doc, style, b"D").unwrap_or_else(|| vec![3.0])
        } else {
            Vec::new()
        };
        return Border {
            width: width.max(0.0),
            dash: clean_dash(dash),
        };
    }
    if let Ok(old_style) = annotation.get(b"Border") {
        if let Some(values) = crate::pdf_values::numbers(doc, old_style) {
            let width = values.get(2).copied().unwrap_or(default_width);
            let dash = crate::pdf_values::resolve(doc, old_style)
                .as_array()
                .ok()
                .and_then(|items| items.get(3))
                .and_then(|pattern| crate::pdf_values::numbers(doc, pattern))
                .unwrap_or_default();
            return Border {
                width: width.max(0.0),
                dash: clean_dash(dash),
            };
        }
    }
    Border {
        width: default_width,
        dash: Vec::new(),
    }
}

/// writes a border as /BS, the current way.
pub(crate) fn set_border(annotation: &mut Dictionary, border: &Border) {
    let mut style = Dictionary::new();
    style.set("Type", name("Border"));
    style.set("W", real(border.width));
    if border.dash.is_empty() {
        style.set("S", name("S"));
    } else {
        style.set("S", name("D"));
        style.set("D", number_array(&border.dash));
    }
    annotation.set("BS", Object::Dictionary(style));
    // the old form would contradict the new one in some readers
    annotation.remove(b"Border");
}

/// drops a dash pattern that would draw nothing or loop forever.
pub(crate) fn clean_dash(dash: Vec<f32>) -> Vec<f32> {
    let usable = dash
        .iter()
        .all(|length| length.is_finite() && *length >= 0.0)
        && dash.iter().any(|length| *length > 0.0);
    if usable {
        dash
    } else {
        Vec::new()
    }
}

pub(crate) fn check_point(point: Point) -> Result<(), AnnotateError> {
    if point.is_finite() {
        Ok(())
    } else {
        Err(AnnotateError::InvalidValue(
            "That position isn’t on the page. Try placing it again.".into(),
        ))
    }
}

pub(crate) fn check_rect(rect: Rect) -> Result<(), AnnotateError> {
    if rect.is_empty() {
        Err(AnnotateError::EmptyArea)
    } else {
        Ok(())
    }
}

pub(crate) fn check_stroke_width(width: f32) -> Result<(), AnnotateError> {
    if width.is_finite() && width > 0.0 && width <= MAX_STROKE_WIDTH {
        Ok(())
    } else {
        Err(AnnotateError::InvalidValue(format!(
            "Line width must be more than 0 and at most {MAX_STROKE_WIDTH} points."
        )))
    }
}

/// the object an annotation refers to under `key`, such as its /Popup.
pub(crate) fn reference(annotation: &Dictionary, key: &[u8]) -> Option<ObjectId> {
    annotation.get(key).ok()?.as_reference().ok()
}
