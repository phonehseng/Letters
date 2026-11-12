//! flattening: turning filled fields into ordinary page drawing.

use std::collections::BTreeMap;

use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::appearance::refresh_field;
use crate::edit::detach_field;
use crate::error::FormResult;
use crate::geometry::{Matrix, PdfRect};
use crate::objects::{
    acroform_id, add_page_xobject, annotation_pages, append_page_content, catalog_id,
    check_document, get, get_dict, get_integer, get_name, get_rect, page_ids, rect_of,
    remove_from_array, resolve,
};
use crate::read::{kind_of, FieldKind};
use crate::tree::{collect_fields, FieldNode};

/// which fields to flatten.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FlattenOptions {
    /// full names of the fields to flatten; none means every field
    pub fields: Option<Vec<String>>,
    /// signature fields are kept unless this is set, so the document can
    /// still be signed (or keeps showing who signed)
    pub include_signatures: bool,
}

/// what flattening did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlattenReport {
    pub flattened: Vec<String>,
}

/// what flattening does, in one sentence the ui can show as it is.
pub const FLATTEN_EXPLANATION: &str = "Flattening makes your answers a permanent part of the page: they look exactly the same, but can no longer be changed as form fields.";

/// flattening makes the answers part of the page itself, so they look the
/// same everywhere but can no longer be changed as form fields. (the same
/// sentence is in `FLATTEN_EXPLANATION` for the ui.)
///
/// each field's drawing is painted into its page's content exactly where
/// the widget was, then the widget and the field are removed. hidden
/// widgets are removed without being painted. when no fields are left the
/// form itself (/AcroForm) is removed too.
pub fn flatten(doc: &mut Document, options: &FlattenOptions) -> FormResult<FlattenReport> {
    check_document(doc)?;
    let chosen: Vec<FieldNode> = collect_fields(doc)
        .into_iter()
        .filter(|field| match &options.fields {
            Some(names) => names.contains(&field.name),
            None => true,
        })
        .filter(|field| {
            options.include_signatures || kind_of(doc, field) != Some(FieldKind::Signature)
        })
        .collect();

    let pages = annotation_pages(doc);
    let page_list = page_ids(doc);
    let mut drawings: BTreeMap<ObjectId, String> = BTreeMap::new();
    let mut report = FlattenReport::default();

    for field in &chosen {
        if needs_drawing(doc, field) {
            refresh_field(doc, field, false)?;
        }
        for widget in &field.widgets {
            let Some(page) = pages.get(widget).map(|index| page_list[*index as usize]) else {
                continue;
            };
            if let Some(content) = paint_widget(doc, page, *widget)? {
                drawings.entry(page).or_default().push_str(&content);
            }
            remove_from_array(doc, page, b"Annots", *widget)?;
        }
        detach_field(doc, field)?;
        report.flattened.push(field.name.clone());
    }
    for (page, content) in drawings {
        append_page_content(doc, page, content.as_bytes())?;
    }
    drop_empty_form(doc)?;
    Ok(report)
}

/// draws the fields again from their values, so every widget has a drawing
/// letters made. with `only_missing`, fields that already have drawings
/// are left alone. useful for files that leave drawing to the reader
/// (/NeedAppearances), whose answers otherwise look different in every app.
/// signed signature fields are never touched.
pub fn redraw_fields(doc: &mut Document, only_missing: bool) -> FormResult<Vec<String>> {
    check_document(doc)?;
    let mut redrawn = Vec::new();
    for field in collect_fields(doc) {
        if !only_missing || needs_drawing(doc, &field) {
            refresh_field(doc, &field, false)?;
            redrawn.push(field.name.clone());
        }
    }
    Ok(redrawn)
}

/// whether any widget of the field lacks a drawing to paint.
fn needs_drawing(doc: &Document, field: &FieldNode) -> bool {
    field
        .widgets
        .iter()
        .any(|widget| normal_appearance(doc, *widget).is_none())
}

