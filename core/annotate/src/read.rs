//! listing the annotations on a page and describing each one for the ui.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::annots::{self, PageContext};
use crate::color::Color;
use crate::common::{self, HIDDEN_FLAG, LOCKED_FLAG, NO_VIEW_FLAG};
use crate::dates;
use crate::error::AnnotateError;
use crate::fonts::StandardFont;
use crate::geometry::{Point, Rect};
use crate::image;
use crate::ink;
use crate::markup;
use crate::note::NoteIcon;
use crate::pdf_values::{dict_bytes, dict_name, dict_rect, resolve};
use crate::shapes::{self, LineEnding};
use crate::text_box::{self, TextAlign, TextBorder};
use crate::text_string;

/// what kind of annotation something is, as the ui groups them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnnotationKind {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
    /// a sticky note, or a reply in a comment thread
    Note,
    TextBox,
    Square,
    Circle,
    Line,
    Polygon,
    PolyLine,
    Ink,
    /// a picture or a rubber stamp
    Stamp,
    /// a kind Letters doesn't edit, such as a link or a sound. it is
    /// listed so it can be deleted, and otherwise left alone
    Other,
}

impl AnnotationKind {
    fn from_subtype(subtype: &str) -> AnnotationKind {
        match subtype {
            "Highlight" => AnnotationKind::Highlight,
            "Underline" => AnnotationKind::Underline,
            "StrikeOut" => AnnotationKind::StrikeOut,
            "Squiggly" => AnnotationKind::Squiggly,
            "Text" => AnnotationKind::Note,
            "FreeText" => AnnotationKind::TextBox,
            "Square" => AnnotationKind::Square,
            "Circle" => AnnotationKind::Circle,
            "Line" => AnnotationKind::Line,
            "Polygon" => AnnotationKind::Polygon,
            "PolyLine" => AnnotationKind::PolyLine,
            "Ink" => AnnotationKind::Ink,
            "Stamp" => AnnotationKind::Stamp,
            _ => AnnotationKind::Other,
        }
    }
}

/// the parts of an annotation that depend on its kind: enough to draw
/// editing handles and fill the style controls.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AnnotationDetails {
    /// highlight, underline, strike-out, squiggly. each quad is four
    /// corners: top left, top right, bottom left, bottom right
    TextMarkup {
        quads: Vec<[Point; 4]>,
    },
    Note {
        icon: NoteIcon,
        open: bool,
        /// where the pop-up window sits, if the note has one
        popup: Option<Rect>,
    },
    TextBox {
        text: String,
        font: StandardFont,
        font_size: f32,
        text_color: Color,
        align: TextAlign,
        border: Option<TextBorder>,
        fill: Option<Color>,
    },
    /// square and circle
    Shape {
        stroke_width: f32,
        dash: Vec<f32>,
        fill: Option<Color>,
    },
    Line {
        start: Point,
        end: Point,
        start_ending: LineEnding,
        end_ending: LineEnding,
        stroke_width: f32,
        dash: Vec<f32>,
        fill: Option<Color>,
    },
    /// polygon and polyline
    Polygon {
        points: Vec<Point>,
        closed: bool,
        stroke_width: f32,
        dash: Vec<f32>,
        fill: Option<Color>,
    },
    Ink {
        strokes: Vec<Vec<Point>>,
        width: f32,
    },
    Stamp {
        /// true for pictures placed by Letters, false for other stamps
        is_picture: bool,
    },
    None,
}

/// one annotation as the ui sees it. every position is in points from
/// the visible top left corner of the page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationInfo {
    /// stable across saving and reopening: the annotation's /NM
    pub id: String,
    /// 0-based page number
    pub page: usize,
    pub kind: AnnotationKind,
    /// the pdf /Subtype, e.g. "Highlight" or "Link"
    pub subtype: String,
    /// the area the annotation covers
    pub rect: Rect,
    /// the main colour (/C): the mark, outline or note colour
    pub color: Option<Color>,
    pub opacity: f32,
    /// the comment text
    pub contents: Option<String>,
    pub author: Option<String>,
    /// ISO 8601, e.g. "2026-11-11T18:30:00Z"
    pub modified: Option<String>,
    pub created: Option<String>,
    /// the id of the annotation this one answers, for comment threads
    pub reply_to: Option<String>,
    /// hidden annotations exist in the file but aren't drawn
    pub hidden: bool,
    /// whether Letters can move and restyle it
    pub editable: bool,
    pub details: AnnotationDetails,
}

/// every annotation on a page, in drawing order (bottom first).
///
/// pop-up windows belong to their note and form fields belong to the
/// form tools, so neither is listed on its own.
pub fn list(doc: &Document, page: usize) -> Result<Vec<AnnotationInfo>, AnnotateError> {
    let page = annots::page(doc, page)?;
    let mut found = Vec::new();
    for (index, entry) in annots::entries(doc, page.id).iter().enumerate() {
        let described = match entry {
            Object::Reference(object) => match doc.get_dictionary(*object) {
                Ok(annotation) => describe(
                    doc,
                    &page,
                    annotation,
                    annots::id_of(doc, *object, annotation),
                ),
                Err(_) => None,
            },
            Object::Dictionary(annotation) => {
                let id = match dict_bytes(doc, annotation, b"NM") {
                    Some(name) if !name.is_empty() => text_string::decode(name),
                    _ => annots::position_id(index),
                };
                describe(doc, &page, annotation, id)
            }
            _ => None,
        };
        found.extend(described);
    }
    Ok(found)
}

