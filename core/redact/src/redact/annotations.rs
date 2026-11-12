//! annotations (comments, links, stamps) and form field widgets that lie
//! in a redaction area.
//!
//! an annotation is not part of the page's content: it floats above it,
//! with its own appearance, its own text (/Contents), maybe a popup and
//! replies. one that touches an area is removed completely: from the
//! page's /Annots, its popup and replies with it, and the objects
//! themselves are emptied so nothing else that still points at them (the
//! structure tree, an outline) can keep their text alive.
//!
//! a form field widget is an annotation too. removing it also removes the
//! field's value (/V, /DV, /RV), because the value is the text that was
//! typed in, and removes the field itself once it has no widgets left.

use std::collections::HashSet;

use lopdf::{Document, Object, ObjectId};

use crate::geometry::Rect;
use crate::objects::{as_dict, dict_array, dict_get, dict_name, numbers, resolve, text_string};

/// one annotation that lies in an area.
#[derive(Clone, Debug)]
pub struct AnnotationHit {
    /// none for an annotation written directly into /Annots
    pub id: Option<ObjectId>,
    /// its place in the page's /Annots array
    pub index: usize,
    /// the number of the area it lies in
    pub area: usize,
    /// what it is, for people: "Comment “Call me”", "Link", "Form field “ID”"
    pub description: String,
    /// the form field value it shows, if it is a field widget
    pub field_value: Option<String>,
}

/// what removing annotations did.
#[derive(Clone, Debug, Default)]
pub struct AnnotationRemoval {
    pub annotations: usize,
    pub field_values: usize,
    pub removed_ids: HashSet<ObjectId>,
    pub warnings: Vec<String>,
}

/// the annotations on a page whose /Rect overlaps an area (areas in
/// default user space, the space /Rect is in).
pub fn find_hits(
    doc: &Document,
    page_id: ObjectId,
    areas: &[Rect],
    area_ids: &[usize],
) -> Vec<AnnotationHit> {
    let Ok(page) = doc.get_dictionary(page_id) else {
        return Vec::new();
    };
    let Some(annots) = dict_array(doc, page, b"Annots") else {
        return Vec::new();
    };
    let mut hits = Vec::new();
    for (index, entry) in annots.iter().enumerate() {
        let Some(annot) = as_dict(resolve(doc, entry)) else {
            continue;
        };
        let Some(rect) = dict_array(doc, annot, b"Rect").map(|a| numbers(doc, a)) else {
            continue;
        };
        let [x0, y0, x1, y1] = rect[..] else {
            continue;
        };
        let bounds = Rect::from_corners(x0, y0, x1, y1);
        // a popup is shown and hidden with its parent, so it goes with it
        // rather than on its own
        if dict_name(doc, annot, b"Subtype") == Some(b"Popup") {
            continue;
        }
        let area = areas.iter().position(|area| {
            area.overlaps(&bounds) || (bounds.is_empty() && area.contains(&bounds))
        });
        if let Some(area) = area {
            hits.push(AnnotationHit {
                id: entry.as_reference().ok(),
                index,
                area: area_ids[area],
                description: describe(doc, annot),
                field_value: field_value(doc, annot),
            });
        }
    }
    hits
}

fn describe(doc: &Document, annot: &lopdf::Dictionary) -> String {
    let subtype = dict_name(doc, annot, b"Subtype").unwrap_or(b"");
    let contents = text_of(doc, annot, b"Contents");
    match subtype {
        b"Link" => "Link".to_owned(),
        b"Widget" => match field_name(doc, annot) {
            Some(name) => format!("Form field “{name}”"),
            None => "Form field".to_owned(),
        },
        _ => {
            let kind = match subtype {
                b"Text" | b"FreeText" => "Comment",
                b"Highlight" | b"Underline" | b"StrikeOut" | b"Squiggly" => "Text markup",
                b"Stamp" => "Stamp",
                b"Ink" => "Drawing",
                b"FileAttachment" => "Attached file",
                _ => "Annotation",
            };
            match contents {
                Some(text) if !text.trim().is_empty() => format!("{kind} “{}”", text.trim()),
                _ => kind.to_owned(),
            }
        }
    }
}

fn text_of(doc: &Document, dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    match dict_get(doc, dict, key)? {
        Object::String(bytes, _) => Some(text_string(bytes)),
        Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
        _ => None,
    }
}

