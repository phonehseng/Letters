//! "Make Editable Text": scanned pages turned into real, visible text with
//! letters-edit's `convert_scan_to_text`, from the words recognition read.
//!
//! a page that already carries Letters' invisible text layer loses it
//! first, so the words aren't on the page twice (once to see and once
//! hidden, which would make every search find them twice). used inside one
//! edit, so when any page fails nothing changes.

use letters_edit::{convert_scan_to_text, page_objects, ChangesSummary, ScanOptions};
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
        // a page made editable before already shows its words as real
        // text: converting it again would write every line a second time
        if has_visible_text(doc, *page) {
            return Err(format!(
                "Page {} already has editable text. Change it in the Edit tab, or Undo the earlier Make Editable Text to read the scan again.",
                page + 1
            ));
        }
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

/// whether a page shows text a person can see (not only Letters' hidden
/// layer over a scan). a page that can't be read is left to the conversion,
/// which says what is wrong with it.
fn has_visible_text(doc: &lopdf::Document, page: u32) -> bool {
    let Ok(objects) = page_objects(doc, page as usize) else {
        return false;
    };
    objects
        .text_blocks
        .iter()
        .flat_map(|block| &block.lines)
        .flat_map(|line| &line.runs)
        .any(|run| !run.invisible && !run.text.trim().is_empty())
}

fn merge(all: &mut ChangesSummary, page: ChangesSummary) {
    for changed in page.pages_changed {
        if !all.pages_changed.contains(&changed) {
            all.pages_changed.push(changed);
        }
    }
    all.notes.extend(page.notes);
}
