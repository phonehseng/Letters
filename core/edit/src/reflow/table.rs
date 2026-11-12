//! editing the text of a table cell: the text is re-set to the cell's
//! width and, when it needs more height, the row grows. rows, rules and
//! content below move down; vertical rules that cross the row's bottom
//! edge are lengthened by rewriting their own coordinates.
//!
//! a table is found from its rules: straight line paths and thin filled
//! rectangles, upright on screen, as most writers draw ruled tables. a
//! grid drawn as one many-segment path is a free shape here and isn't
//! recognised.

use lopdf::Document;

use crate::draft::Draft;
use crate::error::EditError;
use crate::geometry::{Point, Rect};
use crate::layout::{self, PageModel};
use crate::model::{PageObjects, PathShape};
use crate::paths;
use crate::rewrite::{self, Plan};
use crate::same_font;
use crate::structure;
use crate::summary::ChangesSummary;

use super::column::{self, Band, Column, ColumnInputs};
use super::paragraph::{self, Spacing};
use super::{
    block_at, check_text, kerning, push, text_options, write_paragraph, OverflowChoice,
    OverflowOption, OverflowReport, ReflowOptions, ReflowResult, Setting,
};

/// a rectangle this thin (points) counts as a rule, not a box.
const RULE_THICKNESS: f32 = 3.0;
/// a rule shorter than this is a tick or a bullet, not part of a grid.
const MIN_RULE_LENGTH: f32 = 10.0;
/// how far (points) text may sit from the rule that encloses it, and how
/// far rules may miss each other at corners.
const RULE_SLACK: f32 = 2.0;

/// one straight rule on screen.
#[derive(Clone, Copy, Debug)]
struct Rule {
    path: usize,
    horizontal: bool,
    /// the y of a horizontal rule or the x of a vertical one
    at: f32,
    /// where it starts and ends along its length
    start: f32,
    end: f32,
}

impl Rule {
    fn spans(&self, from: f32, to: f32) -> bool {
        self.start <= from + RULE_SLACK && self.end >= to - RULE_SLACK
    }
}

/// a rule from a path, when the path is a straight upright line or a
/// thin rectangle.
fn rule_of(objects: &PageObjects, path: usize) -> Option<Rule> {
    let object = &objects.paths[path];
    let rect = object.rect;
    let straight = match object.shape {
        PathShape::Line => object.points.len() == 2,
        PathShape::Rectangle => rect.width.min(rect.height) <= RULE_THICKNESS,
        _ => false,
    };
    if !straight || rect.width.max(rect.height) < MIN_RULE_LENGTH {
        return None;
    }
    let horizontal = rect.width >= rect.height;
    Some(if horizontal {
        Rule {
            path,
            horizontal,
            at: rect.y + rect.height / 2.0,
            start: rect.x,
            end: rect.right(),
        }
    } else {
        Rule {
            path,
            horizontal,
            at: rect.x + rect.width / 2.0,
            start: rect.y,
            end: rect.bottom(),
        }
    })
}

/// the cell around a text box: the nearest rules above, below, left and
/// right of it that enclose it.
#[derive(Clone, Copy, Debug)]
struct Cell {
    rect: Rect,
    /// the left and right ends of the rule under the cell: the table's width
    table: Band,
}

fn cell_around(rules: &[Rule], text: &Rect) -> Option<Cell> {
    let across = |rule: &&Rule| rule.horizontal && rule.spans(text.x, text.right());
    let top = rules
        .iter()
        .filter(across)
        .filter(|r| r.at <= text.y + RULE_SLACK)
        .max_by(|a, b| a.at.total_cmp(&b.at))?;
    let bottom = rules
        .iter()
        .filter(across)
        .filter(|r| r.at >= text.bottom() - RULE_SLACK)
        .min_by(|a, b| a.at.total_cmp(&b.at))?;
    let down = |rule: &&Rule| !rule.horizontal && rule.spans(top.at, bottom.at);
    let left = rules
        .iter()
        .filter(down)
        .filter(|r| r.at <= text.x + RULE_SLACK)
        .max_by(|a, b| a.at.total_cmp(&b.at))?;
    let right = rules
        .iter()
        .filter(down)
        .filter(|r| r.at >= text.right() - RULE_SLACK)
        .min_by(|a, b| a.at.total_cmp(&b.at))?;
    Some(Cell {
        rect: Rect::new(left.at, top.at, right.at - left.at, bottom.at - top.at),
        table: Band {
            left: bottom.start,
            right: bottom.end,
        },
    })
}