/// a field's name, looked up through its parents.
fn field_name(doc: &Document, widget: &lopdf::Dictionary) -> Option<String> {
    let mut node = widget;
    for _ in 0..32 {
        if let Some(name) = text_of(doc, node, b"T") {
            return Some(name);
        }
        node = as_dict(dict_get(doc, node, b"Parent")?)?;
    }
    None
}

/// the value a widget's field holds, looked up through its parents.
fn field_value(doc: &Document, widget: &lopdf::Dictionary) -> Option<String> {
    if dict_name(doc, widget, b"Subtype") != Some(b"Widget") {
        return None;
    }
    let mut node = widget;
    for _ in 0..32 {
        if let Some(value) = text_of(doc, node, b"V") {
            return Some(value);
        }
        node = as_dict(dict_get(doc, node, b"Parent")?)?;
    }
    None
}

/// removes the annotations that were hit, with their popups and replies.
pub fn remove_hits(
    doc: &mut Document,
    page_id: ObjectId,
    hits: &[AnnotationHit],
) -> AnnotationRemoval {
    let mut removal = AnnotationRemoval::default();
    if hits.is_empty() {
        return removal;
    }
    let annots = page_annots(doc, page_id);
    let mut doomed: HashSet<usize> = hits.iter().map(|hit| hit.index).collect();
    removal.removed_ids = hits.iter().filter_map(|hit| hit.id).collect();
    add_popups_and_replies(doc, &annots, &mut doomed, &mut removal.removed_ids);
    removal.annotations = doomed.len();
    for &index in &doomed {
        if let Some(Object::Reference(id)) = annots.get(index) {
            if is_widget(doc, *id) {
                removal.field_values += remove_widget_from_field(doc, *id, &mut removal.warnings);
            }
        }
    }
    let kept: Vec<Object> = annots
        .iter()
        .enumerate()
        .filter(|(index, _)| !doomed.contains(index))
        .map(|(_, entry)| entry.clone())
        .collect();
    set_page_annots(doc, page_id, kept);
    // empty the objects themselves, so nothing that still refers to them
    // keeps their text, appearance or value in the file
    for id in &removal.removed_ids {
        doc.objects.insert(*id, Object::Null);
    }
    removal
}

fn page_annots(doc: &Document, page_id: ObjectId) -> Vec<Object> {
    doc.get_dictionary(page_id)
        .ok()
        .and_then(|page| dict_array(doc, page, b"Annots"))
        .cloned()
        .unwrap_or_default()
}

fn set_page_annots(doc: &mut Document, page_id: ObjectId, annots: Vec<Object>) {
    if let Ok(page) = doc.get_dictionary_mut(page_id) {
        if annots.is_empty() {
            page.remove(b"Annots");
        } else {
            // written directly on the page: a shared /Annots array would
            // otherwise change other pages too
            page.set("Annots", Object::Array(annots));
        }
    }
}

/// popups belong to the annotation they show, and replies (/IRT) to the
/// one they answer; both go when it goes. repeated until nothing new is
/// found, because a reply can have its own popup and replies.
fn add_popups_and_replies(
    doc: &Document,
    annots: &[Object],
    doomed: &mut HashSet<usize>,
    ids: &mut HashSet<ObjectId>,
) {
    loop {
        let before = doomed.len();
        for (index, entry) in annots.iter().enumerate() {
            if doomed.contains(&index) {
                if let Some(popup) = as_dict(resolve(doc, entry))
                    .and_then(|a| a.get(b"Popup").ok())
                    .and_then(|p| p.as_reference().ok())
                {
                    ids.insert(popup);
                }
                continue;
            }
            let Some(annot) = as_dict(resolve(doc, entry)) else {
                continue;
            };
            let points_at_doomed = [b"IRT".as_slice(), b"Parent"]
                .iter()
                .filter_map(|key| annot.get(key).ok()?.as_reference().ok())
                .any(|target| ids.contains(&target));
            let is_doomed_popup = entry.as_reference().is_ok_and(|id| ids.contains(&id));
            let is_annotation_parent = dict_name(doc, annot, b"Subtype") != Some(b"Widget");
            if (points_at_doomed && is_annotation_parent) || is_doomed_popup {
                doomed.insert(index);
                if let Ok(id) = entry.as_reference() {
                    ids.insert(id);
                }
            }
        }
        if doomed.len() == before {
            return;
        }
    }
}

fn is_widget(doc: &Document, id: ObjectId) -> bool {
    doc.get_dictionary(id)
        .map(|annot| dict_name(doc, annot, b"Subtype") == Some(b"Widget"))
        .unwrap_or(false)
}

