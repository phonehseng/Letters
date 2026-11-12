//! finding pages and keeping each page's /Annots list in order.
//!
//! a page lists its annotations in /Annots, which may sit in the page
//! itself or behind a reference, and in files from other programs may
//! hold direct dictionaries or references to objects that no longer
//! exist. everything that changes the list goes through here so those
//! cases are handled once.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::AnnotateError;
use crate::ids::{parse_reference_id, reference_id};
use crate::page_space::PageSpace;
use crate::pdf_values::{dict_bytes, dict_name, resolve};
use crate::text_string;

/// a page that is being read or changed, and how to measure on it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PageContext {
    pub index: usize,
    pub id: ObjectId,
    pub space: PageSpace,
}

/// refuses to work on an encrypted file: changes written into it could
/// not be read back, so it has to be unlocked first.
pub(crate) fn ensure_unlocked(doc: &Document) -> Result<(), AnnotateError> {
    if doc.is_encrypted() {
        Err(AnnotateError::Locked)
    } else {
        Ok(())
    }
}

/// looks up a page by its 0-based number.
pub(crate) fn page(doc: &Document, index: usize) -> Result<PageContext, AnnotateError> {
    ensure_unlocked(doc)?;
    let number = u32::try_from(index + 1).map_err(|_| AnnotateError::UnknownPage(index))?;
    let id = *doc
        .get_pages()
        .get(&number)
        .ok_or(AnnotateError::UnknownPage(index))?;
    let space = PageSpace::of_page(doc, id)?;
    Ok(PageContext { index, id, space })
}

