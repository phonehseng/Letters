//! alt text, and marking things as decoration.

use std::collections::{BTreeMap, HashSet};

use lopdf::{Document, Object, ObjectId};

use crate::content::{read_page, Mark};
use crate::error::A11yError;
use crate::geometry::PageFrame;
use crate::objects::{dict_mut, ensure_editable, text_object, ObjectRef};
use crate::report::{ContentRef, FixResult, Target};
use crate::rewrite::{
    mcids_to_artifacts, page_operations, replace_content, wrap_units, ArtifactKind, Unit, Wrap,
};
use crate::structure::Kid;
use crate::tagwriter::TagWriter;

use super::{answer, find_element, page_id, parent_object};

/// gives a picture its alt text. for a tagged element the text is stored
/// as its /Alt; for a picture with no tag, the picture is wrapped in a new
/// Figure tag carrying the text, added at the end of the tags.
pub fn set_alt_text(
    doc: &mut Document,
    target: Target,
    text: &str,
) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let text = answer(text, "alt text")?;
    match target {
        Target::Element { element } => {
            let (tree, index) = find_element(doc, element)?;
            let id = tree.elements[index].id.ok_or(A11yError::NoSuchElement)?;
            dict_mut(doc, id)?.set("Alt", text_object(&text));
            Ok(FixResult::one(format!(
                "Set the alt text of {} to “{text}”.",
                tree.path(index)
            )))
        }
        Target::Image { image } => tag_image(doc, image, &text),
    }
}

/// what is drawn at a place in a page's content, as the reader sees it.
fn image_mark(doc: &Document, image: ContentRef) -> Result<(ObjectId, Mark), A11yError> {
    let page = page_id(doc, image.page)?;
    let frame = PageFrame::read(doc, page);
    let content =
        read_page(doc, page, &frame).map_err(|_| A11yError::UnreadableContent(image.page + 1))?;
    let found = content
        .images
        .iter()
        .find(|use_| use_.op_index == image.op_index && !use_.in_form)
        .ok_or(A11yError::NoSuchImage)?;
    Ok((page, found.mark))
}

fn tag_image(doc: &mut Document, image: ContentRef, text: &str) -> Result<FixResult, A11yError> {
    let (page, mark) = image_mark(doc, image)?;
    match mark {
        Mark::Untagged => {}
        Mark::Mcid(mcid) => {
            // already tagged: describe the tag that owns it
            let tree = crate::structure::StructTree::read(doc).ok_or(A11yError::NotTagged)?;
            let owner = *tree.content_owners().get(&(page, mcid)).ok_or(A11yError::NoSuchElement)?;
            let element = tree.elements[owner].id.ok_or(A11yError::NoSuchElement)?;
            return set_alt_text(doc, Target::Element { element: element.into() }, text);
        }
        Mark::Artifact => {
            return Err(A11yError::InvalidInput(
                "This picture is marked as decoration, so screen readers skip it. Remove the decoration mark first if it carries meaning.".into(),
            ))
        }
    }
    if crate::structure::StructTree::read(doc).is_none() {
        return Err(A11yError::NotTagged);
    }
    let mcid = wrap_one(
        doc,
        page,
        image,
        Wrap::Tag {
            target: 0,
            role: "Figure".into(),
        },
    )?;
    let mut writer = TagWriter::open(doc)?;
    let container = writer.top_container();
    let figure = writer.new_element("Figure", container, Some(page));
    if let Ok(dict) = dict_mut(writer.doc, figure) {
        dict.set("Alt", text_object(text));
    }
    if let Some(mcid) = mcid {
        writer.add_content(figure, page, mcid);
    }
    writer.finish()?;
    Ok(FixResult {
        changes: vec![
            format!(
                "Tagged the picture on page {} as a figure with the alt text “{text}”.",
                image.page + 1
            ),
            "The new figure tag is at the end of the tags; check its place in the reading order."
                .into(),
        ],
    })
}

