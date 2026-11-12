//! "Make Editable Text": scanned pages turned into real, visible text with
//! letters-edit's `convert_scan_to_text`, from the words recognition read.
//!
//! a page that already carries Letters' invisible text layer loses it
//! first, so the words aren't on the page twice (once to see and once
//! hidden, which would make every search find them twice). used inside one
//! edit, so when any page fails nothing changes.

use letters_edit::{convert_scan_to_text, ChangesSummary, ScanOptions};
use letters_ocr::{remove_text_layer, OcrPage};
use serde::Serialize;

/// what a conversion did, for the sentence after it.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditableSummary {
    pub pages: usize,
    pub lines: usize,
    /// lines read with low confidence: worth checking against the scan
    pub doubtful_lines: usize,
    /// pictures kept from the scans (text only)
    pub figures: usize,
    /// the pages' change summaries, merged
    pub changes: ChangesSummary,
}

/// converts every page, in order. the error is a sentence for the person,
/// naming the page.
pub fn make_pages_editable(
    doc: &mut lopdf::Document,
    pages: &[(u32, OcrPage)],
    options: &ScanOptions,
) -> Result<EditableSummary, String> {
    let mut summary = EditableSummary::default();
    for (page, words) in pages {
        let on_page = |error: String| format!("Page {}: {error}", page + 1);
        remove_text_layer(doc, *page).map_err(|error| on_page(error.to_string()))?;
        let converted = convert_scan_to_text(doc, *page as usize, words, options)
            .map_err(|error| on_page(error.to_string()))?;
        summary.pages += 1;
        summary.lines += converted.lines.len();
        summary.doubtful_lines += converted
            .lines
            .iter()
            .filter(|line| line.low_confidence)
            .count();
        summary.figures += converted.figures.len();
        merge(&mut summary.changes, converted.summary);
    }
    Ok(summary)
}

fn merge(all: &mut ChangesSummary, page: ChangesSummary) {
    for changed in page.pages_changed {
        if !all.pages_changed.contains(&changed) {
            all.pages_changed.push(changed);
        }
    }
    all.notes.extend(page.notes);
}
