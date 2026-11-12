//! the pages of an incremental relayout: old pages joined after the new
//! ones with their block and page numbers moved, stable line ids, and the
//! fingerprints that say which pages changed.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use letters_office::model::Document;

use super::PageChanges;
use crate::engine::Placed;
use crate::pages::{PageStart, PageState};
use crate::result::{Page, Story};

/// the newly placed pages followed by the old pages from `old_page` on,
/// moved to their new block and page numbers.
pub(super) fn join_old_pages(
    document: &Document,
    placed: Placed,
    old_body: &[PageState],
    old_starts: &[PageStart],
    old_page: usize,
    shift: isize,
) -> (Vec<PageState>, Vec<PageStart>) {
    let mut body = placed.body;
    let mut starts = placed.starts;
    let new_index = body.len();
    // a section that numbers its pages from a set value restarts counting
    let restarts_numbers = |start: &PageStart| {
        start.first_of_section
            && document
                .sections
                .get(start.section)
                .is_some_and(|section| section.page_number_start.is_some())
    };
    // the page number the met page gets now
    let met = &old_starts[old_page];
    let new_number = match body.last() {
        Some(page) if !restarts_numbers(met) => page.page.number + 1,
        _ => met.number,
    };
    let number_shift = i64::from(new_number) - i64::from(met.number);
    let mut shifting_numbers = true;
    for (offset, state) in old_body[old_page..].iter().enumerate() {
        let mut state = state.clone();
        if offset > 0
            && old_starts
                .get(old_page + offset)
                .is_some_and(restarts_numbers)
        {
            shifting_numbers = false;
        }
        state.page.index = (new_index + offset) as u32;
        if shifting_numbers {
            state.page.number = (i64::from(state.page.number) + number_shift).max(1) as u32;
        }
        shift_blocks(&mut state.page, shift);
        body.push(state);
        if let Some(start) = old_starts.get(old_page + offset) {
            let mut start = start.clone();
            start.cursor.block = (start.cursor.block as isize + shift).max(0) as usize;
            if shifting_numbers {
                start.number = (i64::from(start.number) + number_shift).max(1) as u32;
            }
            starts.push(start);
        }
    }
    (body, starts)
}

/// moves every body reference on a page by `shift` blocks.
fn shift_blocks(page: &mut Page, shift: isize) {
    if shift == 0 {
        return;
    }
    let moved = |block: u32| (block as isize + shift).max(0) as u32;
    for line in &mut page.lines {
        match &mut line.story {
            Story::Body => {
                if let Some(first) = line.block.first_mut() {
                    *first = moved(*first);
                }
            }
            Story::TextBox { block } => *block = moved(*block),
            _ => {}
        }
    }
    for image in &mut page.images {
        match &mut image.story {
            Story::Body => {
                if let Some(first) = image.block.first_mut() {
                    *first = moved(*first);
                }
            }
            Story::TextBox { block } => *block = moved(*block),
            _ => {}
        }
    }
    for frame in &mut page.frames {
        frame.block = moved(frame.block);
    }
}

/// gives every line an id: its block's stable id, its place inside the
/// block (cell path) and its number among that paragraph's lines. header,
/// footer and note lines are numbered per page and story.
pub fn assign_line_ids(pages: &mut [Page], block_ids: &[u64]) {
    let mut ordinals: HashMap<(Story, Vec<u32>, u64), u32> = HashMap::new();
    for page in pages.iter_mut() {
        for line in &mut page.lines {
            let (anchor, per_page) = match &line.story {
                Story::Body => (
                    line.block
                        .first()
                        .and_then(|b| block_ids.get(*b as usize))
                        .copied()
                        .unwrap_or(0),
                    false,
                ),
                Story::TextBox { block } => {
                    (block_ids.get(*block as usize).copied().unwrap_or(0), false)
                }
                Story::Footnote { .. } | Story::Endnote { .. } => (0, false),
                _ => (u64::from(page.index), true),
            };
            let tail: Vec<u32> = if matches!(line.story, Story::Body | Story::TextBox { .. }) {
                line.block.iter().skip(1).copied().collect()
            } else {
                line.block.clone()
            };
            let key = (line.story.clone(), tail.clone(), anchor);
            let ordinal = ordinals.entry(key).or_insert(0);
            let mut hasher = DefaultHasher::new();
            line.story.hash(&mut hasher);
            anchor.hash(&mut hasher);
            per_page.hash(&mut hasher);
            tail.hash(&mut hasher);
            ordinal.hash(&mut hasher);
            line.id = hasher.finish();
            *ordinal += 1;
        }
    }
}

/// a page's drawing, and the block numbers its lines belong to.
pub(super) fn page_fingerprints(page: &Page) -> (u64, u64) {
    let mut drawing = DefaultHasher::new();
    let mut blocks = DefaultHasher::new();
    page.number.hash(&mut drawing);
    page.width.to_bits().hash(&mut drawing);
    page.height.to_bits().hash(&mut drawing);
    for line in &page.lines {
        line.id.hash(&mut drawing);
        line.top.to_bits().hash(&mut drawing);
        line.x.to_bits().hash(&mut drawing);
        line.start.hash(&mut drawing);
        line.end.hash(&mut drawing);
        for run in &line.runs {
            run.glyphs.hash(&mut drawing);
            run.font.hash(&mut drawing);
            run.size.to_bits().hash(&mut drawing);
            for x in &run.x {
                x.to_bits().hash(&mut drawing);
            }
        }
        line.block.hash(&mut blocks);
    }
    for rect in &page.rects {
        rect.x.to_bits().hash(&mut drawing);
        rect.y.to_bits().hash(&mut drawing);
        rect.width.to_bits().hash(&mut drawing);
        rect.height.to_bits().hash(&mut drawing);
    }
    for image in &page.images {
        image.media_id.hash(&mut drawing);
        image.x.to_bits().hash(&mut drawing);
        image.y.to_bits().hash(&mut drawing);
        image.block.hash(&mut blocks);
    }
    page.frames.len().hash(&mut drawing);
    (drawing.finish(), blocks.finish())
}

pub(super) fn page_changes(previous: Option<&[(u64, u64)]>, now: &[(u64, u64)]) -> PageChanges {
    let before = previous.unwrap_or(&[]);
    let mut changes = PageChanges {
        pages_before: before.len() as u32,
        pages_after: now.len() as u32,
        ..PageChanges::default()
    };
    for (index, (drawing, blocks)) in now.iter().enumerate() {
        match before.get(index) {
            Some((old_drawing, old_blocks)) if old_drawing == drawing => {
                if old_blocks != blocks {
                    changes.renumbered.push(index as u32);
                }
            }
            _ => changes.changed.push(index as u32),
        }
    }
    changes
}
