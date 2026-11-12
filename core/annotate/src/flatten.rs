//! flattening: painting annotations into the page itself.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::annots::{self, PageContext};
use crate::appearance::appearance_objects;
use crate::common::{self, HIDDEN_FLAG, NO_VIEW_FLAG};
use crate::edit::thread_of;
use crate::error::AnnotateError;
use crate::geometry::Matrix;
use crate::pdf_values::{
    dict_dict, dict_name, dict_numbers, dict_rect, format_number, name, resolve,
};
use crate::redraw;

/// which annotations to flatten.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "scope",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FlattenScope {
    /// every comment and mark in the document
    AllPages,
    /// every comment and mark on these 0-based pages
    Pages { pages: Vec<usize> },
    /// just these annotations on one page (with their replies and pop-ups)
    Annotations { page: usize, ids: Vec<String> },
}

/// what flattening did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlattenSummary {
    /// how many annotations became part of their page, replies included
    pub flattened: usize,
    /// the pages that changed, 0-based
    pub pages: Vec<usize>,
}

/// the kinds of annotation that are comments or marks and can be
/// flattened. links, form fields and redaction marks are left alone:
/// flattening a link would break it, form fields belong to the form
/// tools, and redaction marks must be applied, not painted over.
const FLATTENABLE: [&str; 16] = [
    "Text",
    "FreeText",
    "Line",
    "Square",
    "Circle",
    "Polygon",
    "PolyLine",
    "Highlight",
    "Underline",
    "Squiggly",
    "StrikeOut",
    "Stamp",
    "Caret",
    "Ink",
    "FileAttachment",
    "Sound",
];

/// what flattening means, in words the ui can show next to the button.
pub const FLATTEN_EXPLANATION: &str = "Flattening makes comments and marks a permanent part of the page: they look exactly the same, but can no longer be edited, moved or removed.";

/// Flattening makes comments and marks a permanent part of the page: they
/// look exactly the same, but can no longer be edited, moved or removed.
/// (the same sentence is in `FLATTEN_EXPLANATION` for the ui.)
///
/// each annotation's appearance is painted into the page content where
/// the annotation was, and the annotation objects (with their pop-ups
/// and replies) are removed from the file. use it before sharing a file
/// so nobody can change the comments in it.
pub fn flatten(doc: &mut Document, scope: &FlattenScope) -> Result<FlattenSummary, AnnotateError> {
    annots::ensure_unlocked(doc)?;
    let mut summary = FlattenSummary::default();
    let work: Vec<(usize, Option<&[String]>)> = match scope {
        FlattenScope::AllPages => (0..doc.get_pages().len())
            .map(|page| (page, None))
            .collect(),
        FlattenScope::Pages { pages } => pages.iter().map(|page| (*page, None)).collect(),
        FlattenScope::Annotations { page, ids } => vec![(*page, Some(ids.as_slice()))],
    };
    // check every page first, so a bad page number changes nothing
    let pages: Vec<PageContext> = work
        .iter()
        .map(|(page, _)| annots::page(doc, *page))
        .collect::<Result<_, _>>()?;
    for (page, (_, ids)) in pages.iter().zip(&work) {
        let count = flatten_page(doc, page, *ids)?;
        if count > 0 {
            summary.flattened += count;
            summary.pages.push(page.index);
        }
    }
    Ok(summary)
}

/// flattens one page, returning how many annotations were flattened.
fn flatten_page(
    doc: &mut Document,
    page: &PageContext,
    ids: Option<&[String]>,
) -> Result<usize, AnnotateError> {
    annots::make_entries_objects(doc, page.id)?;
    let targets = targets(doc, page, ids)?;
    if targets.is_empty() {
        return Ok(0);
    }
    let mut painted = Vec::new();
    // the page list is in drawing order, so marks keep their stacking
    for object in annots::annotation_ids(doc, page.id) {
        if !targets.contains(&object) {
            continue;
        }
        ensure_appearance(doc, page, object);
        if let Some(placement) = placement(doc, object) {
            painted.push(placement);
        }
    }
    paint_into_page(doc, page.id, &painted)?;

    let painted_streams: HashSet<ObjectId> = painted.iter().map(|(stream, _)| *stream).collect();
    let mut leftovers = Vec::new();
    let mut flattened = 0;
    for object in &targets {
        if let Ok(annotation) = doc.get_dictionary(*object) {
            leftovers.extend(
                appearance_objects(annotation)
                    .into_iter()
                    .filter(|stream| !painted_streams.contains(stream)),
            );
            // pop-ups are part of their note, so they aren't counted
            if annots::subtype(doc, annotation) != "Popup" {
                flattened += 1;
            }
        }
    }
    annots::remove(doc, page.id, &targets)?;
    for object in &targets {
        doc.objects.remove(object);
    }
    annots::remove_unused(doc, page.id, &leftovers);
    Ok(flattened)
}

