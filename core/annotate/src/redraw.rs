//! redrawing an annotation's appearance from what its dictionary says.
//!
//! the dictionary is the single source of truth: creating an annotation
//! writes its entries and then calls this, and so does every change. so
//! a mark always looks like what its entries describe.

use lopdf::{Dictionary, Document, ObjectId};

use crate::annots::{self, PageContext};
use crate::appearance::{self, Appearance};
use crate::error::AnnotateError;
use crate::page_space::PageSpace;
use crate::{image, ink, markup, note, shapes, text_box};

/// draws a fresh appearance for the annotation and stores it.
pub(crate) fn redraw(
    doc: &mut Document,
    page: &PageContext,
    annotation: ObjectId,
) -> Result<(), AnnotateError> {
    let dictionary = doc
        .get_dictionary(annotation)
        .map_err(|_| AnnotateError::UnknownAnnotation)?;
    let drawing = draw(doc, dictionary, &page.space)?;
    let replaced = appearance::install(doc, annotation, drawing, &page.space)?;
    annots::remove_unused(doc, page.id, &replaced);
    Ok(())
}

/// true for the kinds of annotation Letters knows how to draw.
pub(crate) fn can_draw(subtype: &str) -> bool {
    matches!(
        subtype,
        "Highlight"
            | "Underline"
            | "StrikeOut"
            | "Squiggly"
            | "Text"
            | "FreeText"
            | "Square"
            | "Circle"
            | "Line"
            | "Polygon"
            | "PolyLine"
            | "Ink"
            | "Stamp"
    )
}

fn draw(
    doc: &Document,
    annotation: &Dictionary,
    space: &PageSpace,
) -> Result<Appearance, AnnotateError> {
    match annots::subtype(doc, annotation).as_str() {
        "Highlight" | "Underline" | "StrikeOut" | "Squiggly" => {
            markup::draw(doc, annotation, space)
        }
        "Text" => Ok(note::draw(doc, annotation, space)),
        "FreeText" => text_box::draw(doc, annotation, space),
        "Square" | "Circle" => shapes::draw_box_shape(doc, annotation, space),
        "Line" => shapes::draw_line(doc, annotation, space),
        "Polygon" | "PolyLine" => shapes::draw_polygon(doc, annotation, space),
        "Ink" => ink::draw(doc, annotation, space),
        "Stamp" => image::draw(doc, annotation, space),
        _ => Err(AnnotateError::CannotChange),
    }
}
