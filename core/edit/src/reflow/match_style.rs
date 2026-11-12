//! "Match Style" for new text: a text box that takes the font object,
//! size, colour (in its own colour space), spacing, leading and alignment
//! of the nearest paragraph, so an added line looks like the text around
//! it. characters the font lacks are added to the font, as reflow does.

use std::collections::HashMap;

use lopdf::Document;
use serde::{Deserialize, Serialize};

use crate::edit_run::{run_edit, Landing, Outcome};
use crate::error::EditError;
use crate::geometry::{Point, Rect};
use crate::layout::PageModel;
use crate::model::PageObjects;
use crate::moving;
use crate::same_font;
use crate::summary::{ContentEditResult, EditConsent};
use crate::text_edit::TextEditOptions;

use super::paragraph::{self, Spacing, Typesetter};
use super::standalone::{standalone_text, Placement};
use super::{check_text, kerning};

/// a text box to add in the style of a paragraph on the page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchedText {
    /// the top left of the box, in visible page points
    pub position: Point,
    pub text: String,
    /// wrap to this width (points) with the paragraph's alignment; none
    /// keeps each line of `text` as one line
    #[serde(default)]
    pub width: Option<f32>,
    /// the paragraph to match; none picks the nearest one
    #[serde(default)]
    pub like_block: Option<String>,
}

/// a width this large never wraps: lines break only at new lines.
const NO_WRAP: f32 = 1.0e6;

/// adds a text box in the style of a paragraph on the page.
pub fn add_text_matching(
    doc: &mut Document,
    page: usize,
    new_text: &MatchedText,
    options: &TextEditOptions,
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    check_text(&new_text.text)?;
    if !new_text.position.is_finite() || new_text.text.trim().is_empty() {
        return Err(EditError::InvalidValue(
            "Type some text and place it on the page.".into(),
        ));
    }
    run_edit(doc, page, consent, |model, draft, plan| {
        let objects = model.objects();
        let block = match &new_text.like_block {
            Some(id) => model.find_block(id)?,
            None => nearest_block(&objects, new_text.position).ok_or_else(|| {
                EditError::InvalidValue("There is no text on this page to match.".into())
            })?,
        };
        let rect = objects.text_blocks[block].rect;
        let mut paragraph = paragraph::paragraph_of(model, block, rect);
        let width = new_text.width.unwrap_or(NO_WRAP);
        paragraph.right = paragraph.left + width / paragraph.along_scale;
        let (choice, extension) = same_font::choose_same_font(
            model,
            draft,
            paragraph.anchor_glyph,
            &new_text.text,
            options,
            None,
        )?;
        let font = model.run.glyphs[paragraph.anchor_glyph].font;
        let kerning: HashMap<(char, char), f32> = kerning::kerning_pairs(model, font);
        let typesetter = Typesetter {
            model,
            paragraph: &paragraph,
            choice: &choice,
            spacing: Spacing::NORMAL,
            kerning: &kerning,
        };
        let set = typesetter.set(&new_text.text);
        let placement = Placement {
            model,
            target: model,
            paragraph: &paragraph,
            line_step: typesetter.line_step(),
            change: moved_to(model, rect, new_text.position),
        };
        let drawing = standalone_text(draft, plan, &placement, &set.placed);
        plan.append(drawing.as_bytes());
        let mut outcome = Outcome {
            notices: choice.notices.clone(),
            ..Outcome::default()
        };
        outcome.summary.fonts_extended.extend(extension);
        outcome
            .summary
            .note(style_sentence(model, &objects, block, &paragraph));
        outcome.landings.push(Landing::Block(new_text.position));
        Ok(outcome)
    })
}

/// the page-space move from the paragraph's box to the new box.
fn moved_to(model: &PageModel, rect: Rect, position: Point) -> crate::geometry::Matrix {
    moving::screen_move(model, position.x - rect.x, position.y - rect.y)
}

/// the block whose box is nearest the point.
fn nearest_block(objects: &PageObjects, point: Point) -> Option<usize> {
    let distance = |rect: &Rect| {
        let dx = (rect.x - point.x).max(point.x - rect.right()).max(0.0);
        let dy = (rect.y - point.y).max(point.y - rect.bottom()).max(0.0);
        dx.hypot(dy)
    };
    (0..objects.text_blocks.len()).min_by(|a, b| {
        distance(&objects.text_blocks[*a].rect).total_cmp(&distance(&objects.text_blocks[*b].rect))
    })
}

/// "Matched the style of “Thank you for…”: ArialMT, 11 pt, leading 15 pt."
fn style_sentence(
    model: &PageModel,
    objects: &PageObjects,
    block: usize,
    paragraph: &paragraph::Paragraph,
) -> String {
    let source = &objects.text_blocks[block];
    let start: String = source.text.chars().take(24).collect();
    let font = model.font_info(model.run.glyphs[paragraph.anchor_glyph].font);
    let size = model.run.glyphs[paragraph.anchor_glyph].size;
    format!(
        "Matched the style of “{start}…”: {}, {size:.1} pt, line spacing {:.1} pt.",
        font.name, paragraph.leading
    )
}
