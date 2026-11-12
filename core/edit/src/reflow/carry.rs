//! carrying the lines of a paragraph that no longer fit to the top of the
//! same column on the next page, pushing that column's content down.
//!
//! the carried lines are written as new text on the next page in the
//! paragraph's own font objects, size, spacing and colour space, so they
//! look like the lines they continue. only one page is pushed: when that
//! page then runs past its own limit, it is reported, not carried again.

use crate::draft::Draft;
use crate::error::EditError;
use crate::geometry::Matrix;
use crate::layout::{self, PageModel};
use crate::rewrite::{self, Plan};
use crate::summary::ChangesSummary;
use crate::text_write::Placed;

use super::column::{self, Band, ColumnInputs};
use super::paragraph::{Paragraph, SetText};
use super::push;
use super::standalone::{standalone_text, Placement};
use super::ReflowOptions;

/// a page without content in the band starts its column one inch down,
/// the most common top margin.
const EMPTY_COLUMN_TOP: f32 = 72.0;

/// the lines to carry and how they were set.
pub(crate) struct Carried<'a> {
    pub paragraph: &'a Paragraph,
    pub set: &'a SetText,
    /// the first line that goes to the next page
    pub first_line: usize,
    /// baseline distance in the paragraph's text space and on screen
    pub line_step: f32,
    pub leading: f32,
    pub band: Band,
}

/// writes the carried lines on the next page and pushes its column down.
pub(crate) fn carry(
    draft: &mut Draft,
    model: &PageModel,
    carried: &Carried,
    options: &ReflowOptions,
    summary: &mut ChangesSummary,
) -> Result<(), EditError> {
    let next = layout::build(draft.doc, model.page + 1)?;
    let objects = next.objects();
    let inputs = ColumnInputs {
        doc: draft.doc,
        caller_limit: None,
        look_at_neighbour_pages: options.look_at_neighbour_pages,
    };
    let (top, column) = column::column_on_page(
        draft,
        &next,
        &objects,
        carried.band,
        &inputs,
        EMPTY_COLUMN_TOP,
    );
    let line_count = carried.set.lines.len() - carried.first_line;
    let height = line_count as f32 * carried.leading;
    let mut plan = Plan::default();
    push::push(draft, &next, &mut plan, &column.below, height, summary)?;
    let text = carried_text(draft, model, &next, &mut plan, carried, top);
    plan.append(text.as_bytes());
    rewrite::apply(draft, &next.run, plan)?;
    summary.page(next.page);
    summary.text_lines_moved += line_count;
    if is_tagged(draft.doc) {
        summary.note("The carried lines aren’t in the document’s tags yet, so check the reading order for screen readers.");
    }
    let lowest = column.below.lowest.unwrap_or(top) + height;
    if lowest > column.limit {
        summary.note(format!(
            "Page {} now runs {:.1} points past the bottom of its column.",
            next.page + 1,
            lowest - column.limit
        ));
    }
    Ok(())
}

/// whether the document has a structure tree (it is tagged).
fn is_tagged(doc: &lopdf::Document) -> bool {
    doc.catalog()
        .is_ok_and(|catalog| catalog.has(b"StructTreeRoot"))
}

/// the page-space change that takes a point of this page to where the
/// first carried line's baseline lands on the next page.
fn landing_change(model: &PageModel, next: &PageModel, carried: &Carried, top: f32) -> Matrix {
    let anchor = &model.run.glyphs[carried.paragraph.anchor_glyph];
    let first_baseline = model.run.space.visible_point(anchor.origin).y;
    let offset_from_top = first_baseline - carried.paragraph.rect.y;
    let carried_baseline = first_baseline + carried.first_line as f32 * carried.leading;
    let dy = top + offset_from_top - carried_baseline;
    let to_visible = model.run.space.to_visible_matrix();
    let to_next = next.run.space.to_user_matrix();
    to_visible.then(&Matrix::translate(0.0, dy)).then(&to_next)
}

/// the drawing instructions for the carried lines.
fn carried_text(
    draft: &mut Draft,
    model: &PageModel,
    next: &PageModel,
    plan: &mut Plan,
    carried: &Carried,
    top: f32,
) -> String {
    let glyphs: Vec<Placed> = carried
        .set
        .placed
        .iter()
        .filter(|glyph| glyph.line >= carried.first_line)
        .cloned()
        .collect();
    let placement = Placement {
        model,
        target: next,
        paragraph: carried.paragraph,
        line_step: carried.line_step,
        change: landing_change(model, next, carried, top),
    };
    standalone_text(draft, plan, &placement, &glyphs)
}
