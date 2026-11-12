//! paragraph reflow inside a fixed page, the way a word processor does
//! it: the paragraph is re-set to its width and alignment, and when it
//! grows or shrinks the content below it in the same column moves by the
//! difference. when that content would run past the bottom of the column
//! the edit reports it with choices instead of doing something silent.
//!
//! | module | job |
//! |---|---|
//! | `paragraph` | the paragraph's look, and setting text into lines |
//! | `kerning` | the kerning pairs the page already uses |
//! | `column` | the column band, what lies below, the bottom limit |
//! | `footer` | repeated text at the foot of neighbouring pages |
//! | `push` | moving what lies below by the height change |
//! | `tighten` | the gentle spacing settings tried to make text fit |
//! | `carry` | moving lines that don't fit to the next page |

mod carry;
mod column;
mod footer;
mod kerning;
mod match_style;
mod paragraph;
mod push;
mod standalone;
mod table;
mod tighten;

use std::collections::{HashMap, HashSet};

use lopdf::Document;
use serde::{Deserialize, Serialize};

use crate::draft::{Changes, Draft};
use crate::error::EditError;
use crate::layout::{self, PageModel};
use crate::model::{PageObjects, TextAlignment};
use crate::rewrite::{self, Plan};
use crate::same_font;
use crate::structure;
use crate::summary::{ChangesSummary, EditConsent, Tightening};
use crate::text_edit::{glyphs_of, TextEditOptions};
use crate::text_fonts::{FontChoice, FontNotice};
use crate::text_write::{self, Anchor};

pub use column::LimitReason;
use column::{Column, ColumnInputs};
pub use match_style::{add_text_matching, MatchedText};
use paragraph::{Paragraph, SetText, Spacing, Typesetter};
pub use table::{edit_table_cell, table_cell_of};

/// what to do when the column can't hold the re-set paragraph.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OverflowChoice {
    /// change nothing and report the choices (the default)
    #[default]
    Ask,
    /// let content run into the bottom margin, up to `marginAllowance`
    AllowIntoMargin,
    /// set the paragraph a little tighter, within limits that keep its look
    Tighten,
    /// move the lines that don't fit to the top of the next page's column
    CarryToNextPage,
    /// make the edit and let the content run past the limit
    Leave,
}

/// choices for a reflow.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReflowOptions {
    pub overflow: OverflowChoice,
    /// how far (points) `allowIntoMargin` may run past the limit
    pub margin_allowance: f32,
    /// when the paragraph gets shorter, pull the content below up
    pub close_gap: bool,
    /// read the pages before and after to find a repeated footer
    pub look_at_neighbour_pages: bool,
    /// the lowest point (points from the top) the column may reach, when
    /// the app knows it better
    pub bottom_limit: Option<f32>,
    pub allow_installed_fonts: bool,
    pub allow_standard_fonts: bool,
    pub consent: EditConsent,
}

/// half an inch: the most a margin can usually give before content looks
/// like it is falling off the page.
const DEFAULT_MARGIN_ALLOWANCE: f32 = 36.0;

impl Default for ReflowOptions {
    fn default() -> Self {
        Self {
            overflow: OverflowChoice::Ask,
            margin_allowance: DEFAULT_MARGIN_ALLOWANCE,
            close_gap: false,
            look_at_neighbour_pages: true,
            bottom_limit: None,
            allow_installed_fonts: true,
            allow_standard_fonts: true,
            consent: EditConsent::default(),
        }
    }
}

/// one way out of an overflow, and whether it would work here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverflowOption {
    pub choice: OverflowChoice,
    pub possible: bool,
    /// a sentence saying what it would do, or why it can't
    pub detail: String,
}

/// content that would run past the bottom of the column.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverflowReport {
    /// points past the limit
    pub amount: f32,
    /// the limit, in points from the top of the page
    pub bottom_limit: f32,
    pub limit_reason: LimitReason,
    pub choices: Vec<OverflowOption>,
}

/// what a reflow did, or would do.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflowResult {
    /// false when nothing changed because the overflow needs a choice
    pub applied: bool,
    pub objects: PageObjects,
    pub notices: Vec<FontNotice>,
    pub summary: ChangesSummary,
    /// the paragraph's id now, when applied
    pub block_id: Option<String>,
    pub lines_before: usize,
    pub lines_after: usize,
    /// how much taller (negative: shorter) the paragraph became, points
    pub height_change: f32,
    pub overflow: Option<OverflowReport>,
}

