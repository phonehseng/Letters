//! moving the content below a paragraph down (or up) the column by the
//! paragraph's change in height, every kind of object by its own mover.

use std::collections::{BTreeSet, HashSet};

use crate::draft::Draft;
use crate::error::EditError;
use crate::layout::PageModel;
use crate::moving;
use crate::rewrite::Plan;
use crate::summary::ChangesSummary;
use crate::text_edit::glyphs_of;

use super::column::Below;

/// moves everything in `below` by `dy` points down the screen.
pub(crate) fn push(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    below: &Below,
    dy: f32,
    summary: &mut ChangesSummary,
) -> Result<(), EditError> {
    let change = moving::screen_move(model, 0.0, dy);
    push_text(model, plan, &below.blocks, &change, summary);
    for image in &below.images {
        moving::move_image(model, plan, *image, &change);
        summary.images_moved += 1;
    }
    for path in &below.paths {
        // a path that also clips what follows can't move on its own; it is
        // left in place and said so
        if model.run.paths[*path].paint.clip {
            summary.note("A clipping path below the paragraph was left where it was.");
            continue;
        }
        moving::move_path(draft, model, plan, *path, &change)?;
        summary.paths_moved += 1;
    }
    moving::move_annotations(draft, model, &below.annotations, &change)?;
    summary.annotations_moved += below.annotations.len();
    Ok(())
}

/// moves the show operators of whole text blocks. an operator that also
/// draws text outside these blocks stays, so nothing else moves with it.
fn push_text(
    model: &PageModel,
    plan: &mut Plan,
    blocks: &[usize],
    change: &crate::geometry::Matrix,
    summary: &mut ChangesSummary,
) {
    let moved_glyphs: HashSet<usize> = blocks
        .iter()
        .flat_map(|block| &model.blocks[*block].lines)
        .flat_map(|line| glyphs_of(&line.chars))
        .collect();
    let shows: BTreeSet<usize> = moved_glyphs
        .iter()
        .map(|glyph| model.run.glyphs[*glyph].show)
        .collect();
    let mut kept_whole = true;
    for show in shows {
        let glyph_count = model.run.shows[show]
            .items
            .iter()
            .filter(|item| matches!(item, crate::interpret::ShowItem::Glyph(_)))
            .count();
        let moved_count = moved_glyphs
            .iter()
            .filter(|glyph| model.run.glyphs[**glyph].show == show)
            .count();
        if moved_count == glyph_count {
            moving::move_show(model, plan, show, change);
        } else {
            kept_whole = false;
        }
    }
    summary.text_lines_moved += blocks
        .iter()
        .map(|block| model.blocks[*block].lines.len())
        .sum::<usize>();
    if !kept_whole {
        summary.note("Some text below shares its drawing instructions with text elsewhere, so part of it stayed in place.");
    }
}