/// the drawing a widget shows now: its /AP /N, picked by /AS when it has
/// several states.
fn normal_appearance(doc: &Document, widget: ObjectId) -> Option<ObjectId> {
    let dict = doc.get_dictionary(widget).ok()?;
    let appearances = get_dict(doc, dict, b"AP")?;
    match appearances.get(b"N").ok()? {
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Stream(_) => Some(*id),
            Object::Dictionary(states) => state_stream(doc, dict, states),
            _ => None,
        },
        Object::Dictionary(states) => state_stream(doc, dict, states),
        _ => None,
    }
}

fn state_stream(
    doc: &Document,
    widget: &lopdf::Dictionary,
    states: &lopdf::Dictionary,
) -> Option<ObjectId> {
    let state = get_name(doc, widget, b"AS")?;
    let id = states.get(&state).ok()?.as_reference().ok()?;
    matches!(doc.get_object(id), Ok(Object::Stream(_))).then_some(id)
}

/// the drawing instructions that paint one widget's appearance on its page.
fn paint_widget(
    doc: &mut Document,
    page: ObjectId,
    widget: ObjectId,
) -> FormResult<Option<String>> {
    let dict = doc.get_dictionary(widget)?;
    let annotation_flags = get_integer(doc, dict, b"F").unwrap_or(0);
    // 2 = hidden, 32 = not shown on screen: such widgets are never seen
    if annotation_flags & (2 | 32) != 0 {
        return Ok(None);
    }
    let Some(rect) = get_rect(doc, dict, b"Rect") else {
        return Ok(None);
    };
    let Some(stream_id) = normal_appearance(doc, widget) else {
        return Ok(None);
    };
    let Some(placement) = placement_matrix(doc, stream_id, &rect) else {
        return Ok(None);
    };
    let name = add_page_xobject(doc, page, "LettersFlat", stream_id)?;
    Ok(Some(format!(
        "q {} cm /{name} Do Q\n",
        placement.to_operands()
    )))
}

/// where pdf readers put an appearance: its box, moved by its own /Matrix,
/// is stretched to fill the widget's /Rect (pdf reference, 12.5.5). the
/// form's /Matrix itself is applied by `Do`, so only the stretch is returned.
fn placement_matrix(doc: &Document, stream_id: ObjectId, rect: &PdfRect) -> Option<Matrix> {
    let stream = doc.get_object(stream_id).ok()?.as_stream().ok()?;
    let bbox = stream
        .dict
        .get(b"BBox")
        .ok()
        .and_then(|value| rect_of(doc, value))?;
    let matrix = match get(doc, &stream.dict, b"Matrix") {
        Some(Object::Array(values)) if values.len() == 6 => {
            let v: Vec<f32> = values
                .iter()
                .filter_map(|value| crate::objects::number_of(resolve(doc, value)))
                .collect();
            (v.len() == 6).then(|| Matrix::new(v[0], v[1], v[2], v[3], v[4], v[5]))?
        }
        _ => Matrix::IDENTITY,
    };
    let moved = matrix.apply_rect(&bbox);
    if moved.width() <= 0.0 || moved.height() <= 0.0 {
        return None;
    }
    let scale_x = rect.width() / moved.width();
    let scale_y = rect.height() / moved.height();
    Some(Matrix::new(
        scale_x,
        0.0,
        0.0,
        scale_y,
        rect.left - moved.left * scale_x,
        rect.bottom - moved.bottom * scale_y,
    ))
}

/// a form with no fields left is removed so readers don't offer it.
fn drop_empty_form(doc: &mut Document) -> FormResult<()> {
    let Some(form_id) = acroform_id(doc)? else {
        return Ok(());
    };
    let empty =
        crate::objects::references_in(doc, doc.get_dictionary(form_id)?, b"Fields").is_empty();
    if empty {
        let catalog = catalog_id(doc)?;
        doc.get_dictionary_mut(catalog)?.remove(b"AcroForm");
        doc.objects.remove(&form_id);
    }
    Ok(())
}
