//! putting pages in a new order, moving a few of them, and deleting them.

use std::collections::HashSet;

use lopdf::{Document, ObjectId};
use serde::{Deserialize, Serialize};

use crate::cleanup::forget_pages;
use crate::tree::{check_selection, page_ids, set_page_order};
use crate::PagesError;

/// settings for `reorder`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderParams {
    /// every current page index, in the order the pages should now come
    pub new_order: Vec<u32>,
}

/// settings for `move_pages`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePagesParams {
    /// the pages being dragged
    pub pages: Vec<u32>,
    /// where they are dropped: before the page now at this index, or at
    /// the end when it equals the page count
    pub before_index: u32,
}

/// settings for `delete`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteParams {
    pub pages: Vec<u32>,
}

/// puts every page in a new order. bookmarks and links keep leading to
/// the same pages, wherever those pages end up.
pub fn reorder(doc: &mut Document, params: &ReorderParams) -> Result<(), PagesError> {
    let pages = page_ids(doc)?;
    if params.new_order.len() != pages.len() {
        return Err(PagesError::NotAPermutation);
    }
    let positions =
        check_selection(&params.new_order, pages.len()).map_err(|error| match error {
            PagesError::PageChosenTwice(_) => PagesError::NotAPermutation,
            other => other,
        })?;
    let order: Vec<ObjectId> = positions.iter().map(|&position| pages[position]).collect();
    set_page_order(doc, &order)
}

/// moves some pages, as a block in their current order, to just before
/// another page. this is what dragging thumbnails does.
pub fn move_pages(doc: &mut Document, params: &MovePagesParams) -> Result<(), PagesError> {
    let pages = page_ids(doc)?;
    let mut moving = check_selection(&params.pages, pages.len())?;
    let before = params.before_index as usize;
    if before > pages.len() {
        return Err(PagesError::NoSuchPosition);
    }
    moving.sort_unstable();
    let moving_set: HashSet<usize> = moving.iter().copied().collect();
    // dropping onto one of the moved pages means "before the next page
    // that stays", so look for the first page that isn't moving
    let anchor = (before..pages.len()).find(|position| !moving_set.contains(position));
    let mut order: Vec<ObjectId> = (0..pages.len())
        .filter(|position| !moving_set.contains(position))
        .map(|position| pages[position])
        .collect();
    let insert_at = match anchor {
        Some(anchor) => order
            .iter()
            .position(|&page| page == pages[anchor])
            .unwrap_or(order.len()),
        None => order.len(),
    };
    let block = moving.iter().map(|&position| pages[position]);
    order.splice(insert_at..insert_at, block);
    set_page_order(doc, &order)
}

/// deletes pages. bookmarks and links that led to them are removed, form
/// fields that only had boxes on them are removed, and fonts or pictures
/// only they used are dropped from the file.
pub fn delete(doc: &mut Document, params: &DeleteParams) -> Result<(), PagesError> {
    let pages = page_ids(doc)?;
    let doomed = check_selection(&params.pages, pages.len())?;
    if doomed.len() == pages.len() {
        return Err(PagesError::CannotDeleteEveryPage);
    }
    let doomed: HashSet<usize> = doomed.into_iter().collect();
    let (removed, kept): (Vec<_>, Vec<_>) = pages
        .iter()
        .enumerate()
        .partition(|(position, _)| doomed.contains(position));
    let removed: HashSet<ObjectId> = removed.into_iter().map(|(_, &page)| page).collect();
    let kept: Vec<ObjectId> = kept.into_iter().map(|(_, &page)| page).collect();
    set_page_order(doc, &kept)?;
    forget_pages(doc, &removed, &kept);
    Ok(())
}