/// wraps one operator of a page's content and answers the new mcid,
/// when the wrap was a tag.
fn wrap_one(
    doc: &mut Document,
    page: ObjectId,
    at: ContentRef,
    wrap: Wrap,
) -> Result<Option<i64>, A11yError> {
    let operations = page_operations(doc, page, at.page)?;
    let frame = PageFrame::read(doc, page);
    let content =
        read_page(doc, page, &frame).map_err(|_| A11yError::UnreadableContent(at.page + 1))?;
    let mut next_mcid = content.max_mcid().map_or(0, |m| m + 1);
    let unit = Unit {
        start: at.op_index,
        end: at.op_index,
        wrap,
    };
    let (operations, owners) = wrap_units(&operations, &[unit], &mut next_mcid, false);
    replace_content(doc, page, operations)?;
    Ok(owners.first().map(|(mcid, _)| *mcid))
}

/// marks a tag, or a picture with no tag, as decoration. its content
/// becomes an artifact that screen readers skip, and the tag (with any
/// tags inside it) is taken out of the structure tree.
pub fn mark_as_artifact(doc: &mut Document, target: Target) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    match target {
        Target::Image { image } => {
            let (page, mark) = image_mark(doc, image)?;
            match mark {
                Mark::Artifact => Ok(FixResult::one(
                    "The picture was already marked as decoration.",
                )),
                Mark::Untagged => {
                    wrap_one(doc, page, image, Wrap::Artifact(ArtifactKind::Layout))?;
                    Ok(FixResult::one(format!(
                        "Marked the picture on page {} as decoration.",
                        image.page + 1
                    )))
                }
                Mark::Mcid(mcid) => {
                    let tree =
                        crate::structure::StructTree::read(doc).ok_or(A11yError::NotTagged)?;
                    let owner = *tree
                        .content_owners()
                        .get(&(page, mcid))
                        .ok_or(A11yError::NoSuchElement)?;
                    let element = tree.elements[owner].id.ok_or(A11yError::NoSuchElement)?;
                    mark_element_as_artifact(doc, element.into())
                }
            }
        }
        Target::Element { element } => mark_element_as_artifact(doc, element),
    }
}

fn mark_element_as_artifact(
    doc: &mut Document,
    element: ObjectRef,
) -> Result<FixResult, A11yError> {
    let (tree, index) = find_element(doc, element)?;
    let element_id = tree.elements[index].id.ok_or(A11yError::NoSuchElement)?;
    // the element and everything inside it
    let mut inside = vec![index];
    let mut cursor = 0;
    while cursor < inside.len() {
        let current = inside[cursor];
        for child in tree.children(current) {
            if !inside.contains(&child) {
                inside.push(child);
            }
        }
        cursor += 1;
    }
    if inside.iter().any(|&i| {
        tree.elements[i]
            .kids
            .iter()
            .any(|kid| matches!(kid, Kid::Object { .. }))
    }) {
        return Err(A11yError::InvalidInput(
            "This tag holds a link, comment or form field, which can’t be decoration.".into(),
        ));
    }
    let mut by_page: BTreeMap<ObjectId, HashSet<i64>> = BTreeMap::new();
    for (page, mcid) in tree.content_under(index) {
        if let Some(page) = page {
            by_page.entry(page).or_default().insert(mcid);
        }
    }
    let pages = crate::objects::page_ids(doc);
    let parent = parent_object(&tree, index).ok_or(A11yError::NoSuchElement)?;
    let path = tree.path(index);
    let removed: Vec<ObjectId> = inside.iter().filter_map(|&i| tree.elements[i].id).collect();
    // read everything first, so a page that can't be read changes nothing
    let mut rewritten = Vec::new();
    for (page, mcids) in &by_page {
        let number = pages.iter().position(|p| p == page).unwrap_or(0) as u32;
        let mut operations = page_operations(doc, *page, number)?;
        mcids_to_artifacts(&mut operations, mcids);
        rewritten.push((*page, operations));
    }
    for (page, operations) in rewritten {
        replace_content(doc, page, operations)?;
    }
    let mut writer = TagWriter::open(doc)?;
    let kids: Vec<Object> = writer
        .kids_of(parent)
        .into_iter()
        .filter(|kid| kid.as_reference().ok() != Some(element_id))
        .collect();
    writer.set_kids(parent, kids);
    writer.forget_owners(&removed);
    writer.finish()?;
    for id in removed {
        doc.objects.remove(&id);
    }
    Ok(FixResult::one(format!(
        "Marked {path} as decoration. Screen readers will skip it."
    )))
}
