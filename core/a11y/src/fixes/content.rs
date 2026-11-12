//! changes to page content: recolouring text for contrast, and tagging
//! content that has no tag.

use std::collections::BTreeMap;

use lopdf::Document;

use crate::colour::Rgb;
use crate::content::read_page;
use crate::error::A11yError;
use crate::geometry::PageFrame;
use crate::objects::ensure_editable;
use crate::report::{ContentRef, FixResult};
use crate::rewrite::{recolour, replace_content};

pub use crate::autotag::tag_untagged_content;

/// changes the colour of just these pieces of text (from a contrast
/// finding) to `colour`. right after each, the colour is set back to what
/// it was, so other text and shapes keep their colours.
pub fn apply_text_colour(
    doc: &mut Document,
    runs: &[ContentRef],
    colour: Rgb,
) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    if runs.is_empty() {
        return Err(A11yError::InvalidInput(
            "Choose the text to recolour first.".into(),
        ));
    }
    let mut by_page: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for run in runs {
        by_page.entry(run.page).or_default().push(run.op_index);
    }
    // work out every page's new content before changing any of them
    let mut rewritten = Vec::new();
    for (&page, indexes) in &by_page {
        let page_id = super::page_id(doc, page)?;
        let frame = PageFrame::read(doc, page_id);
        let content =
            read_page(doc, page_id, &frame).map_err(|_| A11yError::UnreadableContent(page + 1))?;
        let mut targets = Vec::new();
        for &index in indexes {
            let run = content
                .texts
                .iter()
                .find(|run| run.op_index == index && !run.in_form)
                .ok_or_else(|| {
                    A11yError::InvalidInput("That text couldn’t be found. Check the document again and pick it from the new list.".into())
                })?;
            if !targets.iter().any(|(i, _)| *i == index) {
                targets.push((index, run.fill_state.fill_operations()));
            }
        }
        let operations = recolour(&content.operations, &targets, colour.unit());
        rewritten.push((page_id, operations));
    }
    for (page_id, operations) in rewritten {
        replace_content(doc, page_id, operations)?;
    }
    Ok(FixResult::one(format!(
        "Changed {} to {}.",
        crate::rules::plural(runs.len(), "piece of text", "pieces of text"),
        colour.hex()
    )))
}
