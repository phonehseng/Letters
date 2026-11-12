//! fields that share a name: grouping fields under one parent, and one
//! field shown on several pages (one answer, a widget per page).

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::add::{check_name, ensure_form, place_widget};
use crate::appearance::refresh_field;
use crate::error::{FormError, FormResult};
use crate::objects::{
    annotation_pages, check_document, get_dict, get_rect, page_frame, page_id, page_ids,
    push_to_array, rect_object, references_in, remove_from_array, text_string,
};
use crate::read::widget_page;
use crate::tree::{collect_fields, find_field};

/// puts top-level fields under one parent, so they share the start of
/// their names: "street" and "city" in the group "address" become
/// "address.street" and "address.city". returns the new names.
pub fn group_fields(doc: &mut Document, group: &str, fields: &[String]) -> FormResult<Vec<String>> {
    check_document(doc)?;
    check_name(group)?;
    let form = ensure_form(doc)?;
    let top_level = references_in(doc, doc.get_dictionary(form)?, b"Fields");
    let mut members = Vec::new();
    for wanted in fields {
        let node = find_field(doc, wanted)?;
        if node.lineage.len() > 1 || !top_level.contains(&node.id) {
            return Err(FormError::WrongValueKind {
                field: wanted.clone(),
                hint: "Only fields that aren’t already in a group can be grouped.".into(),
            });
        }
        members.push(node.id);
    }
    let taken = collect_fields(doc)
        .iter()
        .any(|node| node.name == group || node.name.starts_with(&format!("{group}.")));
    if taken {
        return Err(FormError::NameTaken(group.to_owned()));
    }
    let mut parent = Dictionary::new();
    parent.set("T", text_string(group));
    parent.set(
        "Kids",
        Object::Array(members.iter().map(|id| Object::Reference(*id)).collect()),
    );
    let parent_id = doc.add_object(Object::Dictionary(parent));
    for member in &members {
        remove_from_array(doc, form, b"Fields", *member)?;
        doc.get_dictionary_mut(*member)?
            .set("Parent", Object::Reference(parent_id));
    }
    push_to_array(doc, form, b"Fields", Object::Reference(parent_id))?;
    Ok(fields
        .iter()
        .map(|field| format!("{group}.{field}"))
        .collect())
}

/// shows a field on more pages, at the same place: one field (so one
/// answer), with a widget on each page, as "duplicate across pages" does.
/// pages that already show the field are skipped. returns the pages added.
pub fn duplicate_across_pages(
    doc: &mut Document,
    field_name: &str,
    pages: &[u32],
) -> FormResult<Vec<u32>> {
    check_document(doc)?;
    let field = find_field(doc, field_name)?;
    let source = field.widgets[0];
    let source_dict = doc.get_dictionary(source)?.clone();
    let located = annotation_pages(doc);
    let page_list = page_ids(doc);
    let source_page = widget_page(doc, source, &located, &page_list).ok_or_else(|| {
        FormError::WrongValueKind {
            field: field_name.to_owned(),
            hint: "It isn’t on any page.".into(),
        }
    })?;
    let rect = get_rect(doc, &source_dict, b"Rect").ok_or(FormError::EmptyRect)?;
    let visible = page_frame(doc, page_list[source_page as usize]).to_visible_rect(&rect);
    let shown_on: Vec<u32> = field
        .widgets
        .iter()
        .filter_map(|widget| widget_page(doc, *widget, &located, &page_list))
        .collect();
    let widget_keys: [&[u8]; 9] = [
        b"Type", b"Subtype", b"F", b"MK", b"BS", b"Border", b"AS", b"DA", b"Q",
    ];

    // a field that is its own widget is split into a field and its widget
    let field_id = if field.widgets == [field.id] {
        split_merged_widget(doc, field.id)?
    } else {
        field.id
    };
    let mut added = Vec::new();
    for page in pages {
        if shown_on.contains(page) || added.contains(page) {
            continue;
        }
        let target = page_id(doc, *page)?;
        let frame = page_frame(doc, target);
        let mut widget = Dictionary::new();
        for key in widget_keys {
            if let Ok(value) = source_dict.get(key) {
                widget.set(key.to_vec(), value.clone());
            }
        }
        widget.set("Rect", rect_object(&frame.to_user_rect(&visible)));
        widget.set("P", Object::Reference(target));
        widget.set("Parent", Object::Reference(field_id));
        // a button's states are named in its drawing; keep a placeholder
        // so the redraw below knows the on state
        if let Some(appearances) = get_dict(doc, &source_dict, b"AP") {
            let mut copy = Dictionary::new();
            if let Some(states) =
                crate::objects::get(doc, appearances, b"N").and_then(|n| n.as_dict().ok())
            {
                let mut names = Dictionary::new();
                for (state, value) in states.iter() {
                    names.set(state.clone(), value.clone());
                }
                copy.set("N", Object::Dictionary(names));
            }
            if !copy.is_empty() {
                widget.set("AP", Object::Dictionary(copy));
            }
        }
        let id = doc.add_object(Object::Dictionary(widget));
        crate::objects::push_to_array(doc, field_id, b"Kids", Object::Reference(id))?;
        place_widget(doc, target, id)?;
        added.push(*page);
    }
    let field = find_field(doc, field_name)?;
    refresh_field(doc, &field, false)?;
    Ok(added)
}

/// turns a merged field-and-widget dictionary into a field with one kid
/// widget, keeping the object id of the field. returns the field's id.
fn split_merged_widget(doc: &mut Document, merged: ObjectId) -> FormResult<ObjectId> {
    let dict = doc.get_dictionary(merged)?.clone();
    let widget_only: [&[u8]; 13] = [
        b"Type",
        b"Subtype",
        b"Rect",
        b"P",
        b"F",
        b"MK",
        b"BS",
        b"Border",
        b"AP",
        b"AS",
        b"StructParent",
        b"OC",
        b"H",
    ];
    let mut widget = Dictionary::new();
    let mut field = dict.clone();
    for key in widget_only {
        if let Ok(value) = dict.get(key) {
            widget.set(key.to_vec(), value.clone());
            field.remove(key);
        }
    }
    widget.set("Parent", Object::Reference(merged));
    let widget_id = doc.add_object(Object::Dictionary(widget));
    field.set("Kids", Object::Array(vec![Object::Reference(widget_id)]));
    *doc.get_dictionary_mut(merged)? = field;
    // the page's annotation list now points at the widget instead
    for page in page_ids(doc) {
        let annotations = references_in(doc, doc.get_dictionary(page)?, b"Annots");
        if let Some(position) = annotations.iter().position(|id| *id == merged) {
            let mut replaced: Vec<Object> =
                annotations.into_iter().map(Object::Reference).collect();
            replaced[position] = Object::Reference(widget_id);
            doc.get_dictionary_mut(page)?
                .set("Annots", Object::Array(replaced));
        }
    }
    Ok(merged)
}
