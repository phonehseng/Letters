//! editing text in place: replacing, inserting, deleting and restyling.
//!
//! every edit follows the same steps: read the page, find the text by its
//! id, work out the new glyphs and where they go, check every character
//! can be drawn (choosing fallback fonts if needed), write the changes
//! into a draft, and only then commit. if anything fails the document is
//! left exactly as it was.

use std::collections::HashSet;
use std::ops::Range;

use lopdf::Document;
use serde::{Deserialize, Serialize};

use crate::colors::Color;
use crate::draft::Draft;
use crate::error::EditError;
use crate::fonts::embed;
use crate::fonts::standard::StandardFont;
use crate::geometry::{Matrix, Point};
use crate::layout::{self, LineChar, LineModel, PageModel};
use crate::model::{PageObjects, TextAlignment};
use crate::rewrite::{self, Plan};
use crate::text_fonts::{self, FallbackRules, FontChoice, FontNotice};
use crate::text_write::{self, advance, Anchor, FontRef, Placed};

mod block;
mod line;

pub(crate) use block::wrap;
use block::*;
use line::*;
pub(crate) use line::{choose_fonts, glyphs_of, look_of, to_page};

/// choices for a text edit.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextEditOptions {
    /// when replacing a block's text, wrap it to the block's width (true)
    /// or keep exactly the lines given by new lines in the text (false)
    pub reflow: bool,
    /// may copy the same font from this computer when the original lacks
    /// a character
    pub allow_installed_fonts: bool,
    /// may use a similar standard font when nothing better is possible;
    /// if false, such an edit is refused instead
    pub allow_standard_fonts: bool,
}

impl Default for TextEditOptions {
    fn default() -> Self {
        Self {
            reflow: true,
            allow_installed_fonts: true,
            allow_standard_fonts: true,
        }
    }
}

/// a change of look for some text. fields left out stay as they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextStyleChange {
    /// a standard font to switch to; none keeps each character's font
    pub font: Option<StandardFont>,
    /// the size as seen on the page, in points
    pub size: Option<f32>,
    pub color: Option<Color>,
}

/// text that no longer fits the block it was in.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextOverflow {
    pub lines_before: usize,
    pub lines_after: usize,
    /// how much taller the block became, in points
    pub extra_height: f32,
}

/// what a text edit did, with the page's objects as they are now (and
/// fresh ids for them).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEditResult {
    pub objects: PageObjects,
    /// fallback fonts used, each with a sentence for the person
    pub notices: Vec<FontNotice>,
    /// set when re-set text needs more lines than the block had
    pub overflow: Option<TextOverflow>,
}

/// what an edit worked out, before it is committed.
struct Outcome {
    notices: Vec<FontNotice>,
    overflow: Option<TextOverflow>,
}

/// runs an edit against a draft and commits it only if it all worked.
fn edit(
    doc: &mut Document,
    page: usize,
    work: impl FnOnce(&PageModel, &mut Draft, &mut Plan) -> Result<Outcome, EditError>,
) -> Result<TextEditResult, EditError> {
    let model = layout::build(doc, page)?;
    let changes;
    let outcome;
    {
        let mut draft = Draft::new(doc);
        let mut plan = Plan::default();
        outcome = work(&model, &mut draft, &mut plan)?;
        rewrite::apply(&mut draft, &model.run, plan)?;
        changes = draft.finish();
    }
    changes.commit(doc);
    Ok(TextEditResult {
        objects: layout::build(doc, page)?.objects(),
        notices: outcome.notices,
        overflow: outcome.overflow,
    })
}

pub(crate) fn rules(options: &TextEditOptions) -> FallbackRules {
    FallbackRules {
        installed_fonts: options.allow_installed_fonts,
        standard_fonts: options.allow_standard_fonts,
    }
}

fn check_text(text: &str) -> Result<(), EditError> {
    if text.chars().any(|c| c.is_control() && c != '\n') {
        return Err(EditError::InvalidValue(
            "The text contains invisible control characters. Remove them and try again.".into(),
        ));
    }
    Ok(())
}

/// replaces the text of a line (the line keeps its start and baseline
/// and grows or shrinks) or of a block (re-set over the block's lines).
pub fn replace_text(
    doc: &mut Document,
    page: usize,
    id: &str,
    text: &str,
    options: &TextEditOptions,
) -> Result<TextEditResult, EditError> {
    check_text(text)?;
    edit(doc, page, |model, draft, plan| {
        if let Ok((block, line)) = model.find_line(id) {
            let length = model.blocks[block].lines[line].chars.len();
            let single_line = text.replace('\n', " ");
            return edit_line(
                model,
                draft,
                plan,
                block,
                line,
                0..length,
                Change::Text(&single_line),
                options,
            );
        }
        let block = model.find_block(id)?;
        reset_block(model, draft, plan, block, text, options)
    })
}