/// the annotations to flatten on this page: the chosen ones with their
/// threads, or every flattenable one.
fn targets(
    doc: &Document,
    page: &PageContext,
    ids: Option<&[String]>,
) -> Result<HashSet<ObjectId>, AnnotateError> {
    let mut targets = HashSet::new();
    match ids {
        Some(ids) => {
            for id in ids {
                let object = annots::find(doc, page.id, id)?;
                let annotation = doc
                    .get_dictionary(object)
                    .map_err(|_| AnnotateError::UnknownAnnotation)?;
                match annots::subtype(doc, annotation).as_str() {
                    "Widget" => return Err(AnnotateError::FormField),
                    subtype if !FLATTENABLE.contains(&subtype) => {
                        return Err(AnnotateError::CannotChange)
                    }
                    _ => {}
                }
                targets.extend(thread_of(doc, page.id, object));
            }
        }
        None => {
            for object in annots::annotation_ids(doc, page.id) {
                let Ok(annotation) = doc.get_dictionary(object) else {
                    continue;
                };
                if FLATTENABLE.contains(&annots::subtype(doc, annotation).as_str()) {
                    targets.extend(thread_of(doc, page.id, object));
                }
            }
        }
    }
    Ok(targets)
}

/// gives an annotation Letters can draw an appearance if it has none,
/// so it looks the same after flattening as viewers showed it before.
fn ensure_appearance(doc: &mut Document, page: &PageContext, object: ObjectId) {
    let Ok(annotation) = doc.get_dictionary(object) else {
        return;
    };
    let subtype = annots::subtype(doc, annotation);
    if normal_appearance(doc, annotation).is_none()
        && redraw::can_draw(&subtype)
        && subtype != "Popup"
    {
        // if it can't be drawn it is simply removed, as it showed nothing
        let _ = redraw::redraw(doc, page, object);
    }
}

/// the stream an annotation shows normally, picking the current state
/// (/AS) when it has several, such as a checked and unchecked look.
fn normal_appearance(doc: &Document, annotation: &Dictionary) -> Option<ObjectId> {
    let normal = dict_dict(doc, annotation, b"AP")?.get(b"N").ok()?;
    let stream = match normal {
        Object::Reference(id) if doc.get_object(*id).and_then(Object::as_stream).is_ok() => *id,
        _ => {
            let states = resolve(doc, normal).as_dict().ok()?;
            let state = dict_name(doc, annotation, b"AS")?;
            states.get(state).ok()?.as_reference().ok()?
        }
    };
    doc.get_object(stream).ok()?.as_stream().ok()?;
    Some(stream)
}

/// where an annotation's appearance goes on the page: the stream and
/// the matrix that puts it on the annotation's /Rect.
///
/// this is the placement the pdf standard describes for viewers
/// (section 12.5.5): transform the stream's /BBox by its /Matrix, then
/// scale and move that box onto /Rect. painting with the same placement
/// is what makes the flattened page look identical.
fn placement(doc: &mut Document, object: ObjectId) -> Option<(ObjectId, Matrix)> {
    let annotation = doc.get_dictionary(object).ok()?;
    if common::flags(doc, annotation) & (HIDDEN_FLAG | NO_VIEW_FLAG) != 0 {
        return None;
    }
    let stream_id = normal_appearance(doc, annotation)?;
    let [left, bottom, right, top] = dict_rect(doc, annotation, b"Rect")?;
    let stream = doc.get_object(stream_id).ok()?.as_stream().ok()?;
    let bbox = dict_rect(doc, &stream.dict, b"BBox")?;
    let matrix = match dict_numbers(doc, &stream.dict, b"Matrix").as_deref() {
        Some([a, b, c, d, e, f]) => Matrix::new(*a, *b, *c, *d, *e, *f),
        _ => Matrix::IDENTITY,
    };
    let [box_left, box_bottom, box_right, box_top] = matrix.apply_to_box(bbox);
    let (box_width, box_height) = (box_right - box_left, box_top - box_bottom);
    if box_width <= 0.0 || box_height <= 0.0 {
        return None;
    }
    let scale_x = (right - left) / box_width;
    let scale_y = (top - bottom) / box_height;
    let fit = Matrix::new(
        scale_x,
        0.0,
        0.0,
        scale_y,
        left - box_left * scale_x,
        bottom - box_bottom * scale_y,
    );
    // an appearance stream is a form, but some writers leave out its type
    let stream = doc.get_object_mut(stream_id).ok()?.as_stream_mut().ok()?;
    stream.dict.set("Type", name("XObject"));
    stream.dict.set("Subtype", name("Form"));
    Some((stream_id, fit))
}

/// adds the appearances to the page's resources and draws them after
/// the existing content.
fn paint_into_page(
    doc: &mut Document,
    page: ObjectId,
    painted: &[(ObjectId, Matrix)],
) -> Result<(), AnnotateError> {
    if painted.is_empty() {
        return Ok(());
    }
    let names = add_forms_to_resources(doc, page, painted)?;
    let mut drawing = String::from("Q\n");
    for ((_, matrix), form_name) in painted.iter().zip(&names) {
        let numbers: Vec<String> = matrix
            .to_array()
            .iter()
            .map(|value| format_number(*value))
            .collect();
        drawing.push_str(&format!("q {} cm /{form_name} Do Q\n", numbers.join(" ")));
    }
    wrap_page_content(doc, page, drawing.into_bytes())
}