/// the ruled cell around a text block (on screen), when it sits in one: the
/// app uses this to send a cell's text to `edit_table_cell` and any other
/// block's to `reflow_paragraph`. changes nothing.
pub fn table_cell_of(objects: &PageObjects, block_id: &str) -> Option<Rect> {
    let block = objects
        .text_blocks
        .iter()
        .find(|block| block.id == block_id)?;
    let rules: Vec<Rule> = (0..objects.paths.len())
        .filter_map(|path| rule_of(objects, path))
        .collect();
    cell_around(&rules, &block.rect).map(|cell| cell.rect)
}

/// re-sets the text of a table cell (the text block `block_id`) and grows
/// its row when the text needs more height.
pub fn edit_table_cell(
    doc: &mut Document,
    page: usize,
    block_id: &str,
    text: &str,
    options: &ReflowOptions,
) -> Result<ReflowResult, EditError> {
    check_text(text)?;
    structure::guard(doc, &options.consent)?;
    let model = layout::build(doc, page)?;
    let objects = model.objects();
    let block = model.find_block(block_id)?;
    let text_rect = objects.text_blocks[block].rect;
    let rules: Vec<Rule> = (0..objects.paths.len())
        .filter_map(|path| rule_of(&objects, path))
        .collect();
    let cell = cell_around(&rules, &text_rect).ok_or_else(|| {
        EditError::InvalidValue(
            "This text isn’t inside a ruled table cell. Edit it as a paragraph instead.".into(),
        )
    })?;
    let draft = Draft::new(doc);
    let outcome = grow_cell(draft, &model, &objects, block, text, &cell, &rules, options)?;
    let applied = outcome.changes.is_some();
    if let Some(changes) = outcome.changes {
        changes.commit(doc);
    }
    let mut summary = outcome.summary;
    let objects = if applied {
        summary.page(page);
        layout::build(doc, page)?.objects()
    } else {
        objects
    };
    Ok(ReflowResult {
        applied,
        block_id: applied
            .then(|| block_at(&objects, text_rect.x, text_rect.y))
            .flatten(),
        objects,
        notices: outcome.notices,
        summary,
        lines_before: outcome.lines_before,
        lines_after: outcome.lines_after,
        height_change: outcome.row_growth,
        overflow: outcome.overflow,
    })
}

/// what growing a cell worked out.
struct CellOutcome {
    changes: Option<crate::draft::Changes>,
    summary: ChangesSummary,
    notices: Vec<crate::text_fonts::FontNotice>,
    lines_before: usize,
    lines_after: usize,
    row_growth: f32,
    overflow: Option<OverflowReport>,
}