/// inserts text at a character position of a line; the rest of the line
/// moves along to make room.
pub fn insert_text(
    doc: &mut Document,
    page: usize,
    line_id: &str,
    index: usize,
    text: &str,
    options: &TextEditOptions,
) -> Result<TextEditResult, EditError> {
    check_text(text)?;
    edit(doc, page, |model, draft, plan| {
        let (block, line) = model.find_line(line_id)?;
        let range = char_range(model, &model.blocks[block].lines[line], index..index)?;
        edit_line(
            model,
            draft,
            plan,
            block,
            line,
            range,
            Change::Text(&text.replace('\n', " ")),
            options,
        )
    })
}

/// deletes the characters `start..end` of a line; the rest closes up.
pub fn delete_chars(
    doc: &mut Document,
    page: usize,
    line_id: &str,
    start: usize,
    end: usize,
) -> Result<TextEditResult, EditError> {
    edit(doc, page, |model, draft, plan| {
        let (block, line) = model.find_line(line_id)?;
        let range = char_range(model, &model.blocks[block].lines[line], start..end)?;
        edit_line(
            model,
            draft,
            plan,
            block,
            line,
            range,
            Change::Text(""),
            &TextEditOptions::default(),
        )
    })
}

/// deletes a whole block, line or run. a run's line closes up behind it;
/// deleted lines and blocks simply leave their space empty.
pub fn delete_text(doc: &mut Document, page: usize, id: &str) -> Result<TextEditResult, EditError> {
    edit(doc, page, |model, draft, plan| {
        if let Ok((block, line, range)) = model.find_run(id) {
            return edit_line(
                model,
                draft,
                plan,
                block,
                line,
                range,
                Change::Text(""),
                &TextEditOptions::default(),
            );
        }
        let lines: Vec<&LineModel> = match model.find_line(id) {
            Ok((block, line)) => vec![&model.blocks[block].lines[line]],
            Err(_) => model.blocks[model.find_block(id)?].lines.iter().collect(),
        };
        let removed: HashSet<usize> = lines
            .iter()
            .flat_map(|line| glyphs_of(&line.chars))
            .collect();
        text_write::blank(&model.run, plan, &removed);
        Ok(Outcome {
            notices: Vec::new(),
            overflow: None,
        })
    })
}

/// changes the font, size or colour of text: a run, a line, a block, or
/// characters `range` of a line or run. following text on the line moves
/// if the new look is wider or narrower.
pub fn set_text_style(
    doc: &mut Document,
    page: usize,
    id: &str,
    range: Option<(usize, usize)>,
    style: &TextStyleChange,
    options: &TextEditOptions,
) -> Result<TextEditResult, EditError> {
    if let Some(size) = style.size {
        if !(size.is_finite() && (0.5..=1000.0).contains(&size)) {
            return Err(EditError::InvalidValue(
                "Text size must be between 0.5 and 1000 points.".into(),
            ));
        }
    }
    edit(doc, page, |model, draft, plan| {
        let mut targets: Vec<(usize, usize, Range<usize>)> = Vec::new();
        if let Ok((block, line, run_range)) = model.find_run(id) {
            let line_model = &model.blocks[block].lines[line];
            let chosen = match range {
                Some((start, end)) => {
                    let offset = text_length(model, &line_model.chars[..run_range.start]);
                    char_range(model, line_model, offset + start..offset + end)?
                }
                None => run_range,
            };
            targets.push((block, line, chosen));
        } else if let Ok((block, line)) = model.find_line(id) {
            let line_model = &model.blocks[block].lines[line];
            let chosen = match range {
                Some((start, end)) => char_range(model, line_model, start..end)?,
                None => 0..line_model.chars.len(),
            };
            targets.push((block, line, chosen));
        } else {
            let block = model.find_block(id)?;
            for (line, line_model) in model.blocks[block].lines.iter().enumerate() {
                targets.push((block, line, 0..line_model.chars.len()));
            }
        }
        let mut notices = Vec::new();
        for (block, line, chosen) in targets {
            let outcome = edit_line(
                model,
                draft,
                plan,
                block,
                line,
                chosen,
                Change::Style(style),
                options,
            )?;
            notices.extend(outcome.notices);
        }
        Ok(Outcome {
            notices,
            overflow: None,
        })
    })
}
