//! adds highlights, notes, text boxes, shapes and freehand ink to pages.
//!
//! every operation works on a `lopdf::Document` held in memory and
//! changes it in place. the app applies each one to its working copy and
//! keeps its own undo history, so nothing here touches files.
//!
//! the rules every function follows:
//! - pages are counted from 0
//! - every position is in points (1/72 inch) from the top left corner of
//!   the page as the person sees it, with y going down. turning the
//!   page (/Rotate), cropping it (/CropBox) and odd page origins are
//!   handled here, so the ui never has to think about them
//! - annotations are standard pdf annotations with their own drawing
//!   (an appearance stream), so they look the same in every pdf reader
//! - every annotation has a stable id (its /NM) that survives saving
//! - errors are sentences the ui can show as they are
//!
//! the modules, smallest first:
//! - geometry, page_space: points, rectangles and the visible-to-pdf conversion
//! - color, pdf_values, text_string, dates, ids: small value helpers
//! - font_metrics, fonts, winansi, text_layout: measuring and wrapping text
//! - appearance: drawing appearance streams
//! - annots, common, redraw: page lists and what every annotation shares
//! - markup, note, text_box, shapes, ink, image: one module per kind
//! - read, edit, flatten: listing, changing, deleting and flattening

mod annots;
mod appearance;
mod color;
mod common;
mod dates;
mod edit;
mod error;
mod flatten;
mod font_metrics;
mod fonts;
mod geometry;
mod ids;
mod image;
mod ink;
mod markup;
mod note;
mod page_space;
mod pdf_values;
mod read;
mod redraw;
mod shapes;
mod text_box;
mod text_layout;
mod text_string;
mod winansi;

use lopdf::Document;
use serde::{Deserialize, Serialize};

pub use color::Color;
pub use edit::{delete, update, AnnotationChanges};
pub use error::AnnotateError;
pub use flatten::{flatten, FlattenScope, FlattenSummary, FLATTEN_EXPLANATION};
pub use fonts::StandardFont;
pub use geometry::{Point, Rect};
pub use image::{add_image, ImageParams};
pub use ink::{add_ink, InkParams};
pub use markup::{add_text_markup, TextMarkupKind, TextMarkupParams};
pub use note::{add_note, add_reply, NoteIcon, NoteParams, ReplyParams};
pub use read::{get, list, AnnotationDetails, AnnotationInfo, AnnotationKind};
pub use shapes::{
    add_line, add_polygon, add_shape, LineEnding, LineParams, PolygonParams, ShapeKind,
    ShapeParams, StrokeStyle,
};
pub use text_box::{add_text_box, TextAlign, TextBorder, TextBoxParams};

/// any new annotation, for a shell that prefers one "create" command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NewAnnotation {
    TextMarkup(TextMarkupParams),
    Note(NoteParams),
    TextBox(TextBoxParams),
    Shape(ShapeParams),
    Line(LineParams),
    Polygon(PolygonParams),
    Ink(InkParams),
    Image(ImageParams),
}

/// adds any kind of annotation to a page. the same as calling the
/// matching `add_` function.
pub fn create(
    doc: &mut Document,
    page: usize,
    annotation: &NewAnnotation,
) -> Result<AnnotationInfo, AnnotateError> {
    match annotation {
        NewAnnotation::TextMarkup(params) => add_text_markup(doc, page, params),
        NewAnnotation::Note(params) => add_note(doc, page, params),
        NewAnnotation::TextBox(params) => add_text_box(doc, page, params),
        NewAnnotation::Shape(params) => add_shape(doc, page, params),
        NewAnnotation::Line(params) => add_line(doc, page, params),
        NewAnnotation::Polygon(params) => add_polygon(doc, page, params),
        NewAnnotation::Ink(params) => add_ink(doc, page, params),
        NewAnnotation::Image(params) => add_image(doc, page, params),
    }
}

/// the default opacity: fully solid.
fn full_opacity() -> f32 {
    1.0
}