/// the raw entries of a page's /Annots list, wherever it is stored.
pub(crate) fn entries(doc: &Document, page: ObjectId) -> Vec<Object> {
    let Ok(page) = doc.get_dictionary(page) else {
        return Vec::new();
    };
    match page.get(b"Annots") {
        Ok(list) => resolve(doc, list)
            .as_array()
            .map(|list| list.to_vec())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// the annotation objects on a page, in order, skipping anything in
/// the list that isn't an annotation that still exists.
pub(crate) fn annotation_ids(doc: &Document, page: ObjectId) -> Vec<ObjectId> {
    entries(doc, page)
        .iter()
        .filter_map(|entry| entry.as_reference().ok())
        .filter(|id| doc.get_dictionary(*id).is_ok())
        .collect()
}

/// gives the page's list as a mutable array, creating it if missing and
/// following it if it lives in its own object.
fn list_mut(doc: &mut Document, page: ObjectId) -> Result<&mut Vec<Object>, AnnotateError> {
    let damaged = || AnnotateError::Damaged("the page is missing".into());
    let stored_elsewhere = match doc
        .get_dictionary(page)
        .map_err(|_| damaged())?
        .get(b"Annots")
    {
        Ok(Object::Reference(id)) if doc.get_object(*id).and_then(Object::as_array).is_ok() => {
            Some(*id)
        }
        _ => None,
    };
    if let Some(list) = stored_elsewhere {
        return doc
            .get_object_mut(list)
            .and_then(Object::as_array_mut)
            .map_err(|_| damaged());
    }
    let page = doc.get_dictionary_mut(page).map_err(|_| damaged())?;
    if !matches!(page.get(b"Annots"), Ok(Object::Array(_))) {
        // a missing or broken list is replaced by an empty one
        page.set("Annots", Object::Array(Vec::new()));
    }
    page.get_mut(b"Annots")
        .and_then(Object::as_array_mut)
        .map_err(|_| damaged())
}

/// adds an annotation object to the end of the page's list, which is
/// also the top of the drawing order.
pub(crate) fn append(
    doc: &mut Document,
    page: ObjectId,
    annotation: ObjectId,
) -> Result<(), AnnotateError> {
    list_mut(doc, page)?.push(Object::Reference(annotation));
    Ok(())
}

/// removes the given annotations from the page's list, together with
/// any entries that point at objects which no longer exist.
pub(crate) fn remove(
    doc: &mut Document,
    page: ObjectId,
    removed: &HashSet<ObjectId>,
) -> Result<(), AnnotateError> {
    let kept: Vec<Object> = entries(doc, page)
        .into_iter()
        .filter(|entry| match entry {
            Object::Reference(id) => !removed.contains(id) && doc.objects.contains_key(id),
            Object::Dictionary(_) => true,
            _ => false,
        })
        .collect();
    *list_mut(doc, page)? = kept;
    Ok(())
}

/// turns annotations written straight into the list into objects of
/// their own, in place, so they can be referred to like any other.
/// popups and replies need a reference to point at.
pub(crate) fn make_entries_objects(
    doc: &mut Document,
    page: ObjectId,
) -> Result<(), AnnotateError> {
    let has_direct = entries(doc, page)
        .iter()
        .any(|entry| matches!(entry, Object::Dictionary(_)));
    if !has_direct {
        return Ok(());
    }
    let list = std::mem::take(list_mut(doc, page)?);
    let mut converted = Vec::with_capacity(list.len());
    for entry in list {
        converted.push(match entry {
            Object::Dictionary(annotation) => Object::Reference(doc.add_object(annotation)),
            other => other,
        });
    }
    *list_mut(doc, page)? = converted;
    Ok(())
}

/// the id the app uses for an annotation: its /NM, or for one without
/// a name, its object number.
pub(crate) fn id_of(doc: &Document, object: ObjectId, annotation: &Dictionary) -> String {
    match dict_bytes(doc, annotation, b"NM") {
        Some(name) if !name.is_empty() => text_string::decode(name),
        _ => reference_id(object),
    }
}

/// the id reported for an annotation written straight into the list
/// (with no object of its own and no /NM): its place in the list.
pub(crate) fn position_id(index: usize) -> String {
    format!("{POSITION_PREFIX}{index}")
}

const POSITION_PREFIX: &str = "at:";

/// finds an annotation on a page by the id `id_of` or `position_id` gave
/// it. call `make_entries_objects` first so every entry has an object.
pub(crate) fn find(doc: &Document, page: ObjectId, id: &str) -> Result<ObjectId, AnnotateError> {
    if let Some(index) = id.strip_prefix(POSITION_PREFIX) {
        let index: usize = index
            .parse()
            .map_err(|_| AnnotateError::UnknownAnnotation)?;
        return entries(doc, page)
            .get(index)
            .and_then(|entry| entry.as_reference().ok())
            .filter(|object| doc.get_dictionary(*object).is_ok())
            .ok_or(AnnotateError::UnknownAnnotation);
    }
    let candidates = annotation_ids(doc, page);
    // the object number form also finds an annotation that has since
    // been given a /NM, so an id the ui still holds keeps working
    if let Some(object) = parse_reference_id(id) {
        if candidates.contains(&object) {
            return Ok(object);
        }
    }
    candidates
        .into_iter()
        .find(|object| {
            doc.get_dictionary(*object)
                .map(|annotation| id_of(doc, *object, annotation) == id)
                .unwrap_or(false)
        })
        .ok_or(AnnotateError::UnknownAnnotation)
}

/// the annotation's /Subtype, e.g. "Highlight"; empty when missing.
pub(crate) fn subtype(doc: &Document, annotation: &Dictionary) -> String {
    dict_name(doc, annotation, b"Subtype")
        .map(|subtype| String::from_utf8_lossy(subtype).into_owned())
        .unwrap_or_default()
}

/// removes objects that only the removed annotations used, such as
/// their appearance streams. an object still used by any annotation
/// left on the page is kept, so a shared appearance is never broken.
pub(crate) fn remove_unused(doc: &mut Document, page: ObjectId, candidates: &[ObjectId]) {
    let still_used: HashSet<ObjectId> = annotation_ids(doc, page)
        .into_iter()
        .filter_map(|annotation| doc.get_dictionary(annotation).ok())
        .flat_map(crate::appearance::appearance_objects)
        .collect();
    for object in candidates {
        if !still_used.contains(object) {
            doc.objects.remove(object);
        }
    }
}
