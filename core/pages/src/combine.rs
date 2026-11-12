//! making new documents out of pages: extracting some pages, and merging
//! whole documents into one.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::copy::PageCopier;
use crate::links::{text_string, Destination};
use crate::outline::{self, OutlineEntry};
use crate::tree::{check_selection, page_ids, set_page_order};
use crate::PagesError;

/// settings for `extract`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractParams {
    /// the pages to take, in the order they should appear in the new file
    pub pages: Vec<u32>,
}

/// settings for `merge`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergeParams {
    /// one name per document (usually its file name), used for the
    /// bookmark that leads to its first page
    pub names: Vec<String>,
}

/// a new document holding copies of some pages, with only what they need.
/// bookmarks that lead to the chosen pages come along; the rest don't.
/// the original document is not changed.
pub fn extract(doc: &Document, params: &ExtractParams) -> Result<Document, PagesError> {
    let pages = page_ids(doc)?;
    let chosen = check_selection(&params.pages, pages.len())?;
    Ok(extract_positions(doc, &pages, &chosen))
}

/// `extract` for positions already checked.
pub(crate) fn extract_positions(doc: &Document, pages: &[ObjectId], chosen: &[usize]) -> Document {
    let mut target = empty_document(&doc.version);
    let chosen: Vec<ObjectId> = chosen.iter().map(|&position| pages[position]).collect();
    let mut copier = PageCopier::new(doc, pages, &chosen, &mut target);
    let new_pages = copier.copy_pages(&mut target);
    copier.finish_forms(&mut target);
    let entries = copied_outline(doc, &mut copier, &mut target);
    finish_document(&mut target, &new_pages, &entries);
    target
}

/// combines documents one after another into a new one.
///
/// each document gets a top level bookmark named after it that leads to
/// its first page, with that document's own bookmarks under it. the
/// documents themselves are not changed.
pub fn merge(docs: &[Document], params: &MergeParams) -> Result<Document, PagesError> {
    if docs.is_empty() {
        return Err(PagesError::NothingToMerge);
    }
    if params.names.len() != docs.len() {
        return Err(PagesError::MergeNamesMissing);
    }
    let page_lists = docs.iter().map(page_ids).collect::<Result<Vec<_>, _>>()?;
    if page_lists.iter().all(Vec::is_empty) {
        return Err(PagesError::NothingToMerge);
    }

    let version = docs
        .iter()
        .map(|doc| doc.version.as_str())
        .max()
        .unwrap_or("1.7");
    let mut target = empty_document(version);
    let mut all_pages = Vec::new();
    let mut entries = Vec::new();
    for ((doc, pages), name) in docs.iter().zip(&page_lists).zip(&params.names) {
        if pages.is_empty() {
            continue;
        }
        let mut copier = PageCopier::new(doc, pages, pages, &mut target);
        let new_pages = copier.copy_pages(&mut target);
        copier.finish_forms(&mut target);
        let children = copied_outline(doc, &mut copier, &mut target);
        entries.push(OutlineEntry {
            title: text_string(name),
            target: Some(Destination {
                page: new_pages[0],
                view: vec![Object::Name(b"Fit".to_vec())],
            }),
            style: Vec::new(),
            open: false,
            children,
        });
        all_pages.extend(new_pages);
    }
    finish_document(&mut target, &all_pages, &entries);
    Ok(target)
}

/// a document with a catalog and an empty page list.
fn empty_document(version: &str) -> Document {
    let mut doc = Document::with_version(version);
    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Kids", Object::Array(Vec::new()));
    pages.set("Count", Object::Integer(0));
    let pages = doc.add_object(Object::Dictionary(pages));
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages));
    let catalog = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog));
    doc
}

/// the source's bookmarks that lead to copied pages, pointed at the copies.
fn copied_outline(
    source: &Document,
    copier: &mut PageCopier,
    target: &mut Document,
) -> Vec<OutlineEntry> {
    let entries = outline::read(source, copier.names());
    let mut entries = outline::retarget(entries, &|page| copier.target_page(page));
    copy_styles(&mut entries, copier, target);
    copier.finish(target);
    entries
}

/// copies each entry's colour, style and action into the target.
fn copy_styles(entries: &mut [OutlineEntry], copier: &mut PageCopier, target: &mut Document) {
    for entry in entries {
        entry.style = std::mem::take(&mut entry.style)
            .into_iter()
            .filter_map(|(key, value)| Some((key, copier.copy_object(target, &value)?)))
            .collect();
        copy_styles(&mut entry.children, copier, target);
    }
}

/// puts the pages in the page list and writes the outline.
fn finish_document(target: &mut Document, pages: &[ObjectId], entries: &[OutlineEntry]) {
    // the new document's catalog was made above, so this can't fail
    let _ = set_page_order(target, pages);
    outline::replace(target, entries);
}
