//! incremental relayout for the editor.
//!
//! preparing a block (shaping and line breaking) is most of the work, so
//! the engine keeps every block's prepared lines and reuses a block whose
//! content, list label, note numbers, column width, section and flavor are
//! unchanged, whatever index it moved to.
//!
//! placing is also done only where it has to be:
//!
//! - it starts again at the last page of the previous run that began
//!   safely before the first edited block (one column, no notes carried
//!   over, ten blocks of keep-with-next room before the edit);
//! - it stops as soon as a new page begins exactly where an old page began
//!   after the edited blocks, in the same state (same line of the same
//!   block, same section, same kind of page, same break before it). from
//!   there on the old pages are taken as they were, with block numbers and
//!   page numbers moved by what the edit inserted or removed.
//!
//! headers and footers are drawn again on every page (they show the page
//! count). the result is always what `layout` would give; the property
//! test in tests/incremental.rs checks that after random edits.
//!
//! every line gets an id that stays the same while its paragraph and its
//! place in the paragraph do, so the editor can keep a caret and its line
//! elements across relayouts; `LayoutEngine::changes` says which pages
//! differ from the previous run.

mod keys;
mod pages;

pub use pages::assign_line_ids;

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use letters_office::model::Document;

use crate::engine::{
    draw_parts, font_source, new_context, place_body_with, prepare_block, LayoutOptions, Placed,
    Prepared, Resume,
};
use crate::fonts::FontLibrary;
use crate::pages::{Context, PageStart, PageState, ResyncTarget};
use crate::result::{LayoutResult, Page};
use keys::{block_key, globals_key, set_index, BlockIndex};
use pages::{join_old_pages, page_changes, page_fingerprints};

/// how much the last layout could reuse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RelayoutStats {
    pub blocks: usize,
    /// blocks shaped and broken again
    pub prepared: usize,
    /// blocks taken from the cache
    pub reused: usize,
    /// pages placed again, and pages taken from the previous run as they were
    pub pages_placed: usize,
    pub pages_reused: usize,
}

/// which pages differ from the previous layout.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageChanges {
    /// pages whose drawing changed (or are new)
    pub changed: Vec<u32>,
    /// pages that look the same but whose lines now belong to other block
    /// numbers (an edit before them inserted or removed blocks)
    pub renumbered: Vec<u32>,
    pub pages_before: u32,
    pub pages_after: u32,
}

/// what a run leaves for the next one.
#[derive(Clone)]
struct PreviousRun {
    keys: Vec<u64>,
    globals: u64,
    body: Vec<PageState>,
    starts: Vec<PageStart>,
    /// per page: (drawing fingerprint, fingerprint of its block numbers)
    fingerprints: Vec<(u64, u64)>,
    block_ids: Vec<u64>,
    result: LayoutResult,
}

/// a layout engine that keeps its work between layouts of the same
/// (edited) document.
pub struct LayoutEngine {
    options: LayoutOptions,
    fonts: Option<FontLibrary>,
    /// the embedded fonts the cache was built with; a change empties it
    fonts_key: u64,
    cache: HashMap<u64, Prepared>,
    previous: Option<PreviousRun>,
    next_block_id: u64,
    pub stats: RelayoutStats,
    pub changes: PageChanges,
}

/// blocks of keep-with-next look-ahead a resumed page must have before
/// the first edited block (`engine::need_after` follows at most 9)
const KEEP_ROOM: usize = 10;

impl LayoutEngine {
    pub fn new(options: LayoutOptions) -> LayoutEngine {
        LayoutEngine {
            options,
            fonts: None,
            fonts_key: 0,
            cache: HashMap::new(),
            previous: None,
            next_block_id: 1,
            stats: RelayoutStats::default(),
            changes: PageChanges::default(),
        }
    }