/// one annotation by its id.
pub fn get(doc: &Document, page: usize, id: &str) -> Result<AnnotationInfo, AnnotateError> {
    let context = annots::page(doc, page)?;
    if let Ok(object) = annots::find(doc, context.id, id) {
        return info(doc, &context, object);
    }
    list(doc, page)?
        .into_iter()
        .find(|annotation| annotation.id == id)
        .ok_or(AnnotateError::UnknownAnnotation)
}

/// describes an annotation object on a known page.
pub(crate) fn info(
    doc: &Document,
    page: &PageContext,
    object: ObjectId,
) -> Result<AnnotationInfo, AnnotateError> {
    let annotation = doc
        .get_dictionary(object)
        .map_err(|_| AnnotateError::UnknownAnnotation)?;
    describe(
        doc,
        page,
        annotation,
        annots::id_of(doc, object, annotation),
    )
    .ok_or(AnnotateError::UnknownAnnotation)
}

/// none for the entries that aren't listed (pop-ups and form fields).
fn describe(
    doc: &Document,
    page: &PageContext,
    annotation: &Dictionary,
    id: String,
) -> Option<AnnotationInfo> {
    let subtype = annots::subtype(doc, annotation);
    if matches!(subtype.as_str(), "Popup" | "Widget") {
        return None;
    }
    let space = &page.space;
    let kind = AnnotationKind::from_subtype(&subtype);
    let text = |key: &[u8]| dict_bytes(doc, annotation, key).map(text_string::decode);
    let date = |key: &[u8]| text(key).and_then(|date| dates::iso_from_pdf_date(&date));
    let flags = common::flags(doc, annotation);
    let reply_to = annotation
        .get(b"IRT")
        .ok()
        .and_then(|target| target.as_reference().ok())
        .and_then(|target| {
            let target_dict = doc.get_dictionary(target).ok()?;
            Some(annots::id_of(doc, target, target_dict))
        });
    Some(AnnotationInfo {
        id,
        page: page.index,
        kind,
        rect: dict_rect(doc, annotation, b"Rect")
            .map(|rect| space.visible_rect(rect))
            .unwrap_or(Rect::new(0.0, 0.0, 0.0, 0.0)),
        color: common::color(doc, annotation, b"C"),
        opacity: common::opacity(doc, annotation),
        contents: text(b"Contents"),
        author: text(b"T"),
        modified: date(b"M"),
        created: date(b"CreationDate"),
        reply_to,
        hidden: flags & (HIDDEN_FLAG | NO_VIEW_FLAG) != 0,
        editable: kind != AnnotationKind::Other && flags & LOCKED_FLAG == 0,
        details: details(doc, annotation, kind, page),
        subtype,
    })
}

fn details(
    doc: &Document,
    annotation: &Dictionary,
    kind: AnnotationKind,
    page: &PageContext,
) -> AnnotationDetails {
    let space = &page.space;
    let border = || common::border(doc, annotation, 1.0);
    match kind {
        AnnotationKind::Highlight
        | AnnotationKind::Underline
        | AnnotationKind::StrikeOut
        | AnnotationKind::Squiggly => AnnotationDetails::TextMarkup {
            quads: markup::quads(doc, annotation, space),
        },
        AnnotationKind::Note => AnnotationDetails::Note {
            icon: dict_name(doc, annotation, b"Name")
                .map(NoteIcon::from_pdf_name)
                .unwrap_or(NoteIcon::Note),
            open: matches!(
                annotation.get(b"Open").map(|open| resolve(doc, open)),
                Ok(Object::Boolean(true))
            ),
            popup: common::reference(annotation, b"Popup")
                .and_then(|popup| doc.get_dictionary(popup).ok())
                .and_then(|popup| dict_rect(doc, popup, b"Rect"))
                .map(|rect| space.visible_rect(rect)),
        },
        AnnotationKind::TextBox => {
            let style = text_box::read_style(doc, annotation);
            AnnotationDetails::TextBox {
                text: text_box::text(doc, annotation),
                font: style.font,
                font_size: style.font_size,
                text_color: style.text_color,
                align: style.align,
                border: style.border,
                fill: style.fill,
            }
        }
        AnnotationKind::Square | AnnotationKind::Circle => AnnotationDetails::Shape {
            stroke_width: border().width,
            dash: border().dash,
            fill: common::color(doc, annotation, b"IC"),
        },
        AnnotationKind::Line => {
            let (start, end) = shapes::line_ends(doc, annotation, space)
                .unwrap_or((Point::new(0.0, 0.0), Point::new(0.0, 0.0)));
            let (start_ending, end_ending) = shapes::line_endings(doc, annotation);
            AnnotationDetails::Line {
                start,
                end,
                start_ending,
                end_ending,
                stroke_width: border().width,
                dash: border().dash,
                fill: common::color(doc, annotation, b"IC"),
            }
        }
        AnnotationKind::Polygon | AnnotationKind::PolyLine => AnnotationDetails::Polygon {
            points: shapes::vertices(doc, annotation, space),
            closed: kind == AnnotationKind::Polygon,
            stroke_width: border().width,
            dash: border().dash,
            fill: common::color(doc, annotation, b"IC"),
        },
        AnnotationKind::Ink => AnnotationDetails::Ink {
            strokes: ink::strokes(doc, annotation, space),
            width: border().width,
        },
        AnnotationKind::Stamp => AnnotationDetails::Stamp {
            is_picture: image::is_picture_stamp(doc, annotation),
        },
        AnnotationKind::Other => AnnotationDetails::None,
    }
}
