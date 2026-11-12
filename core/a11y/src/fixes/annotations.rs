//! links, comments and form fields: descriptions, tooltips, tab order and tags.

use lopdf::{Document, ObjectId};

use crate::error::A11yError;
use crate::objects::{
    dict_mut, ensure_editable, get_array, get_name, name, page_ids, text_object, ObjectRef,
};
use crate::report::FixResult;
use crate::rules::{page_annotations, CheckOptions, Context};
use crate::structure::StructTree;
use crate::tagwriter::TagWriter;

use super::answer;

/// the annotation's subtype, or an error when it isn't an annotation.
fn subtype(doc: &Document, annotation: ObjectRef) -> Result<String, A11yError> {
    let dict = doc
        .get_dictionary(annotation.id())
        .map_err(|_| A11yError::NoSuchAnnotation)?;
    get_name(doc, dict, b"Subtype")
        .map(crate::objects::name_text)
        .ok_or(A11yError::NoSuchAnnotation)
}

/// gives a form field the name a screen reader reads out (its tooltip, /TU).
pub fn set_field_tooltip(
    doc: &mut Document,
    field: ObjectRef,
    text: &str,
) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let text = answer(text, "field name")?;
    let dict = doc
        .get_dictionary(field.id())
        .map_err(|_| A11yError::NoSuchAnnotation)?;
    // a widget without a name of its own belongs to its parent field
    let target = if dict.has(b"T") {
        field.id()
    } else {
        match dict.get(b"Parent").ok().and_then(|p| p.as_reference().ok()) {
            Some(parent) if doc.get_dictionary(parent).is_ok() => parent,
            _ if get_name(doc, dict, b"Subtype") == Some(b"Widget") || dict.has(b"FT") => {
                field.id()
            }
            _ => return Err(A11yError::NoSuchAnnotation),
        }
    };
    dict_mut(doc, target)?.set("TU", text_object(&text));
    Ok(FixResult::one(format!(
        "Gave the form field the name “{text}”."
    )))
}

/// makes the Tab key follow the tags on every page with links, comments
/// or fields.
pub fn set_tab_order(doc: &mut Document) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let pages: Vec<ObjectId> = page_ids(doc)
        .into_iter()
        .filter(|&page| {
            doc.get_dictionary(page)
                .ok()
                .and_then(|dict| get_array(doc, dict, b"Annots"))
                .is_some_and(|list| !list.is_empty())
        })
        .collect();
    for &page in &pages {
        dict_mut(doc, page)?.set("Tabs", name("S"));
    }
    Ok(FixResult::one(match pages.len() {
        0 => "No page has links, comments or fields.".to_owned(),
        1 => "Made the Tab key follow the tags on the page with links or fields.".to_owned(),
        count => {
            format!("Made the Tab key follow the tags on the {count} pages with links or fields.")
        }
    }))
}

/// gives a link a description of where it goes. the link's tag gets the
/// same text as its alt text, so a screen reader reads it instead of
/// vague words like "click here".
pub fn add_link_text(
    doc: &mut Document,
    annotation: ObjectRef,
    text: &str,
) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let text = answer(text, "link description")?;
    if subtype(doc, annotation)? != "Link" {
        return Err(A11yError::InvalidInput("That isn’t a link.".into()));
    }
    dict_mut(doc, annotation.id())?.set("Contents", text_object(&text));
    let link_tag = StructTree::read(doc).and_then(|tree| {
        let owner = *tree.object_owners().get(&annotation.id())?;
        (tree.elements[owner].role == "Link")
            .then_some(tree.elements[owner].id)
            .flatten()
    });
    if let Some(tag) = link_tag {
        dict_mut(doc, tag)?.set("Alt", text_object(&text));
    }
    Ok(FixResult::one(format!("Described the link as “{text}”.")))
}

/// gives a comment, stamp or other annotation a text description.
pub fn set_annotation_contents(
    doc: &mut Document,
    annotation: ObjectRef,
    text: &str,
) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let text = answer(text, "description")?;
    let kind = subtype(doc, annotation)?;
    dict_mut(doc, annotation.id())?.set("Contents", text_object(&text));
    Ok(FixResult::one(format!(
        "Described the {kind} annotation as “{text}”."
    )))
}

/// one annotation to tag, worked out before anything changes.
struct Planned {
    page: ObjectId,
    annotation: ObjectId,
    role: &'static str,
    /// the tag of the text it sits on, for links
    inside: Option<ObjectId>,
}

/// gives every link, comment and form field that has no tag one: Link
/// tags go inside the tag of the text they sit on, Form and Annot tags at
/// the end of the tags.
pub fn tag_annotations(doc: &mut Document) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let planned = plan(doc)?;
    if planned.is_empty() {
        return Ok(FixResult::one(
            "Every link, comment and field already had a tag.",
        ));
    }
    let mut writer = TagWriter::open(doc)?;
    let container = writer.top_container();
    let (mut links, mut others) = (0, 0);
    for item in &planned {
        let parent = item.inside.unwrap_or(container);
        let element = writer.new_element(item.role, parent, Some(item.page));
        writer.add_object(element, item.page, item.annotation);
        if item.role == "Link" {
            links += 1;
        } else {
            others += 1;
        }
    }
    writer.finish()?;
    let mut result = FixResult::default();
    if links > 0 {
        result.push(format!(
            "Added Link tags for {links} links, inside the tags of their text."
        ));
    }
    if others > 0 {
        result.push(format!(
            "Added tags for {others} comments or form fields at the end of the tags; check their place in the reading order."
        ));
    }
    Ok(result)
}

fn plan(doc: &Document) -> Result<Vec<Planned>, A11yError> {
    let context = Context::read(doc, CheckOptions::default())?;
    let tree = context.tree.as_ref().ok_or(A11yError::NotTagged)?;
    let objects = tree.object_owners();
    let content = tree.content_owners();
    let mut planned = Vec::new();
    for (page, info) in context.pages.iter().enumerate() {
        for annotation in page_annotations(doc, info.id, &info.frame) {
            let Some(id) = annotation.id else {
                continue;
            };
            if objects.contains_key(&id)
                || annotation.hidden
                || matches!(
                    annotation.subtype.as_str(),
                    "Popup" | "PrinterMark" | "TrapNet"
                )
            {
                continue;
            }
            let role = match annotation.subtype.as_str() {
                "Link" => "Link",
                "Widget" => "Form",
                _ => "Annot",
            };
            let inside = if role == "Link" {
                annotation.rect.and_then(|rect| {
                    // the smallest tagged text under the link's middle
                    context
                        .mcid_rects
                        .iter()
                        .filter(|((p, _), r)| *p == page as u32 && r.contains_point(rect.center()))
                        .min_by(|a, b| a.1.area().total_cmp(&b.1.area()))
                        .and_then(|((_, mcid), _)| content.get(&(info.id, *mcid)))
                        .and_then(|&owner| tree.elements[owner].id)
                })
            } else {
                None
            };
            planned.push(Planned {
                page: info.id,
                annotation: id,
                role,
                inside,
            });
        }
    }
    Ok(planned)
}
