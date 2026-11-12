//! the wizard's engine: one small function per fix.
//!
//! every fix takes the open document, checks everything it needs before
//! changing anything, and answers with the changes it made, worded for
//! the person. run the checker again afterwards to see the new state.

mod annotations;
mod content;
mod figures;
mod headings;
mod metadata;
mod order;
mod tables;

pub use annotations::{
    add_link_text, set_annotation_contents, set_field_tooltip, set_tab_order, tag_annotations,
};
pub use content::{apply_text_colour, tag_untagged_content};
pub use figures::{mark_as_artifact, set_alt_text};
pub use headings::{
    add_bookmarks_from_headings, fix_heading_levels, propose_heading_levels, set_heading_level,
    HeadingChange,
};
pub use metadata::{mark_as_tagged, set_language, set_title, show_title};
pub use order::{auto_reading_order, set_reading_order};
pub use tables::{set_table_headers, TableHeaders};

use lopdf::{Document, ObjectId};

use crate::error::A11yError;
use crate::objects::ObjectRef;
use crate::structure::StructTree;

/// the tree and the index of an element in it, or a clear error.
pub(crate) fn find_element(
    doc: &Document,
    element: ObjectRef,
) -> Result<(StructTree, usize), A11yError> {
    let tree = StructTree::read(doc).ok_or(A11yError::NotTagged)?;
    let index = tree.find(element.id()).ok_or(A11yError::NoSuchElement)?;
    Ok((tree, index))
}

/// text the person typed, trimmed, refusing an empty answer.
pub(crate) fn answer(text: &str, what: &str) -> Result<String, A11yError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(A11yError::InvalidInput(format!("Write the {what} first.")));
    }
    if text.chars().count() > 5000 {
        return Err(A11yError::InvalidInput(format!(
            "The {what} is very long. Keep it under 5,000 characters; longer descriptions belong in the document itself."
        )));
    }
    Ok(text.to_owned())
}

/// the page object for a 0-based page index.
pub(crate) fn page_id(doc: &Document, page: u32) -> Result<ObjectId, A11yError> {
    crate::objects::page_ids(doc)
        .get(page as usize)
        .copied()
        .ok_or(A11yError::NoSuchPage(page + 1))
}

/// the object that holds an element's kids list: its parent element, or
/// the root for a top-level element.
pub(crate) fn parent_object(tree: &StructTree, index: usize) -> Option<ObjectId> {
    match tree.elements[index].parent {
        Some(parent) => tree.elements[parent].id,
        None => tree.root,
    }
}