    /// lays the document out, reusing every block and page that did not
    /// change since the last call. the result is the same as `layout`.
    pub fn layout(&mut self, document: &Document) -> LayoutResult {
        let fonts = self.fonts_for(document);
        let mut context = new_context(document, &self.options, fonts);
        let index = BlockIndex::of(document, &context);
        let keys: Vec<u64> = (0..document.blocks.len())
            .map(|block| block_key(&context, &index, block))
            .collect();
        let globals = globals_key(document);
        let mut stats = RelayoutStats {
            blocks: document.blocks.len(),
            ..RelayoutStats::default()
        };
        let prepared = self.prepare_blocks(&mut context, &keys, &mut stats);
        let previous = self
            .previous
            .take()
            .filter(|previous| previous.globals == globals);
        let diff = previous
            .as_ref()
            .map(|previous| Diff::of(&previous.keys, &keys));
        let block_ids = self.block_ids(previous.as_ref(), diff.as_ref(), keys.len());
        if let (Some(previous), true) = (&previous, diff.is_some_and(|d| d.unchanged())) {
            return self.keep_previous(previous.clone(), keys, block_ids, stats, &mut context);
        }
        let (body, starts) =
            place_changed(&mut context, &prepared, previous.as_ref(), diff, &mut stats);
        let result = self.finish(&mut context, body.clone(), &block_ids);
        let fingerprints: Vec<(u64, u64)> = result.pages.iter().map(page_fingerprints).collect();
        self.changes = page_changes(
            previous.as_ref().map(|p| p.fingerprints.as_slice()),
            &fingerprints,
        );
        self.stats = stats;
        self.previous = Some(PreviousRun {
            keys,
            globals,
            body,
            starts,
            fingerprints,
            block_ids,
            result: result.clone(),
        });
        result
    }

    /// every block, from the cache where its key is there, else prepared.
    fn prepare_blocks(
        &mut self,
        context: &mut Context,
        keys: &[u64],
        stats: &mut RelayoutStats,
    ) -> Vec<Prepared> {
        let mut prepared = Vec::with_capacity(keys.len());
        let mut used = HashMap::new();
        for (block, key) in keys.iter().enumerate() {
            let item = match self.cache.remove(key) {
                Some(mut cached) => {
                    stats.reused += 1;
                    set_index(&mut cached, block as u32);
                    cached
                }
                None => {
                    stats.prepared += 1;
                    prepare_block(context, block)
                }
            };
            used.insert(*key, item.clone());
            prepared.push(item);
        }
        // blocks no longer in the document are forgotten
        self.cache = used;
        prepared
    }

    /// nothing changed since the last run: its result stands.
    fn keep_previous(
        &mut self,
        previous: PreviousRun,
        keys: Vec<u64>,
        block_ids: Vec<u64>,
        mut stats: RelayoutStats,
        context: &mut Context,
    ) -> LayoutResult {
        let count = previous.result.pages.len();
        stats.pages_reused = count;
        self.stats = stats;
        self.changes = PageChanges {
            pages_before: count as u32,
            pages_after: count as u32,
            ..PageChanges::default()
        };
        let result = previous.result.clone();
        self.fonts = Some(std::mem::replace(&mut context.fonts, FontLibrary::system()));
        self.previous = Some(PreviousRun {
            keys,
            block_ids,
            ..previous
        });
        result
    }

    /// headers, footers and line ids on the body pages, and the result.
    fn finish(
        &mut self,
        context: &mut Context,
        body: Vec<PageState>,
        block_ids: &[u64],
    ) -> LayoutResult {
        let mut pages = draw_parts(context, body);
        assign_line_ids(&mut pages, block_ids);
        let mut result = crate::engine::finish_context(context, pages);
        if self.options.word_grid_drawing {
            crate::grid::snap_to_word_grid(&mut result);
        }
        self.fonts = Some(std::mem::replace(&mut context.fonts, FontLibrary::system()));
        result
    }

    /// the pages that changed in the last layout, with their contents:
    /// what the editor redraws.
    pub fn changed_pages<'r>(&self, result: &'r LayoutResult) -> Vec<&'r Page> {
        self.changes
            .changed
            .iter()
            .filter_map(|index| result.pages.get(*index as usize))
            .collect()
    }

    /// a stable id per block: blocks before and after the edit keep their
    /// ids, edited blocks keep the id of the block at their place, and
    /// inserted blocks get new ones.
    fn block_ids(
        &mut self,
        previous: Option<&PreviousRun>,
        diff: Option<&Diff>,
        count: usize,
    ) -> Vec<u64> {
        let (Some(previous), Some(diff)) = (previous, diff) else {
            return (0..count)
                .map(|_| {
                    self.next_block_id += 1;
                    self.next_block_id
                })
                .collect();
        };
        let old = &previous.block_ids;
        let old_middle = old.len() - diff.suffix;
        let mut ids = Vec::with_capacity(count);
        for index in 0..count {
            let id = if index < diff.prefix {
                old[index]
            } else if index >= count - diff.suffix {
                old[index + old.len() - count]
            } else if diff.prefix + (index - diff.prefix) < old_middle {
                old[diff.prefix + (index - diff.prefix)]
            } else {
                self.next_block_id += 1;
                self.next_block_id
            };
            ids.push(id);
        }
        ids
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
            self.previous = None;
            self.fonts = None;
        }
        self.fonts
            .take()
            .unwrap_or_else(|| FontLibrary::new(font_source(document, &self.options)))
    }
}

