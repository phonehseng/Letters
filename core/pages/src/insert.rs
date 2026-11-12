//! adding blank pages, and pages taken from another document.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::copy::PageCopier;
use crate::tree::{check_selection, page_ids, set_page_order, Rect};
use crate::PagesError;

/// the smallest and largest page sides pdf allows, in points.
pub(crate) const PAGE_SIDE_RANGE: std::ops::RangeInclusive<f32> = 3.0..=14_400.0;

/// more blank pages than this at once is almost certainly a typing slip.
const MAX_BLANK_PAGES: u32 = 1000;

/// settings for `insert_blank`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertBlankParams {
    /// the new pages go before the page now at this index (the page count
    /// puts them at the end)
    pub at_index: u32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub count: u32,
}

/// settings for `insert_from`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertFromParams {
    pub at_index: u32,
    /// pages of the other document, in the order they should be inserted
    pub other_pages: Vec<u32>,
}

/// adds empty pages of one size.
pub fn insert_blank(doc: &mut Document, params: &InsertBlankParams) -> Result<(), PagesError> {
    let pages = page_ids(doc)?;
    let at = insert_position(params.at_index, pages.len())?;
    if !PAGE_SIDE_RANGE.contains(&params.width_pt) || !PAGE_SIDE_RANGE.contains(&params.height_pt) {
        return Err(PagesError::PageSizeOutOfRange);
    }
    if params.count == 0 || params.count > MAX_BLANK_PAGES {
        return Err(PagesError::BlankPageCountOutOfRange);
    }
    let paper = Rect {
        left: 0.0,
        bottom: 0.0,
        right: params.width_pt,
        top: params.height_pt,
    };
    let blanks: Vec<ObjectId> = (0..params.count)
        .map(|_| doc.add_object(Object::Dictionary(blank_page(paper))))
        .collect();
    set_page_order(doc, &spliced(&pages, at, blanks))
}

/// a page with a size, no resources and nothing drawn on it.
fn blank_page(paper: Rect) -> Dictionary {
    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("MediaBox", paper.to_object());
    page.set("Resources", Object::Dictionary(Dictionary::new()));
    page
}

/// copies pages from another document in, with everything they need:
/// fonts, pictures, annotations, form fields and inherited settings.
/// links on them that lead to pages not brought along are dropped.
pub fn insert_from(
    doc: &mut Document,
    other: &Document,
    params: &InsertFromParams,
) -> Result<(), PagesError> {
    let pages = page_ids(doc)?;
    let at = insert_position(params.at_index, pages.len())?;
    let other_pages = page_ids(other)?;
    let chosen = check_selection(&params.other_pages, other_pages.len())?;
    let chosen: Vec<ObjectId> = chosen
        .iter()
        .map(|&position| other_pages[position])
        .collect();

    let mut copier = PageCopier::new(other, &other_pages, &chosen, doc);
    let new_pages = copier.copy_pages(doc);
    copier.finish_forms(doc);
    set_page_order(doc, &spliced(&pages, at, new_pages))
}

fn insert_position(at_index: u32, page_count: usize) -> Result<usize, PagesError> {
    let at = at_index as usize;
    if at > page_count {
        return Err(PagesError::NoSuchPosition);
    }
    Ok(at)
}

/// the page list with `new_pages` put in at `at`.
fn spliced(pages: &[ObjectId], at: usize, new_pages: Vec<ObjectId>) -> Vec<ObjectId> {
    let mut order = pages.to_vec();
    order.splice(at..at, new_pages);
    order
}