/// the keys that hold a field's value: the value, the default value, and
/// the rich text value.
const VALUE_KEYS: [&[u8]; 3] = [b"V", b"DV", b"RV"];

/// takes a widget out of its field, clears the field's value and removes
/// the field once no widget is left. returns 1 when a value was removed.
fn remove_widget_from_field(
    doc: &mut Document,
    widget_id: ObjectId,
    warnings: &mut Vec<String>,
) -> usize {
    let field_id = field_of_widget(doc, widget_id);
    let mut removed_value = 0;
    for node in field_and_ancestors(doc, field_id) {
        if let Ok(dict) = doc.get_dictionary_mut(node) {
            for key in VALUE_KEYS {
                if dict.remove(key).is_some() {
                    removed_value = 1;
                }
            }
        }
    }
    if field_id != widget_id {
        remove_from_array(doc, field_id, b"Kids", widget_id);
        let other_widgets = dict_array_of(doc, field_id, b"Kids").len();
        if other_widgets > 0 {
            warnings.push(
                "A form field you redacted is also shown somewhere else in the document. Its value was removed, but the other place may still show it until the form is filled again."
                    .to_owned(),
            );
            return removed_value;
        }
    }
    remove_field(doc, field_id);
    removed_value
}

/// a widget is its own field when it carries field entries; otherwise its
/// /Parent is the field.
fn field_of_widget(doc: &Document, widget_id: ObjectId) -> ObjectId {
    let Ok(widget) = doc.get_dictionary(widget_id) else {
        return widget_id;
    };
    if widget.has(b"T") || widget.has(b"FT") {
        return widget_id;
    }
    widget
        .get(b"Parent")
        .ok()
        .and_then(|parent| parent.as_reference().ok())
        .unwrap_or(widget_id)
}

fn field_and_ancestors(doc: &Document, field_id: ObjectId) -> Vec<ObjectId> {
    let mut chain = vec![field_id];
    let mut current = field_id;
    for _ in 0..32 {
        let parent = doc
            .get_dictionary(current)
            .ok()
            .and_then(|dict| dict.get(b"Parent").ok())
            .and_then(|parent| parent.as_reference().ok());
        match parent {
            // a parent with a value holds it for its children (radio
            // buttons, fields shown in several places), so it goes too
            Some(parent) if !chain.contains(&parent) => {
                let holds_value = doc.get_dictionary(parent).is_ok_and(|dict| dict.has(b"V"));
                if !holds_value {
                    break;
                }
                chain.push(parent);
                current = parent;
            }
            _ => break,
        }
    }
    chain
}

fn dict_array_of(doc: &Document, id: ObjectId, key: &[u8]) -> Vec<Object> {
    doc.get_dictionary(id)
        .ok()
        .and_then(|dict| dict_array(doc, dict, key))
        .cloned()
        .unwrap_or_default()
}

fn remove_from_array(doc: &mut Document, owner: ObjectId, key: &[u8], target: ObjectId) {
    let items: Vec<Object> = dict_array_of(doc, owner, key)
        .into_iter()
        .filter(|item| item.as_reference().ok() != Some(target))
        .collect();
    if let Ok(dict) = doc.get_dictionary_mut(owner) {
        dict.set(key.to_vec(), Object::Array(items));
    }
}

/// takes a field out of its parent's /Kids or the form's /Fields and the
/// calculation order, then empties it.
fn remove_field(doc: &mut Document, field_id: ObjectId) {
    let parent = doc
        .get_dictionary(field_id)
        .ok()
        .and_then(|dict| dict.get(b"Parent").ok())
        .and_then(|parent| parent.as_reference().ok());
    match parent {
        Some(parent) => remove_from_array(doc, parent, b"Kids", field_id),
        None => {
            if let Some(form_id) = acro_form_id(doc) {
                remove_from_array(doc, form_id, b"Fields", field_id);
                remove_from_array(doc, form_id, b"CO", field_id);
            } else if let Ok(catalog) = doc.catalog_mut() {
                if let Ok(Object::Dictionary(form)) = catalog.get_mut(b"AcroForm") {
                    for key in [b"Fields".as_slice(), b"CO"] {
                        if let Ok(Object::Array(items)) = form.get_mut(key) {
                            items.retain(|item| item.as_reference().ok() != Some(field_id));
                        }
                    }
                }
            }
        }
    }
    doc.objects.insert(field_id, Object::Null);
}

fn acro_form_id(doc: &Document) -> Option<ObjectId> {
    doc.catalog()
        .ok()?
        .get(b"AcroForm")
        .ok()?
        .as_reference()
        .ok()
}
