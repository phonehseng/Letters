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

fn rules(options: &TextEditOptions) -> FallbackRules {
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

/// what an edit does to the chosen characters of a line.
#[derive(Clone, Copy)]
enum Change<'a> {
    /// replace them with this text
    Text(&'a str),
    /// keep them, with a new look
    Style(&'a TextStyleChange),
}

fn glyphs_of(chars: &[LineChar]) -> impl Iterator<Item = usize> + '_ {
    chars.iter().filter_map(|char| match char {
        LineChar::Glyph(glyph) => Some(*glyph),
        LineChar::Gap => None,
    })
}

fn text_length(model: &PageModel, chars: &[LineChar]) -> usize {
    chars
        .iter()
        .map(|char| model.char_text(char).chars().count())
        .sum()
}

/// turns a range of characters of the line's text into a range of the
/// line's glyphs and gaps. a ligature (one glyph, several characters)
/// is taken whole when the range touches it.
fn char_range(
    model: &PageModel,
    line: &LineModel,
    range: Range<usize>,
) -> Result<Range<usize>, EditError> {
    let total = text_length(model, &line.chars);
    if range.start > range.end || range.end > total {
        return Err(EditError::OutOfRange);
    }
    let mut position = 0;
    let (mut start, mut end) = (line.chars.len(), line.chars.len());
    for (index, char) in line.chars.iter().enumerate() {
        let length = model.char_text(char).chars().count();
        if start == line.chars.len() && range.start < position + length.max(1) {
            start = index;
        }
        if range.end <= position {
            end = index;
            break;
        }
        position += length;
    }
    if range.start == range.end {
        end = start;
    }
    Ok(start..end.max(start))
}

/// the text matrix of a glyph taken all the way to the page.
fn to_page(model: &PageModel, glyph: usize) -> Matrix {
    let record = &model.run.glyphs[glyph];
    record.text_matrix.then(&model.run.shows[record.show].ctm)
}

/// where a glyph starts, measured in another glyph's text space.
fn relative_x(model: &PageModel, glyph: usize, anchor: &Matrix) -> f32 {
    let inverse = anchor.invert().unwrap_or(Matrix::IDENTITY);
    to_page(model, glyph).then(&inverse).e
}

fn font_ref(model: &PageModel, glyph: usize) -> Result<FontRef, EditError> {
    let record = &model.run.glyphs[glyph];
    let show = &model.run.shows[record.show];
    match record.font.and_then(|font| model.run.fonts[font].object) {
        Some(object) => Ok(FontRef::Object(object)),
        None if !show.before.font_name.is_empty() => Ok(FontRef::Named {
            source: show.source,
            name: show.before.font_name.clone(),
        }),
        None => Err(EditError::Damaged("the text has no font".into())),
    }
}

/// the anchor of an edit starting at line position `from`: the glyph to
/// write after, and the text-space position the edit starts at.
fn anchor_for(model: &PageModel, line: &LineModel, from: usize) -> (usize, f32) {
    match line.chars.get(from) {
        Some(LineChar::Glyph(glyph)) => (*glyph, 0.0),
        _ => {
            // a gap or the end of the line: start where the glyph before ends
            let previous = glyphs_of(&line.chars[..from])
                .last()
                .expect("lines start with a glyph");
            (previous, model.run.glyphs[previous].advance)
        }
    }
}

/// picks the fonts to write `text` in, starting with the font of `like`.
fn choose_fonts(
    model: &PageModel,
    draft: &mut Draft,
    like: usize,
    text: &str,
    style: Option<&TextStyleChange>,
    options: &TextEditOptions,
) -> Result<FontChoice, EditError> {
    if let Some(standard) = style.and_then(|style| style.font) {
        let missing = embed::not_in_standard_fonts(text);
        if !missing.is_empty() {
            return Err(EditError::UnsupportedCharacters(
                missing.into_iter().collect(),
            ));
        }
        let font = embed::standard_font(draft, standard);
        return Ok(FontChoice {
            by_char: text.chars().map(|c| (c, 0)).collect(),
            fonts: vec![font],
            notices: Vec::new(),
            to_unicode_additions: Default::default(),
        });
    }
    let record = &model.run.glyphs[like];
    let Some(font) = record.font.map(|font| &model.run.fonts[font]) else {
        return Err(EditError::Damaged("the text has no font".into()));
    };
    let choice = text_fonts::choose(draft, font, text, rules(options))?;
    if !choice.to_unicode_additions.is_empty() {
        if let Some(object) = font.object {
            embed::extend_to_unicode(draft, object, &choice.to_unicode_additions);
        }
    }
    Ok(choice)
}

/// the placed glyphs of new text, in the given fonts, from position `x`.
#[allow(clippy::too_many_arguments)]
fn place_text(
    model: &PageModel,
    anchor_show: usize,
    text: &str,
    choice: &FontChoice,
    size: f32,
    rise: f32,
    color: Color,
    start_x: f32,
    line: usize,
) -> (Vec<Placed>, f32) {
    let state = &model.run.shows[anchor_show].before;
    let mut placed = Vec::new();
    let mut x = start_x;
    for character in text.chars() {
        let Some(font) = choice
            .by_char
            .get(&character)
            .map(|index| &choice.fonts[*index])
        else {
            // a space no font here can draw: a plain move a quarter wide
            x += advance(state, 250.0, size, false);
            continue;
        };
        let code = font
            .codes
            .get(&character)
            .expect("chosen fonts draw their characters");
        placed.push(Placed {
            x,
            line,
            font: FontRef::Object(font.object),
            size,
            rise,
            color,
            code: Some(code.code.clone()),
            width: code.width,
            is_word_space: code.is_word_space,
        });
        x += advance(state, code.width, size, code.is_word_space);
    }
    (placed, x)
}

/// the size, rise and colour new text takes from the glyph it continues.
fn look_of(model: &PageModel, glyph: usize) -> (f32, f32, Color) {
    let record = &model.run.glyphs[glyph];
    let state = &model.run.shows[record.show].before;
    (state.size, state.rise, record.color)
}

/// the core line edit: the line's characters `range` change (replaced
/// or restyled) and everything after them on the line is written again,
/// unchanged but moved by however much wider or narrower the change is.
#[allow(clippy::too_many_arguments)]
fn edit_line(
    model: &PageModel,
    draft: &mut Draft,
    plan: &mut Plan,
    block: usize,
    line_index: usize,
    range: Range<usize>,
    change: Change,
    options: &TextEditOptions,
) -> Result<Outcome, EditError> {
    let line = &model.blocks[block].lines[line_index];
    let (anchor_glyph, start_x) = anchor_for(model, line, range.start);
    let anchor_show = model.run.glyphs[anchor_glyph].show;
    let anchor_matrix = model.run.glyphs[anchor_glyph].text_matrix;
    let anchor_page = to_page(model, anchor_glyph);
    let (size, rise, color) = look_of(model, anchor_glyph);

    let mut placed = Vec::new();
    let mut notices = Vec::new();
    let mut x = start_x;
    match change {
        Change::Text(text) if !text.is_empty() => {
            let choice = choose_fonts(model, draft, anchor_glyph, text, None, options)?;
            let (glyphs, end) =
                place_text(model, anchor_show, text, &choice, size, rise, color, x, 0);
            placed.extend(glyphs);
            notices = choice.notices;
            x = end;
        }
        Change::Text(_) => {}
        Change::Style(style) => {
            let restyled = restyle(
                model,
                draft,
                line,
                range.clone(),
                style,
                &anchor_page,
                x,
                options,
            )?;
            placed.extend(restyled.0);
            notices = restyled.1;
            x = restyled.2;
        }
    }

    // the rest of the line keeps its glyphs and spacing, moved along
    let tail = &line.chars[range.end..];
    if glyphs_of(tail).next().is_some() {
        let tail_start = match tail[0] {
            LineChar::Glyph(glyph) => relative_x(model, glyph, &anchor_page),
            LineChar::Gap => {
                let previous = glyphs_of(&line.chars[..range.end])
                    .last()
                    .expect("lines start with a glyph");
                relative_x(model, previous, &anchor_page) + model.run.glyphs[previous].advance
            }
        };
        let shift = x - tail_start;
        for glyph in glyphs_of(tail) {
            let record = &model.run.glyphs[glyph];
            let (size, rise, _) = look_of(model, glyph);
            placed.push(Placed {
                x: relative_x(model, glyph, &anchor_page) + shift,
                line: 0,
                font: font_ref(model, glyph)?,
                size,
                rise,
                color: record.color,
                code: Some(record.code.clone()),
                width: record.width,
                is_word_space: record.code == [32],
            });
        }
    }

    let removed: HashSet<usize> = glyphs_of(&line.chars[range.start..]).collect();
    text_write::blank(&model.run, plan, &removed);
    let anchor = Anchor {
        show: anchor_show,
        text_matrix: anchor_matrix,
        line_step: 0.0,
    };
    if !placed.is_empty() {
        text_write::write(draft.doc, &model.run, plan, &anchor, &placed)?;
    }
    Ok(Outcome {
        notices,
        overflow: None,
    })
}

/// the restyled glyphs of a range, and where the pen ends.
#[allow(clippy::too_many_arguments)]
fn restyle(
    model: &PageModel,
    draft: &mut Draft,
    line: &LineModel,
    range: Range<usize>,
    style: &TextStyleChange,
    anchor_page: &Matrix,
    start_x: f32,
    options: &TextEditOptions,
) -> Result<(Vec<Placed>, Vec<FontNotice>, f32), EditError> {
    let chars = &line.chars[range.clone()];
    let Some(first) = glyphs_of(chars).next() else {
        return Ok((Vec::new(), Vec::new(), start_x));
    };
    let anchor_show = model.run.glyphs[first].show;
    let (original_size, rise, original_color) = look_of(model, first);
    let visible_size = model.run.glyphs[first].size.max(1e-6);
    let size = style.size.map_or(original_size, |wanted| {
        original_size * wanted / visible_size
    });
    let color = style.color.unwrap_or(original_color);
    if style.font.is_some() {
        let text: String = chars.iter().map(|char| model.char_text(char)).collect();
        let choice = choose_fonts(model, draft, first, &text, Some(style), options)?;
        let (placed, end) = place_text(
            model,
            anchor_show,
            &text,
            &choice,
            size,
            rise,
            color,
            start_x,
            0,
        );
        return Ok((placed, choice.notices, end));
    }
    // same fonts: each glyph keeps its code; positions follow the new size
    let state = &model.run.shows[anchor_show].before;
    let mut placed = Vec::new();
    let mut x = start_x;
    let mut previous_end: Option<f32> = None;
    for char in chars {
        match char {
            LineChar::Glyph(glyph) => {
                let record = &model.run.glyphs[*glyph];
                let glyph_size = look_of(model, *glyph).0 * size / original_size.max(1e-6);
                placed.push(Placed {
                    x,
                    line: 0,
                    font: font_ref(model, *glyph)?,
                    size: glyph_size,
                    rise,
                    color: style.color.unwrap_or(record.color),
                    code: Some(record.code.clone()),
                    width: record.width,
                    is_word_space: record.code == [32],
                });
                x += advance(state, record.width, glyph_size, record.code == [32]);
                previous_end = Some(relative_x(model, *glyph, anchor_page) + record.advance);
            }
            LineChar::Gap => {
                // a gap keeps its width, scaled with the text
                let next = glyphs_of(&line.chars[range.start..]).find(|glyph| {
                    previous_end.is_some_and(|end| relative_x(model, *glyph, anchor_page) > end)
                });
                let gap = match (previous_end, next) {
                    (Some(end), Some(next)) => relative_x(model, next, anchor_page) - end,
                    _ => advance(state, 250.0, original_size, false),
                };
                x += gap * size / original_size.max(1e-6);
            }
        }
    }
    Ok((placed, Vec::new(), x))
}

/// re-sets a block's text over its lines, wrapped to the block's width
/// (or line by line as given), keeping its first line's start, its line
/// spacing and its alignment.
fn reset_block(
    model: &PageModel,
    draft: &mut Draft,
    plan: &mut Plan,
    block_index: usize,
    text: &str,
    options: &TextEditOptions,
) -> Result<Outcome, EditError> {
    let block = &model.blocks[block_index];
    let first_line = &block.lines[0];
    let anchor_glyph = PageModel::first_glyph(first_line);
    let anchor_show = model.run.glyphs[anchor_glyph].show;
    let anchor_matrix = model.run.glyphs[anchor_glyph].text_matrix;
    let anchor_page = to_page(model, anchor_glyph);
    let (size, rise, color) = look_of(model, anchor_glyph);

    // page units per text-space unit, along and across the line
    let along_scale = model
        .run
        .space
        .visible_vector(anchor_page.apply_vector(Point::new(1.0, 0.0)))
        .length()
        .max(1e-6);
    let across_scale = model
        .run
        .space
        .visible_vector(anchor_page.apply_vector(Point::new(0.0, 1.0)))
        .length()
        .max(1e-6);
    let leading = block.leading.unwrap_or(first_line.size * 1.2);
    let line_step = leading / across_scale;

    // the block's left and right edges, along the first line, in text space
    let along = |line: &LineModel| {
        line.start.minus(first_line.start).dot(first_line.direction) / along_scale
    };
    let left = block.lines.iter().map(along).fold(f32::MAX, f32::min);
    let right = block
        .lines
        .iter()
        .map(|line| along(line) + line.length / along_scale)
        .fold(f32::MIN, f32::max);
    let width = right - left;

    let choice = choose_fonts(model, draft, anchor_glyph, text, None, options)?;
    let state = &model.run.shows[anchor_show].before;
    let measure = |word: &str| -> f32 {
        word.chars()
            .map(|character| {
                match choice
                    .by_char
                    .get(&character)
                    .map(|index| &choice.fonts[*index])
                    .and_then(|font| font.codes.get(&character))
                {
                    Some(code) => advance(state, code.width, size, code.is_word_space),
                    None => advance(state, 250.0, size, false),
                }
            })
            .sum()
    };
    let lines = if options.reflow {
        wrap(text, width, &measure)
    } else {
        text.split('\n')
            .map(|line| (line.to_owned(), true))
            .collect()
    };

    let mut placed = Vec::new();
    for (number, (line_text, ends_paragraph)) in lines.iter().enumerate() {
        let natural = measure(line_text);
        let spaces = line_text.chars().filter(|c| *c == ' ').count();
        let (start, extra_per_space) = match block.alignment {
            TextAlignment::Left => (left, 0.0),
            TextAlignment::Right => (right - natural, 0.0),
            TextAlignment::Center => (left + (width - natural) / 2.0, 0.0),
            TextAlignment::Justified if !ends_paragraph && spaces > 0 => {
                (left, (width - natural).max(0.0) / spaces as f32)
            }
            TextAlignment::Justified => (left, 0.0),
        };
        let (mut glyphs, _) = place_text(
            model,
            anchor_show,
            line_text,
            &choice,
            size,
            rise,
            color,
            start,
            number,
        );
        if extra_per_space > 0.0 {
            // spread the spare width over the spaces: each glyph after a
            // space moves by one more share
            let mut shift = 0.0;
            let mut chars = line_text.chars().filter(|c| choice.by_char.contains_key(c));
            for glyph in &mut glyphs {
                glyph.x += shift;
                if chars.next() == Some(' ') {
                    shift += extra_per_space;
                }
            }
        }
        placed.extend(glyphs);
    }

    let removed: HashSet<usize> = block
        .lines
        .iter()
        .flat_map(|line| glyphs_of(&line.chars))
        .collect();
    text_write::blank(&model.run, plan, &removed);
    let anchor = Anchor {
        show: anchor_show,
        text_matrix: anchor_matrix,
        line_step,
    };
    if !placed.is_empty() {
        text_write::write(draft.doc, &model.run, plan, &anchor, &placed)?;
    }
    let overflow = (lines.len() > block.lines.len()).then(|| TextOverflow {
        lines_before: block.lines.len(),
        lines_after: lines.len(),
        extra_height: (lines.len() - block.lines.len()) as f32 * leading,
    });
    Ok(Outcome {
        notices: choice.notices,
        overflow,
    })
}

/// greedy line breaking: words move to the next line when they don't
/// fit; a new line in the text always starts a new line. each line says
/// whether it ends a paragraph (justified text leaves those ragged).
fn wrap(text: &str, width: f32, measure: &dyn Fn(&str) -> f32) -> Vec<(String, bool)> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split(' ').filter(|word| !word.is_empty()) {
            let candidate = if current.is_empty() {
                word.to_owned()
            } else {
                format!("{current} {word}")
            };
            if current.is_empty() || measure(&candidate) <= width + 1e-3 {
                current = candidate;
            } else {
                lines.push((std::mem::take(&mut current), false));
                current = word.to_owned();
            }
        }
        lines.push((current, true));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greedy_wrapping_fills_lines() {
        let measure = |word: &str| word.chars().count() as f32;
        let lines = wrap("aa bb cc dd\nee", 5.0, &measure);
        let texts: Vec<&str> = lines.iter().map(|(text, _)| text.as_str()).collect();
        assert_eq!(texts, ["aa bb", "cc dd", "ee"]);
        assert_eq!(
            lines.iter().map(|(_, end)| *end).collect::<Vec<_>>(),
            [false, true, true]
        );
    }
}
