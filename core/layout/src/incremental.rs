//! incremental relayout for the editor: a layout engine that remembers
//! each block's shaped, broken lines, so after an edit only the blocks
//! that changed are prepared again.
//!
//! preparing (shaping and line breaking) is most of the work; placing the
//! prepared lines on pages is quick arithmetic. a block is reused when its
//! content, its list label, the note numbers it shows, its column width
//! and the flavor are all unchanged, whatever index it moved to. placing
//! runs again over the whole document: it is cheap enough (see the report's
//! timings) that resynchronising pages early was not needed yet.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use letters_office::model::Document;

use crate::engine::{
    column_width_of, font_source, new_context, place_body, prepare_block, LayoutOptions, Prepared,
};
use crate::fonts::FontLibrary;
use crate::pages::Context;
use crate::result::LayoutResult;

/// how much the last layout could reuse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RelayoutStats {
    pub blocks: usize,
    /// blocks shaped and broken again
    pub prepared: usize,
    /// blocks taken from the cache
    pub reused: usize,
}

/// a layout engine that keeps its work between layouts of the same
/// (edited) document.
pub struct LayoutEngine {
    options: LayoutOptions,
    fonts: Option<FontLibrary>,
    /// the embedded fonts the cache was built with; a change empties it
    fonts_key: u64,
    cache: HashMap<u64, Prepared>,
    pub stats: RelayoutStats,
}

impl LayoutEngine {
    pub fn new(options: LayoutOptions) -> LayoutEngine {
        LayoutEngine {
            options,
            fonts: None,
            fonts_key: 0,
            cache: HashMap::new(),
            stats: RelayoutStats::default(),
        }
    }

    /// lays the document out, reusing every block that did not change
    /// since the last call. the result is the same as `layout` would give.
    pub fn layout(&mut self, document: &Document) -> LayoutResult {
        let fonts = self.fonts_for(document);
        let mut context = new_context(document, &self.options, fonts);
        let mut prepared = Vec::with_capacity(document.blocks.len());
        let mut used = HashMap::new();
        let mut stats = RelayoutStats {
            blocks: document.blocks.len(),
            ..RelayoutStats::default()
        };
        for index in 0..document.blocks.len() {
            let key = block_key(&context, index);
            let block = match self.cache.remove(&key) {
                Some(mut cached) => {
                    stats.reused += 1;
                    set_index(&mut cached, index as u32);
                    cached
                }
                None => {
                    stats.prepared += 1;
                    prepare_block(&mut context, index)
                }
            };
            used.insert(key, block.clone());
            prepared.push(block);
        }
        // blocks no longer in the document are forgotten
        self.cache = used;
        self.stats = stats;
        let pages = place_body(&mut context, &prepared);
        let result = crate::engine::finish_context(&mut context, pages);
        self.fonts = Some(std::mem::replace(&mut context.fonts, FontLibrary::system()));
        result
    }

    fn fonts_for(&mut self, document: &Document) -> FontLibrary {
        let embedded = crate::fonts::embedded::embedded_faces(document);
        let mut hasher = DefaultHasher::new();
        for face in &embedded {
            face.family.hash(&mut hasher);
            face.bytes.len().hash(&mut hasher);
        }
        let key = hasher.finish();
        if key != self.fonts_key || self.fonts.is_none() {
            self.fonts_key = key;
            self.cache.clear();
            self.fonts = None;
        }
        self.fonts
            .take()
            .unwrap_or_else(|| FontLibrary::new(font_source(document, &self.options)))
    }
}

/// what a prepared block depends on, as one number.
fn block_key(context: &Context, index: usize) -> u64 {
    let document = context.document;
    let block = &document.blocks[index];
    let mut hasher = DefaultHasher::new();
    format!("{block:?}").hash(&mut hasher);
    // the label depends on the paragraphs before (list counting)
    if let Some(label) = context.labels.get(&vec![index as u32]) {
        label.text.hash(&mut hasher);
    }
    // labels and notes of paragraphs inside tables too
    for (path, label) in &context.labels {
        if path.len() > 1 && path[0] == index as u32 {
            path.hash(&mut hasher);
            label.text.hash(&mut hasher);
        }
    }
    let mut numbers: Vec<(&String, &String)> = context.note_numbers.iter().collect();
    numbers.sort();
    let shown = format!("{block:?}");
    for (id, number) in numbers {
        if shown.contains(&format!("note_id: \"{id}\"")) {
            id.hash(&mut hasher);
            number.hash(&mut hasher);
        }
    }
    column_width_of(document, index).to_bits().hash(&mut hasher);
    document.section_index_of(index).hash(&mut hasher);
    hasher.finish()
}

/// a reused block may have moved: its paths start with its new index.
fn set_index(prepared: &mut Prepared, index: u32) {
    match prepared {
        Prepared::Paragraph(paragraph) => {
            if let Some(first) = paragraph.path.first_mut() {
                *first = index;
            }
        }
        Prepared::Table(table) => crate::tables::set_first_index(table, index),
        _ => {}
    }
}
