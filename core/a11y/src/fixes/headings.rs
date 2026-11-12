//! heading levels, and bookmarks made from headings.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::error::A11yError;
use crate::objects::{
    catalog_id, dict_mut, ensure_editable, ensure_readable, get, get_dict, name, text_object,
    ObjectRef,
};
use crate::report::FixResult;
use crate::rules::{CheckOptions, Context};

use super::find_element;

/// one heading whose level would change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadingChange {
    pub element: ObjectRef,
    pub text: String,
    pub from: u8,
    pub to: u8,
}

/// sets a tag to be a heading of `level` (1 to 6).
pub fn set_heading_level(
    doc: &mut Document,
    element: ObjectRef,
    level: u8,
) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    if !(1..=6).contains(&level) {
        return Err(A11yError::InvalidInput(
            "Heading levels go from 1 to 6.".into(),
        ));
    }
    let (tree, index) = find_element(doc, element)?;
    let id = tree.elements[index].id.ok_or(A11yError::NoSuchElement)?;
    let path = tree.path(index);
    dict_mut(doc, id)?.set("S", name(&format!("H{level}")));
    Ok(FixResult::one(format!(
        "Made {path} a level {level} heading."
    )))
}

/// a consistent ladder of heading levels: the first heading becomes
/// level 1 and no heading is more than one level deeper than the one
/// before it, keeping the relative order of the levels. only headings
/// that change are listed.
///
/// each heading looks back for the nearest earlier heading with a lower
/// original level and goes one below that heading's new level, so
/// H1 H3 H3 H2 becomes H1 H2 H2 H2 and a lone H2 H4 becomes H1 H2.
pub fn propose_heading_levels(doc: &Document) -> Result<Vec<HeadingChange>, A11yError> {
    ensure_readable(doc)?;
    let context = Context::read(doc, CheckOptions::default())?;
    let tree = context.tree.as_ref().ok_or(A11yError::NotTagged)?;
    let mut changes = Vec::new();
    // (original level, new level) of the open headings above this point
    let mut ladder: Vec<(u8, u8)> = Vec::new();
    for index in tree.preorder() {
        let Some(level) = tree.elements[index].is_heading() else {
            continue;
        };
        while ladder
            .last()
            .is_some_and(|&(original, _)| original >= level)
        {
            ladder.pop();
        }
        let new_level = ladder.last().map_or(1, |&(_, above)| above + 1).min(6);
        ladder.push((level, new_level));
        if new_level != level {
            if let Some(id) = tree.elements[index].id {
                changes.push(HeadingChange {
                    element: id.into(),
                    text: context.element_text(index),
                    from: level,
                    to: new_level,
                });
            }
        }
    }
    Ok(changes)
}

/// applies `propose_heading_levels`.
pub fn fix_heading_levels(doc: &mut Document) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let changes = propose_heading_levels(doc)?;
    let mut result = FixResult::default();
    for change in &changes {
        dict_mut(doc, change.element.id())?.set("S", name(&format!("H{}", change.to)));
        result.push(format!(
            "Changed the heading “{}” from level {} to level {}.",
            crate::rules::short(&change.text),
            change.from,
            change.to
        ));
    }
    if result.changes.is_empty() {
        result.push("The heading levels were already in order.");
    }
    Ok(result)
}

/// one bookmark to write, with the bookmarks under it.
struct Entry {
    title: String,
    page: ObjectId,
    level: u8,
    children: Vec<Entry>,
}

/// adds bookmarks made from the headings in the tags, nested by level.
/// refuses when the document already has bookmarks.
pub fn add_bookmarks_from_headings(doc: &mut Document) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let catalog = catalog_id(doc)?;
    let has_bookmarks = get_dict(doc, crate::objects::catalog(doc)?, b"Outlines")
        .is_some_and(|outlines| get(doc, outlines, b"First").is_some());
    if has_bookmarks {
        return Err(A11yError::InvalidInput(
            "This document already has bookmarks.".into(),
        ));
    }
    let entries = {
        let context = Context::read(doc, CheckOptions::default())?;
        let tree = context.tree.as_ref().ok_or(A11yError::NotTagged)?;
        let mut flat = Vec::new();
        for index in tree.preorder() {
            let Some(level) = tree.elements[index].is_heading() else {
                continue;
            };
            let text = context.element_text(index);
            let page = context
                .element_content(index)
                .first()
                .map(|(page, _)| context.pages[*page as usize].id);
            if let (false, Some(page)) = (text.is_empty(), page) {
                flat.push(Entry {
                    title: text,
                    page,
                    level,
                    children: Vec::new(),
                });
            }
        }
        nest(flat)
    };
    if entries.is_empty() {
        return Err(A11yError::InvalidInput(
            "The tags have no headings with text to make bookmarks from.".into(),
        ));
    }
    let root = doc.new_object_id();
    let (first, last, count) = write_entries(doc, &entries, root);
    let mut outlines = Dictionary::new();
    outlines.set("Type", name("Outlines"));
    outlines.set("First", Object::Reference(first));
    outlines.set("Last", Object::Reference(last));
    outlines.set("Count", Object::Integer(count));
    doc.objects.insert(root, Object::Dictionary(outlines));
    dict_mut(doc, catalog)?.set("Outlines", Object::Reference(root));
    Ok(FixResult::one(format!(
        "Added {count} bookmarks made from the headings."
    )))
}

/// turns headings in order into a tree: each heading holds the deeper
/// headings that follow it.
fn nest(flat: Vec<Entry>) -> Vec<Entry> {
    let mut top: Vec<Entry> = Vec::new();
    for entry in flat {
        insert(&mut top, entry);
    }
    top
}

fn insert(siblings: &mut Vec<Entry>, entry: Entry) {
    match siblings.last_mut() {
        Some(last) if last.level < entry.level => insert(&mut last.children, entry),
        _ => siblings.push(entry),
    }
}

/// writes sibling bookmarks and their children, answering the first and
/// last object and how many entries were written.
fn write_entries(
    doc: &mut Document,
    entries: &[Entry],
    parent: ObjectId,
) -> (ObjectId, ObjectId, i64) {
    let ids: Vec<ObjectId> = entries.iter().map(|_| doc.new_object_id()).collect();
    let mut count = 0;
    for (index, entry) in entries.iter().enumerate() {
        let mut dict = Dictionary::new();
        dict.set("Title", text_object(&entry.title));
        dict.set("Parent", Object::Reference(parent));
        dict.set(
            "Dest",
            Object::Array(vec![Object::Reference(entry.page), name("Fit")]),
        );
        if index > 0 {
            dict.set("Prev", Object::Reference(ids[index - 1]));
        }
        if index + 1 < ids.len() {
            dict.set("Next", Object::Reference(ids[index + 1]));
        }
        count += 1;
        if !entry.children.is_empty() {
            let (first, last, children) = write_entries(doc, &entry.children, ids[index]);
            dict.set("First", Object::Reference(first));
            dict.set("Last", Object::Reference(last));
            dict.set("Count", Object::Integer(children));
            count += children;
        }
        doc.objects.insert(ids[index], Object::Dictionary(dict));
    }
    (ids[0], ids[ids.len() - 1], count)
}