#[allow(clippy::too_many_arguments)]
fn grow_cell(
    mut draft: Draft,
    model: &PageModel,
    objects: &PageObjects,
    block: usize,
    text: &str,
    cell: &Cell,
    rules: &[Rule],
    options: &ReflowOptions,
) -> Result<CellOutcome, EditError> {
    let text_rect = objects.text_blocks[block].rect;
    let paragraph = cell_paragraph(model, block, text_rect, cell);
    let row_bottom = cell.rect.bottom();
    let row = Rect::new(
        cell.table.left,
        cell.rect.y,
        cell.table.right - cell.table.left,
        cell.rect.height - RULE_SLACK,
    );
    let inputs = ColumnInputs {
        doc: draft.doc,
        caller_limit: options.bottom_limit,
        look_at_neighbour_pages: options.look_at_neighbour_pages,
    };
    let column = column::below_area(&draft, model, objects, &row, cell.table, &inputs);
    let (choice, extension) = same_font::choose_same_font(
        model,
        &mut draft,
        paragraph.anchor_glyph,
        text,
        &text_options(options),
    )?;
    let font = model.run.glyphs[paragraph.anchor_glyph].font;
    let kerning = kerning::kerning_pairs(model, font);
    let setting = Setting {
        model,
        paragraph: &paragraph,
        choice: &choice,
        kerning: &kerning,
        column: &column,
        text,
    };
    let trial = setting.trial(Spacing::NORMAL, false);
    // the row grows only by what the text needs beyond its old padding
    let bottom_padding = row_bottom - text_rect.bottom();
    let needed_bottom = text_rect.bottom() + trial.height_change + bottom_padding;
    let row_growth = (needed_bottom - row_bottom).max(0.0);
    let lowest = column.below.lowest.unwrap_or(row_bottom);
    let overflow_amount = lowest + row_growth - column.limit;
    let mut outcome = CellOutcome {
        changes: None,
        summary: ChangesSummary {
            fonts_extended: extension.into_iter().collect(),
            ..ChangesSummary::default()
        },
        notices: choice.notices.clone(),
        lines_before: paragraph.lines_before,
        lines_after: trial.set.lines.len(),
        row_growth,
        overflow: None,
    };
    if overflow_amount > 0.01 {
        let report = cell_overflow(overflow_amount, &column, options);
        let allowed = match options.overflow {
            OverflowChoice::Leave => true,
            OverflowChoice::AllowIntoMargin => overflow_amount <= options.margin_allowance,
            _ => false,
        };
        outcome.overflow = Some(report);
        if !allowed {
            return Ok(outcome);
        }
    }
    let mut plan = Plan::default();
    write_paragraph(draft.doc, &setting, &mut plan, &trial, None)?;
    if row_growth > 0.0 {
        push::push(
            &mut draft,
            model,
            &mut plan,
            &column.below,
            row_growth,
            &mut outcome.summary,
        )?;
        stretch_rules(
            model,
            objects,
            &mut plan,
            rules,
            row_bottom,
            row_growth,
            &mut outcome.summary,
        )?;
    }
    rewrite::apply(&mut draft, &model.run, plan)?;
    outcome.changes = Some(draft.finish());
    Ok(outcome)
}

/// the cell's text as a paragraph as wide as the cell: the text keeps
/// the inset it had from the cell's left rule on the right side too.
fn cell_paragraph(
    model: &PageModel,
    block: usize,
    text_rect: Rect,
    cell: &Cell,
) -> paragraph::Paragraph {
    let mut paragraph = paragraph::paragraph_of(model, block, text_rect);
    let inset = text_rect.x - cell.rect.x;
    let wanted_right = cell.rect.right() - inset;
    paragraph.right = paragraph.left + (wanted_right - text_rect.x) / paragraph.along_scale;
    paragraph
}

/// lengthens the vertical rules that cross the row's bottom edge.
fn stretch_rules(
    model: &PageModel,
    objects: &PageObjects,
    plan: &mut Plan,
    rules: &[Rule],
    row_bottom: f32,
    growth: f32,
    summary: &mut ChangesSummary,
) -> Result<(), EditError> {
    let crossing = rules.iter().filter(|rule| {
        !rule.horizontal
            && rule.start < row_bottom - RULE_SLACK
            && rule.end > row_bottom - RULE_SLACK
    });
    for rule in crossing {
        let points = lengthened(&objects.paths[rule.path].points, growth);
        paths::reshape(model, plan, rule.path, &points)?;
        summary.paths_reshaped += 1;
    }
    Ok(())
}

/// a vertical rule's points with its lower end (or lower corners) moved
/// down: line ends, or rectangle corners top left, top right, bottom
/// left, bottom right.
fn lengthened(points: &[Point], growth: f32) -> Vec<Point> {
    let lowest = points.iter().map(|point| point.y).fold(f32::MIN, f32::max);
    points
        .iter()
        .map(|point| {
            if (point.y - lowest).abs() < 0.01 {
                Point::new(point.x, point.y + growth)
            } else {
                *point
            }
        })
        .collect()
}

fn cell_overflow(amount: f32, column: &Column, options: &ReflowOptions) -> OverflowReport {
    OverflowReport {
        amount,
        bottom_limit: column.limit,
        limit_reason: column.reason,
        choices: vec![
            OverflowOption {
                choice: OverflowChoice::AllowIntoMargin,
                possible: amount <= options.margin_allowance,
                detail: format!("Let the table run {amount:.1} points into the bottom margin."),
            },
            OverflowOption {
                choice: OverflowChoice::Leave,
                possible: true,
                detail: "Make the change and let the table run past the bottom; fix it by hand."
                    .into(),
            },
        ],
    }
}