/// everything worked out for one reflow, before it is written.
struct Prepared {
    applied: bool,
    lines_after: usize,
    height_change: f32,
    notices: Vec<FontNotice>,
    summary: ChangesSummary,
    overflow: Option<OverflowReport>,
    changes: Option<Changes>,
}

/// re-sets a paragraph's text and moves the content below it in its
/// column by the change in height. see `ReflowOptions` for overflow.
pub fn reflow_paragraph(
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
    let lines_before = model.blocks[block].lines.len();
    let prepared = prepare(doc, &model, &objects, block, text, options)?;
    let top_left = objects.text_blocks[block].rect;
    if let Some(changes) = prepared.changes {
        changes.commit(doc);
    }
    let mut summary = prepared.summary;
    let objects = if prepared.applied {
        summary.page(page);
        layout::build(doc, page)?.objects()
    } else {
        objects
    };
    let block_id = prepared
        .applied
        .then(|| block_at(&objects, top_left.x, top_left.y))
        .flatten();
    Ok(ReflowResult {
        applied: prepared.applied,
        objects,
        notices: prepared.notices,
        summary,
        block_id,
        lines_before,
        lines_after: prepared.lines_after,
        height_change: prepared.height_change,
        overflow: prepared.overflow,
    })
}

fn check_text(text: &str) -> Result<(), EditError> {
    if text.chars().any(|c| c.is_control() && c != '\n') {
        return Err(EditError::InvalidValue(
            "The text contains invisible control characters. Remove them and try again.".into(),
        ));
    }
    Ok(())
}

/// the block whose box starts nearest a point.
fn block_at(objects: &PageObjects, x: f32, y: f32) -> Option<String> {
    objects
        .text_blocks
        .iter()
        .min_by(|a, b| {
            let distance = |rect: &crate::geometry::Rect| (rect.x - x).abs() + (rect.y - y).abs();
            distance(&a.rect).total_cmp(&distance(&b.rect))
        })
        .map(|block| block.id.clone())
}

/// the pieces an evaluation of one spacing needs.
struct Setting<'a> {
    model: &'a PageModel,
    paragraph: &'a Paragraph,
    choice: &'a FontChoice,
    kerning: &'a HashMap<(char, char), f32>,
    column: &'a Column,
    text: &'a str,
}

/// one spacing tried: the lines, the height change and the overflow.
struct Trial {
    spacing: Spacing,
    set: SetText,
    height_change: f32,
    overflow: f32,
}

impl Setting<'_> {
    fn typesetter(&self, spacing: Spacing) -> Typesetter<'_> {
        Typesetter {
            model: self.model,
            paragraph: self.paragraph,
            choice: self.choice,
            spacing,
            kerning: self.kerning,
        }
    }

    fn trial(&self, spacing: Spacing, close_gap: bool) -> Trial {
        let typesetter = self.typesetter(spacing);
        let set = typesetter.set(self.text);
        let new_height = set.lines.len() as f32 * typesetter.leading();
        let old_height = self.paragraph.lines_before as f32 * self.paragraph.leading;
        let height_change = new_height - old_height;
        let lowest = self
            .column
            .below
            .lowest
            .unwrap_or(0.0)
            .max(self.paragraph.rect.bottom());
        let moves_content = height_change > 0.0 || close_gap;
        let bottom = if moves_content {
            lowest + height_change
        } else {
            lowest
        };
        Trial {
            spacing,
            set,
            height_change,
            overflow: bottom - self.column.limit,
        }
    }
}

/// works out the whole reflow in a draft; the draft's changes come back
/// only when the edit is to be applied.
fn prepare(
    doc: &Document,
    model: &PageModel,
    objects: &PageObjects,
    block: usize,
    text: &str,
    options: &ReflowOptions,
) -> Result<Prepared, EditError> {
    let mut draft = Draft::new(doc);
    let inputs = ColumnInputs {
        doc,
        caller_limit: options.bottom_limit,
        look_at_neighbour_pages: options.look_at_neighbour_pages,
    };
    let column = column::analyse(&draft, model, objects, block, &inputs);
    let rect = objects.text_blocks[block].rect;
    let paragraph = widened(paragraph::paragraph_of(model, block, rect), &column);
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
    let normal = setting.trial(Spacing::NORMAL, options.close_gap);
    let normal_lines = normal.set.lines.len();
    match decide(&setting, normal, options, model, doc) {
        Decision::Refuse(report) => Ok(Prepared {
            applied: false,
            lines_after: normal_lines,
            height_change: 0.0,
            notices: choice.notices.clone(),
            summary: ChangesSummary::default(),
            overflow: Some(report),
            changes: None,
        }),
        Decision::Apply {
            trial,
            carry,
            report,
        } => {
            let mut summary = write_all(&mut draft, &setting, &trial, carry, options)?;
            summary.fonts_extended.extend(extension);
            Ok(Prepared {
                applied: true,
                lines_after: trial.set.lines.len(),
                height_change: trial.height_change,
                notices: choice.notices.clone(),
                summary,
                overflow: report,
                changes: Some(draft.finish()),
            })
        }
    }
}

