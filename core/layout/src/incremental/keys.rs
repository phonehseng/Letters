//! what a prepared block and the whole layout depend on, as numbers:
//! a changed number means the block (or every page) is laid out again.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use letters_office::model::{Block, Document, Inline};

use crate::engine::{column_width_of, Prepared};
use crate::pages::Context;

/// everything outside the body blocks that layout depends on: styles,
/// numbering, sections, headers and footers, notes, settings, pictures.
/// a change here places every page again (prepared blocks still reused
/// where their own key allows).
pub(super) fn globals_key(document: &Document) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_debug(&mut hasher, &document.styles);
    hash_debug(&mut hasher, &document.numbering);
    hash_debug(&mut hasher, &document.sections);
    hash_debug(&mut hasher, &document.headers_footers);
    hash_debug(&mut hasher, &document.notes);
    hash_debug(&mut hasher, &document.settings);
    for item in &document.media {
        item.id.hash(&mut hasher);
        item.bytes.len().hash(&mut hasher);
    }
    hasher.finish()
}

/// hashes a value's debug text as it is written, without keeping it.
fn hash_debug(hasher: &mut DefaultHasher, value: &dyn std::fmt::Debug) {
    struct Writer<'h>(&'h mut DefaultHasher);
    impl std::fmt::Write for Writer<'_> {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            self.0.write(text.as_bytes());
            Ok(())
        }
    }
    let _ = std::fmt::write(&mut Writer(hasher), format_args!("{value:?}"));
}

/// per block, the list labels and note numbers inside it (one pass over
/// the document, so a key costs no search through the whole document).
pub(super) struct BlockIndex {
    labels: HashMap<u32, Vec<(Vec<u32>, String)>>,
    notes: HashMap<u32, Vec<String>>,
}

impl BlockIndex {
    pub(super) fn of(document: &Document, context: &Context) -> BlockIndex {
        let mut labels: HashMap<u32, Vec<(Vec<u32>, String)>> = HashMap::new();
        for (path, label) in &context.labels {
            if let Some(first) = path.first() {
                labels
                    .entry(*first)
                    .or_default()
                    .push((path.clone(), label.text.clone()));
            }
        }
        for list in labels.values_mut() {
            list.sort();
        }
        let mut notes: HashMap<u32, Vec<String>> = HashMap::new();
        for (index, block) in document.blocks.iter().enumerate() {
            let mut ids = Vec::new();
            note_ids(std::slice::from_ref(block), &mut ids, 0);
            if !ids.is_empty() {
                let shown = ids
                    .into_iter()
                    .map(|id| {
                        let number = context.note_numbers.get(&id).cloned().unwrap_or_default();
                        format!("{id}={number}")
                    })
                    .collect();
                notes.insert(index as u32, shown);
            }
        }
        BlockIndex { labels, notes }
    }
}

fn note_ids(blocks: &[Block], out: &mut Vec<String>, depth: usize) {
    if depth > 16 {
        return;
    }
    for block in blocks {
        match block {
            Block::Paragraph(_) | Block::Heading(_) => {
                for inline in &block.paragraph().expect("paragraph").content {
                    if let Inline::NoteReference { note_id, .. } = inline {
                        out.push(note_id.clone());
                    }
                }
            }
            Block::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        note_ids(&cell.blocks, out, depth + 1);
                    }
                }
            }
            Block::TextBox(text_box) => note_ids(&text_box.blocks, out, depth + 1),
            _ => {}
        }
    }
}

/// what a prepared block depends on, as one number.
pub(super) fn block_key(context: &Context, index: &BlockIndex, block: usize) -> u64 {
    let document = context.document;
    let mut hasher = DefaultHasher::new();
    hash_debug(&mut hasher, &document.blocks[block]);
    // labels depend on the paragraphs before (list counting), notes on
    // the references before
    if let Some(labels) = index.labels.get(&(block as u32)) {
        labels.hash(&mut hasher);
    }
    if let Some(notes) = index.notes.get(&(block as u32)) {
        notes.hash(&mut hasher);
    }
    column_width_of(document, block).to_bits().hash(&mut hasher);
    document.section_index_of(block).hash(&mut hasher);
    hasher.finish()
}

/// a reused block may have moved: its paths start with its new index.
pub(super) fn set_index(prepared: &mut Prepared, index: u32) {
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