/// puts the page's existing content between "q" and "Q" and adds the
/// new drawing after it.
///
/// the existing content may leave the drawing state changed (moved,
/// scaled, clipped) when it ends; the q/Q pair resets that, so the
/// flattened marks land where the annotations were.
fn wrap_page_content(
    doc: &mut Document,
    page: ObjectId,
    drawing: Vec<u8>,
) -> Result<(), AnnotateError> {
    let damaged = || AnnotateError::Damaged("the page is missing".into());
    let existing: Vec<Object> = match doc
        .get_dictionary(page)
        .map_err(|_| damaged())?
        .get(b"Contents")
    {
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(parts)) => parts.clone(),
            _ => vec![Object::Reference(*id)],
        },
        Ok(Object::Array(parts)) => parts.clone(),
        _ => Vec::new(),
    };
    let opening = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
    let mut closing = Stream::new(Dictionary::new(), drawing);
    let _ = closing.compress();
    let closing = doc.add_object(closing);
    let mut parts = vec![Object::Reference(opening)];
    parts.extend(existing);
    parts.push(Object::Reference(closing));
    doc.get_dictionary_mut(page)
        .map_err(|_| damaged())?
        .set("Contents", Object::Array(parts));
    Ok(())
}

/// adds each appearance to the page's /Resources /XObject under a name
/// not used yet, and returns the names in order.
fn add_forms_to_resources(
    doc: &mut Document,
    page: ObjectId,
    painted: &[(ObjectId, Matrix)],
) -> Result<Vec<String>, AnnotateError> {
    let forms = xobject_dictionary(doc, page)?;
    let mut names = Vec::new();
    let mut counter = 0;
    for (stream, _) in painted {
        let form_name = loop {
            counter += 1;
            let candidate = format!("LettersFlat{counter}");
            if !forms.has(candidate.as_bytes()) {
                break candidate;
            }
        };
        forms.set(form_name.clone(), Object::Reference(*stream));
        names.push(form_name);
    }
    Ok(names)
}

/// the page's /XObject resource dictionary, made writable.
///
/// resources may be inherited from a parent in the page tree, shared
/// with other pages behind a reference, or missing. inherited ones are
/// copied onto the page so other pages don't change; shared ones are
/// written in place, which is harmless because new names are unique.
fn xobject_dictionary(
    doc: &mut Document,
    page: ObjectId,
) -> Result<&mut Dictionary, AnnotateError> {
    let damaged = || AnnotateError::Damaged("the page resources are damaged".into());
    let page_dict = doc.get_dictionary(page).map_err(|_| damaged())?;
    if !page_dict.has(b"Resources") {
        let inherited = inherited_resources(doc, page_dict).unwrap_or_default();
        doc.get_dictionary_mut(page)
            .map_err(|_| damaged())?
            .set("Resources", Object::Dictionary(inherited));
    }
    let resources_id = doc
        .get_dictionary(page)
        .map_err(|_| damaged())?
        .get(b"Resources")
        .ok()
        .and_then(|resources| resources.as_reference().ok());
    let xobjects_id = {
        let resources = match resources_id {
            Some(id) => doc.get_dictionary(id).map_err(|_| damaged())?,
            None => doc
                .get_dictionary(page)
                .map_err(|_| damaged())?
                .get(b"Resources")
                .and_then(Object::as_dict)
                .map_err(|_| damaged())?,
        };
        resources
            .get(b"XObject")
            .ok()
            .and_then(|xobjects| xobjects.as_reference().ok())
    };
    if let Some(id) = xobjects_id {
        return doc.get_dictionary_mut(id).map_err(|_| damaged());
    }
    let resources = match resources_id {
        Some(id) => doc.get_dictionary_mut(id).map_err(|_| damaged())?,
        None => doc
            .get_dictionary_mut(page)
            .map_err(|_| damaged())?
            .get_mut(b"Resources")
            .and_then(Object::as_dict_mut)
            .map_err(|_| damaged())?,
    };
    if !matches!(resources.get(b"XObject"), Ok(Object::Dictionary(_))) {
        resources.set("XObject", Object::Dictionary(Dictionary::new()));
    }
    resources
        .get_mut(b"XObject")
        .and_then(Object::as_dict_mut)
        .map_err(|_| damaged())
}

/// a copy of the resources a page inherits from its parents.
fn inherited_resources(doc: &Document, page: &Dictionary) -> Option<Dictionary> {
    let mut node = page;
    for _ in 0..64 {
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        node = doc.get_dictionary(parent).ok()?;
        if let Some(resources) = dict_dict(doc, node, b"Resources") {
            return Some(resources.clone());
        }
    }
    None
}
