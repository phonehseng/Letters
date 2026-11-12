//! structure analysis: what a page's text *is*. headings, paragraphs,
//! lists, tables, pictures with captions, footnotes, links, columns and
//! reading order, and the headers and footers repeated across pages.
//!
//! the html "easy to edit" mode, the text writer, the spreadsheet writer
//! and the office crate's "easy to edit" word and odt writers all start
//! from the `StructuredDocument` made here.
//!
//! the work, page by page:
//! 1. `fragments`: letters-edit's lines cut at wide gaps into pieces of
//!    words on one baseline, with styles and links
//! 2. `repeated`: text repeated at the top or bottom of pages is set apart
//! 3. `tables`: grids of drawn lines, then text lined up in columns
//! 4. `reading_order`: columns found from their gutters, read in turn
//! 5. `blocks`: lines joined into paragraphs, headings, lists, footnotes;
//!    captions joined to their pictures and tables
//!
//! the document is read twice: once to find what repeats and the usual
//! text size, keeping only the top and bottom lines of each page, and
//! once to build the blocks. so memory holds one page's characters at a
//! time however long the document is.

mod blocks;
pub(crate) mod fonts;
pub(crate) mod fragments;
mod links;
mod markers;
pub mod model;
mod reading_order;
mod repeated;
mod tables;

use std::collections::HashMap;

use letters_ocr::{CancelFlag, Progress};
use serde::{Deserialize, Serialize};

use crate::content::read_page_drawing;
use crate::source::{PageSelection, PdfSource};
use crate::ConvertError;
use blocks::{assign_heading_levels, page_blocks, PageParts, PagePicture};
use fragments::{page_fragments, Fragment};
use model::{BlockContent, PageInfo, StructuredDocument};
use repeated::{find_repeated, PageFragments};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StructureOptions {
    pub pages: PageSelection,
    /// rejoin words split with a hyphen at the end of a line
    pub dehyphenate: bool,
    pub find_tables: bool,
    /// set headers, footers and page numbers apart from the body
    pub find_repeated: bool,
}

impl Default for StructureOptions {
    fn default() -> Self {
        Self {
            pages: PageSelection::All,
            dehyphenate: true,
            find_tables: true,
            find_repeated: true,
        }
    }
}