fn text_options(options: &ReflowOptions) -> TextEditOptions {
    TextEditOptions {
        reflow: true,
        allow_installed_fonts: options.allow_installed_fonts,
        allow_standard_fonts: options.allow_standard_fonts,
    }
}

/// writes the decided reflow into the draft: the paragraph, the content
/// pushed below it, and any lines carried to the next page.
fn write_all(
    draft: &mut Draft,
    setting: &Setting,
    trial: &Trial,
    carry: Option<usize>,
    options: &ReflowOptions,
) -> Result<ChangesSummary, EditError> {
    let model = setting.model;
    let mut summary = ChangesSummary::default();
    if trial.spacing != Spacing::NORMAL {
        summary.tightening = Some(Tightening {
            letter_spacing_em: trial.spacing.letter_em,
            word_spacing_scale: trial.spacing.word_scale,
            leading_scale: trial.spacing.leading_scale,
        });
    }
    let mut plan = Plan::default();
    write_paragraph(draft.doc, setting, &mut plan, trial, carry)?;
    let grows = trial.height_change > 0.0;
    let closes_gap = options.close_gap && trial.height_change < 0.0;
    if (grows || closes_gap) && carry.is_none() {
        let below = &setting.column.below;
        push::push(
            draft,
            model,
            &mut plan,
            below,
            trial.height_change,
            &mut summary,
        )?;
    }
    rewrite::apply(draft, &model.run, plan)?;
    if let Some(first_carried) = carry {
        let typesetter = setting.typesetter(trial.spacing);
        let carried = carry::Carried {
            paragraph: setting.paragraph,
            set: &trial.set,
            first_line: first_carried,
            line_step: typesetter.line_step(),
            leading: typesetter.leading(),
            band: setting.column.band,
        };
        carry::carry(draft, model, &carried, options, &mut summary)?;
    }
    Ok(summary)
}

/// a one-line paragraph has no width of its own to wrap to: it wraps at
/// its column's right edge (when it starts at the column's left edge).
fn widened(mut paragraph: Paragraph, column: &Column) -> Paragraph {
    // two points: the slack of anti-aliased box edges
    let starts_at_left = (paragraph.rect.x - column.band.left).abs() < 2.0;
    let wraps_left = matches!(
        paragraph.alignment,
        TextAlignment::Left | TextAlignment::Justified
    );
    if paragraph.lines_before == 1 && starts_at_left && wraps_left {
        let extra = (column.band.right - paragraph.rect.right()).max(0.0);
        paragraph.right += extra / paragraph.along_scale;
    }
    paragraph
}

/// what to do about the trial with normal spacing.
enum Decision {
    Apply {
        trial: Trial,
        /// the first line that goes to the next page
        carry: Option<usize>,
        report: Option<OverflowReport>,
    },
    Refuse(OverflowReport),
}

/// overflow smaller than this (points) is rounding, not overflow.
const OVERFLOW_TOLERANCE: f32 = 0.01;

fn decide(
    setting: &Setting,
    normal: Trial,
    options: &ReflowOptions,
    model: &PageModel,
    doc: &Document,
) -> Decision {
    if normal.overflow <= OVERFLOW_TOLERANCE {
        return Decision::Apply {
            trial: normal,
            carry: None,
            report: None,
        };
    }
    let report = report_for(setting, &normal, options, model, doc);
    let possible = |choice: OverflowChoice| {
        report
            .choices
            .iter()
            .any(|option| option.choice == choice && option.possible)
    };
    match options.overflow {
        OverflowChoice::Ask => Decision::Refuse(report),
        OverflowChoice::Leave => Decision::Apply {
            trial: normal,
            carry: None,
            report: Some(report),
        },
        OverflowChoice::AllowIntoMargin if possible(OverflowChoice::AllowIntoMargin) => {
            Decision::Apply {
                trial: normal,
                carry: None,
                report: Some(report),
            }
        }
        OverflowChoice::Tighten => match tightest_needed(setting, options) {
            Some(trial) => Decision::Apply {
                trial,
                carry: None,
                report: None,
            },
            None => Decision::Refuse(report),
        },
        OverflowChoice::CarryToNextPage if possible(OverflowChoice::CarryToNextPage) => {
            let keep = lines_that_fit(setting, &normal);
            Decision::Apply {
                trial: normal,
                carry: Some(keep),
                report: Some(report),
            }
        }
        _ => Decision::Refuse(report),
    }
}

