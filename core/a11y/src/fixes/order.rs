//! reading order: proposing one from the layout, and applying an order.

use std::collections::{BTreeMap, HashSet};

use lopdf::{Document, Object, ObjectId};

use crate::error::A11yError;
use crate::objects::{ensure_editable, ensure_readable, ObjectRef};
use crate::report::FixResult;
use crate::rules::{page_blocks, visual_order, CheckOptions, Context};
use crate::structure::StructTree;
use crate::tagwriter::TagWriter;

use super::parent_object;

/// the blocks of a page (paragraphs, headings, lists, tables, figures) in
/// the order the layout suggests: top to bottom, one column at a time.
/// this is a proposal for the person to review before `set_reading_order`.
pub fn auto_reading_order(doc: &Document, page: u32) -> Result<Vec<ObjectRef>, A11yError> {
    ensure_readable(doc)?;
    let context = Context::read(doc, CheckOptions::default())?;
    if page >= context.page_count() {
        return Err(A11yError::NoSuchPage(page + 1));
    }
    let tree = context.tree.as_ref().ok_or(A11yError::NotTagged)?;
    let blocks = page_blocks(&context, tree, page);
    Ok(visual_order(&blocks)
        .into_iter()
        .filter_map(|b| tree.elements[blocks[b].element].id.map(ObjectRef::from))
        .collect())
}

/// puts tags into the given order. tags that share a parent swap places
/// among themselves; tags not listed stay where they are, so the rest of
/// the document is untouched.
pub fn set_reading_order(
    doc: &mut Document,
    page: u32,
    order: &[ObjectRef],
) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    super::page_id(doc, page)?;
    let tree = StructTree::read(doc).ok_or(A11yError::NotTagged)?;
    let mut seen = HashSet::new();
    // parent object → the listed kids, in the new order
    let mut by_parent: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
    for element in order {
        if !seen.insert(*element) {
            return Err(A11yError::InvalidInput(
                "A tag is listed twice in the new order.".into(),
            ));
        }
        let index = tree.find(element.id()).ok_or(A11yError::NoSuchElement)?;
        let parent = parent_object(&tree, index).ok_or(A11yError::NoSuchElement)?;
        by_parent.entry(parent).or_default().push(element.id());
    }
    let mut writer = TagWriter::open(doc)?;
    let mut moved = 0;
    for (parent, new_order) in &by_parent {
        let mut kids = writer.kids_of(*parent);
        let slots: Vec<usize> = kids
            .iter()
            .enumerate()
            .filter(|(_, kid)| kid.as_reference().is_ok_and(|id| new_order.contains(&id)))
            .map(|(slot, _)| slot)
            .collect();
        for (slot, id) in slots.iter().zip(new_order) {
            if kids[*slot].as_reference().ok() != Some(*id) {
                moved += 1;
            }
            kids[*slot] = Object::Reference(*id);
        }
        writer.set_kids(*parent, kids);
    }
    writer.finish()?;
    Ok(FixResult::one(if moved == 0 {
        format!(
            "The reading order of page {} was already in that order.",
            page + 1
        )
    } else {
        format!(
            "Changed the reading order of page {}: {moved} tags moved.",
            page + 1
        )
    }))
}