/// reads the structure of the chosen pages. `progress` hears after each
/// page of the main pass, and `cancel` is checked between pages.
pub fn analyze_structure(
    source: &PdfSource,
    options: &StructureOptions,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<StructuredDocument, ConvertError> {
    let pages = options.pages.resolve(source.page_count())?;
    let page_numbers = page_number_map(source);

    // first pass: the lines at the top and bottom of each page, and how
    // much text there is at each size
    let mut edges: Vec<(usize, f32, Vec<Fragment>, Vec<usize>)> = Vec::new();
    let mut size_counts: HashMap<i32, usize> = HashMap::new();
    for page in &pages {
        check_cancel(cancel)?;
        let read = read_page(source, *page, &page_numbers)?;
        for fragment in &read.fragments {
            *size_counts
                .entry((fragment.size * 2.0).round() as i32)
                .or_default() += fragment.char_count();
        }
        let (edge_indexes, edge_fragments): (Vec<usize>, Vec<Fragment>) = read
            .fragments
            .into_iter()
            .enumerate()
            .filter(|(_, f)| f.rect.bottom() <= read.height * 0.1 || f.rect.y >= read.height * 0.9)
            .unzip();
        edges.push((*page, read.height, edge_fragments, edge_indexes));
    }
    let body_size = size_counts
        .iter()
        .max_by_key(|(size, count)| (**count, -**size))
        .map(|(size, _)| *size as f32 / 2.0)
        .unwrap_or(12.0);

    let (repeated, covered) = if options.find_repeated {
        let input: Vec<PageFragments> = edges
            .iter()
            .map(|(page, height, fragments, _)| PageFragments {
                page: *page,
                height: *height,
                fragments,
            })
            .collect();
        let found = find_repeated(&input);
        let covered: Vec<(usize, usize)> = found
            .covered
            .iter()
            .map(|(page, edge_index)| {
                let (_, _, _, indexes) =
                    edges.iter().find(|e| e.0 == *page).expect("page was read");
                (*page, indexes[*edge_index])
            })
            .collect();
        (found.texts, covered)
    } else {
        (Vec::new(), Vec::new())
    };
    drop(edges);

    // second pass: the blocks of each page
    let mut document = StructuredDocument {
        title: source.title(),
        pages: Vec::new(),
        blocks: Vec::new(),
        repeated,
        body_size,
    };
    for (done, page) in pages.iter().enumerate() {
        check_cancel(cancel)?;
        let read = read_page(source, *page, &page_numbers)?;
        let skip: Vec<usize> = covered
            .iter()
            .filter(|(p, _)| p == page)
            .map(|(_, i)| *i)
            .collect();
        let (blocks, columns) = analyse_page(&read, &skip, body_size, options);
        document.pages.push(PageInfo {
            page: *page,
            width: read.width,
            height: read.height,
            columns,
        });
        document.blocks.extend(blocks);
        progress(Progress {
            page: *page,
            done: done + 1,
            total: pages.len(),
        });
    }
    assign_heading_levels(&mut document.blocks);
    if document.title.is_none() {
        document.title = document
            .blocks
            .iter()
            .find_map(|block| match &block.content {
                BlockContent::Heading { level: 1, .. } => Some(block.text()),
                _ => None,
            });
    }
    Ok(document)
}

fn check_cancel(cancel: &CancelFlag) -> Result<(), ConvertError> {
    if cancel.is_cancelled() {
        Err(ConvertError::Cancelled)
    } else {
        Ok(())
    }
}

/// what one page holds, read from the pdf.
pub(crate) struct ReadPage {
    pub width: f32,
    pub height: f32,
    pub fragments: Vec<Fragment>,
    pub drawing: crate::content::PageDrawing,
    pub objects: letters_edit::PageObjects,
}

pub(crate) fn read_page(
    source: &PdfSource,
    page: usize,
    page_numbers: &HashMap<lopdf::ObjectId, usize>,
) -> Result<ReadPage, ConvertError> {
    let objects = letters_edit::page_objects(source.document(), page)?;
    let page_id = source.page_id(page)?;
    let space = source.page_space(page)?;
    let links = links::page_links(source.document(), page_id, &space, page_numbers);
    let fragments = page_fragments(&objects, &links);
    let drawing = read_page_drawing(source.document(), page_id)?;
    Ok(ReadPage {
        width: space.width,
        height: space.height,
        fragments,
        drawing,
        objects,
    })
}

/// the page numbers of a source, for reading links.
pub(crate) fn page_number_map(source: &PdfSource) -> HashMap<lopdf::ObjectId, usize> {
    (0..source.page_count())
        .filter_map(|page| source.page_id(page).ok().map(|id| (id, page)))
        .collect()
}

fn analyse_page(
    read: &ReadPage,
    skip: &[usize],
    body_size: f32,
    options: &StructureOptions,
) -> (Vec<model::Block>, usize) {
    let mut taken = vec![false; read.fragments.len()];
    for index in skip {
        taken[*index] = true;
    }
    let mut found_tables = Vec::new();
    if options.find_tables {
        let lines = read.drawing.rule_lines();
        let fills = read.drawing.filled_boxes();
        let available: Vec<Fragment> = read
            .fragments
            .iter()
            .enumerate()
            .map(|(i, f)| if taken[i] { empty_like(f) } else { f.clone() })
            .collect();
        for table in tables::ruled_tables(&lines, &fills, &available) {
            for index in &table.used {
                taken[*index] = true;
            }
            found_tables.push(table);
        }
        for table in tables::whitespace_tables(&read.fragments, &taken) {
            for index in &table.used {
                taken[*index] = true;
            }
            found_tables.push(table);
        }
    }
    let fragments: Vec<&Fragment> = read
        .fragments
        .iter()
        .enumerate()
        .filter(|(i, _)| !taken[*i])
        .map(|(_, f)| f)
        .collect();
    let rules = read.drawing.rule_lines();
    let parts = PageParts {
        page: read.objects.page,
        height: read.height,
        width: read.width,
        fragments,
        tables: found_tables,
        pictures: page_pictures(read),
        rules: &rules,
        body_size,
        dehyphenate: options.dehyphenate,
    };
    page_blocks(parts)
}

/// a fragment with no characters, standing in for one already used so
/// indexes stay the same.
fn empty_like(fragment: &Fragment) -> Fragment {
    let mut empty = fragment.clone();
    empty.glyphs.clear();
    empty.rect = crate::geometry::Rect::new(-1e6, -1e6, 0.0, 0.0);
    empty
}

/// the page's pictures worth keeping: not tiny, and not the scan behind
/// a page's invisible text layer (that picture *is* the page).
fn page_pictures(read: &ReadPage) -> Vec<PagePicture> {
    let has_invisible_text = read.fragments.iter().any(|f| f.invisible);
    let page_area = read.width * read.height;
    read.objects
        .images
        .iter()
        .filter(|image| image.rect.width >= 8.0 && image.rect.height >= 8.0)
        .filter(|image| {
            !(has_invisible_text && image.rect.width * image.rect.height > page_area * 0.7)
        })
        .map(|image| PagePicture {
            rect: crate::geometry::Rect::from_edit(image.rect),
            image_id: image.id.clone(),
            pixel_width: image.pixel_width,
            pixel_height: image.pixel_height,
        })
        .collect()
}