fn tightest_needed(setting: &Setting, options: &ReflowOptions) -> Option<Trial> {
    let spacing = tighten::gentlest_fit(|spacing| {
        setting.trial(*spacing, options.close_gap).overflow <= OVERFLOW_TOLERANCE
    })?;
    Some(setting.trial(spacing, options.close_gap))
}

/// how many of the new lines stay on this page when the rest is carried.
fn lines_that_fit(setting: &Setting, trial: &Trial) -> usize {
    let lines_over = (trial.overflow / setting.paragraph.leading).ceil() as usize;
    trial.set.lines.len().saturating_sub(lines_over).max(1)
}

/// the overflow and what each choice would do about it.
fn report_for(
    setting: &Setting,
    normal: &Trial,
    options: &ReflowOptions,
    model: &PageModel,
    doc: &Document,
) -> OverflowReport {
    let amount = normal.overflow;
    let margin = OverflowOption {
        choice: OverflowChoice::AllowIntoMargin,
        possible: amount <= options.margin_allowance,
        detail: format!(
            "Let the text run {amount:.1} points into the bottom margin (allowed: {:.1}).",
            options.margin_allowance
        ),
    };
    let tighten = match tightest_needed(setting, options) {
        Some(trial) => OverflowOption {
            choice: OverflowChoice::Tighten,
            possible: true,
            detail: format!(
                "Set the paragraph a little tighter: letter spacing {:+.3} em, word spaces {:.0}%, line spacing {:.0}%.",
                trial.spacing.letter_em,
                trial.spacing.word_scale * 100.0,
                trial.spacing.leading_scale * 100.0
            ),
        },
        None => OverflowOption {
            choice: OverflowChoice::Tighten,
            possible: false,
            detail: "Even the tightest setting that keeps the look wouldn’t make it fit.".into(),
        },
    };
    let has_next_page = doc.get_pages().len() > model.page + 1;
    let carry_possible = has_next_page && setting.column.below.is_empty();
    let carry = OverflowOption {
        choice: OverflowChoice::CarryToNextPage,
        possible: carry_possible,
        detail: if carry_possible {
            "Move the lines that don’t fit to the top of the same column on the next page.".into()
        } else if !has_next_page {
            "There is no next page to carry the lines to.".into()
        } else {
            "Only a paragraph at the bottom of its column can carry lines to the next page.".into()
        },
    };
    let leave = OverflowOption {
        choice: OverflowChoice::Leave,
        possible: true,
        detail: "Make the change and let the column run past its bottom; fix it by hand.".into(),
    };
    OverflowReport {
        amount,
        bottom_limit: setting.column.limit,
        limit_reason: setting.column.reason,
        choices: vec![margin, tighten, carry, leave],
    }
}

/// takes the paragraph's old glyphs out and writes the new lines that
/// stay on this page.
fn write_paragraph(
    doc: &Document,
    setting: &Setting,
    plan: &mut Plan,
    trial: &Trial,
    carry: Option<usize>,
) -> Result<(), EditError> {
    let model = setting.model;
    let paragraph = setting.paragraph;
    let removed: HashSet<usize> = model.blocks[paragraph.block]
        .lines
        .iter()
        .flat_map(|line| glyphs_of(&line.chars))
        .collect();
    text_write::blank(&model.run, plan, &removed);
    // the person changed this text, so what it says for reading aloud changes too
    structure::update_actual_text(model, plan, &removed, setting.text);
    let keep = carry.unwrap_or(trial.set.lines.len());
    let placed: Vec<_> = trial
        .set
        .placed
        .iter()
        .filter(|glyph| glyph.line < keep)
        .cloned()
        .collect();
    let anchor = Anchor {
        show: paragraph.anchor_show,
        text_matrix: paragraph.anchor_matrix,
        line_step: setting.typesetter(trial.spacing).line_step(),
    };
    if !placed.is_empty() {
        text_write::write(doc, &model.run, plan, &anchor, &placed)?;
    }
    Ok(())
}