/// the blocks two runs share at the start and at the end.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Diff {
    prefix: usize,
    suffix: usize,
    old_len: usize,
    new_len: usize,
}

impl Diff {
    fn of(old: &[u64], new: &[u64]) -> Diff {
        let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
        let room = old.len().min(new.len()) - prefix;
        let suffix = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(room)
            .take_while(|(a, b)| a == b)
            .count();
        Diff {
            prefix,
            suffix,
            old_len: old.len(),
            new_len: new.len(),
        }
    }

    fn unchanged(&self) -> bool {
        self.prefix == self.old_len && self.prefix == self.new_len
    }
}

/// the last page of the old run that began safely before the edit, with
/// the old pages before it.
fn resume_point(previous: &PreviousRun, diff: &Diff) -> Option<Resume> {
    let index = previous
        .starts
        .iter()
        .rposition(|start| start.resumable && start.cursor.block + KEEP_ROOM <= diff.prefix)?;
    if index == 0 || index > previous.body.len() {
        return None;
    }
    Some(Resume {
        pages: previous.body[..index].to_vec(),
        starts: previous.starts[..index].to_vec(),
        start: previous.starts[index].clone(),
    })
}

/// places the pages an edit changed: from the last safe old page start
/// before it, until a new page starts where an old one did after it, and
/// the old pages after that. without a previous run, every page.
fn place_changed(
    context: &mut Context,
    prepared: &[Prepared],
    previous: Option<&PreviousRun>,
    diff: Option<Diff>,
    stats: &mut RelayoutStats,
) -> (Vec<PageState>, Vec<PageStart>) {
    let document = context.document;
    if crate::fields::has_body_page_fields(document) {
        // as `layout` does: a first pass finds the count page fields show
        let first = place_body_with(context, prepared, None, None);
        context.page_count_hint = Some(first.body.len() as u32);
        let placed = place_body_with(context, prepared, None, None);
        stats.pages_placed = placed.body.len();
        return (placed.body, placed.starts);
    }
    let (Some(previous), Some(diff)) = (previous, diff) else {
        let placed = place_body_with(context, prepared, None, None);
        stats.pages_placed = placed.body.len();
        return (placed.body, placed.starts);
    };
    let resume = resume_point(previous, &diff);
    let resumed_pages = resume.as_ref().map_or(0, |r| r.pages.len());
    let target = ResyncTarget {
        starts: previous.starts.clone(),
        unchanged_from: diff.new_len - diff.suffix,
        shift: diff.new_len as isize - diff.old_len as isize,
        parity_matters: document.settings.even_and_odd_headers,
    };
    let shift = target.shift;
    let placed: Placed = place_body_with(context, prepared, resume, Some(target));
    stats.pages_placed = placed.body.len() - resumed_pages;
    match placed.stopped {
        Some(old_page) => {
            let joined = join_old_pages(
                document,
                placed,
                &previous.body,
                &previous.starts,
                old_page,
                shift,
            );
            stats.pages_reused = joined.0.len() - stats.pages_placed;
            joined
        }
        None => {
            stats.pages_reused = resumed_pages;
            (placed.body, placed.starts)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diff_finds_the_shared_start_and_end() {
        let diff = Diff::of(&[1, 2, 3, 4, 5], &[1, 2, 9, 4, 5]);
        assert_eq!((diff.prefix, diff.suffix), (2, 2));
        let inserted = Diff::of(&[1, 2, 3], &[1, 2, 7, 3]);
        assert_eq!((inserted.prefix, inserted.suffix), (2, 1));
        let removed = Diff::of(&[1, 2, 3, 4], &[1, 4]);
        assert_eq!((removed.prefix, removed.suffix), (1, 1));
        // shared start and end never overlap
        let same_ends = Diff::of(&[1, 1], &[1, 1, 1]);
        assert!(same_ends.prefix + same_ends.suffix <= 2);
        assert!(Diff::of(&[1, 2], &[1, 2]).unchanged());
    }

    #[test]
    fn page_changes_tell_drawing_from_renumbering() {
        let previous = [(1, 10), (2, 20), (3, 30)];
        let run = PreviousRun {
            keys: Vec::new(),
            globals: 0,
            body: Vec::new(),
            starts: Vec::new(),
            fingerprints: previous.to_vec(),
            block_ids: Vec::new(),
            result: LayoutResult::default(),
        };
        let changes = page_changes(
            Some(&run.fingerprints),
            &[(1, 10), (5, 20), (3, 31), (4, 40)],
        );
        assert_eq!(changes.changed, vec![1, 3]);
        assert_eq!(changes.renumbered, vec![2]);
        assert_eq!((changes.pages_before, changes.pages_after), (3, 4));
    }
}
