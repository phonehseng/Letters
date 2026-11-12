//! the Accessibility tab: checking a document and the fix wizard.
//!
//! checking reads the document as it is shown (`state.read_document`, no
//! undo step) and measures contrast from pages drawn by the pdf worker.
//! every fix is one call through `state.edit`, so it is one undo step and
//! the file on disk changes only when the person saves. the engine is
//! core/a11y; the requests and the page pictures are in shell-core's a11y/.

use std::path::PathBuf;

use letters_a11y::{
    self as a11y, propose_heading_levels, A11yError, HeadingChange, ObjectRef, Rect, Report,
};
use letters_pdf::{DocumentId, PageSize};
use letters_shell_core::a11y::{
    apply_fix, reading_blocks, region_png, FixOutcome, FixRequest, PageRegions, ReadingBlock,
};
use letters_shell_core::forms::{data_url, write_chosen_bytes};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

/// the size the wizard shows a picture or a table at, in pixels per point.
const REGION_SCALE: f32 = 1.5;

impl From<A11yError> for CommandError {
    fn from(error: A11yError) -> Self {
        CommandError::failed(error.to_string())
    }
}

/// a check's report, and whether Mark as PDF/UA may be offered: the
/// engine's own rule (no must-fix left, nothing it needs left unchecked).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckAnswer {
    #[serde(flatten)]
    report: Report,
    machine_checks_pass: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixEdit {
    document: DocumentState,
    outcome: FixOutcome,
}

/// the engine's sentences the tab reuses, so the words live in one place.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct A11yWords {
    tags: &'static str,
    alt_text: &'static str,
    reading_order: &'static str,
    pdf_ua: &'static str,
    human_checks: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum A11yReportFormat {
    Html,
    Text,
}

/// checks the document as it is shown now, contrast included.
#[tauri::command(async)]
pub fn a11y_check(state: State<AppState>, id: u64) -> Result<CheckAnswer, CommandError> {
    let report = check_document(&state, id)?;
    Ok(CheckAnswer {
        machine_checks_pass: report.machine_checks_pass(),
        report,
    })
}

fn check_document(state: &AppState, id: u64) -> Result<Report, CommandError> {
    let mut regions = page_regions(state, id)?;
    state.read_document(DocumentId(id), |document| {
        Ok(a11y::check(document, Some(&mut regions))?)
    })
}

/// runs one fix of the wizard as one edit.
#[tauri::command(async)]
pub fn a11y_fix(
    state: State<AppState>,
    id: u64,
    request: FixRequest,
) -> Result<FixEdit, CommandError> {
    let mut regions = if request.needs_renderer() {
        Some(page_regions(&state, id)?)
    } else {
        None
    };
    let mut outcome = FixOutcome::default();
    let document = state.edit(DocumentId(id), |document| {
        let renderer = regions
            .as_mut()
            .map(|regions| regions as &mut dyn a11y::RegionRenderer);
        outcome = apply_fix(document, &request, renderer)?;
        Ok::<(), A11yError>(())
    })?;
    Ok(FixEdit { document, outcome })
}

/// the heading ladder the engine proposes, for the wizard to show and edit.
#[tauri::command(async)]
pub fn a11y_heading_proposal(
    state: State<AppState>,
    id: u64,
) -> Result<Vec<HeadingChange>, CommandError> {
    state.read_document(DocumentId(id), |document| {
        Ok(propose_heading_levels(document)?)
    })
}

/// the blocks of a page in `order`, or in the order the layout suggests
/// when none is given, with where each is and its first words.
#[tauri::command(async)]
pub fn a11y_reading_blocks(
    state: State<AppState>,
    id: u64,
    page: u32,
    order: Option<Vec<ObjectRef>>,
) -> Result<Vec<ReadingBlock>, CommandError> {
    state.read_document(DocumentId(id), |document| {
        let order = match order {
            Some(order) => order,
            None => a11y::auto_reading_order(document, page)?,
        };
        Ok(reading_blocks(document, page, &order))
    })
}

/// a part of a page as a picture, for the figure and table steps.
#[tauri::command(async)]
pub fn a11y_region_picture(
    state: State<AppState>,
    id: u64,
    page: u32,
    rect: Rect,
) -> Result<String, CommandError> {
    let mut regions = page_regions(&state, id)?;
    let png =
        region_png(&mut regions, page, rect, 6.0, REGION_SCALE).map_err(CommandError::failed)?;
    Ok(data_url("image/png", &png))
}

/// writes the check's report where the save panel said.
#[tauri::command(async)]
pub fn a11y_save_report(
    state: State<AppState>,
    id: u64,
    path: String,
    format: A11yReportFormat,
) -> Result<String, CommandError> {
    let report = check_document(&state, id)?;
    let text = match format {
        A11yReportFormat::Html => a11y::report_html(&report),
        A11yReportFormat::Text => a11y::report_text(&report),
    };
    let path = PathBuf::from(path);
    write_chosen_bytes(&path, text.as_bytes()).map_err(CommandError::failed)?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn a11y_words() -> A11yWords {
    A11yWords {
        tags: a11y::TAGS_EXPLAINED,
        alt_text: a11y::ALT_TEXT_EXPLAINED,
        reading_order: a11y::READING_ORDER_EXPLAINED,
        pdf_ua: a11y::PDF_UA_EXPLAINED,
        human_checks: a11y::PDFUA_HUMAN_CHECKS,
    }
}

/// draws the document's pages as the window shows them, for contrast.
fn page_regions(state: &AppState, id: u64) -> Result<PageRegions, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let shown = state.documents.state(DocumentId(id))?;
    let sizes = shown
        .pages
        .iter()
        .map(|page| PageSize {
            width_pt: page.width,
            height_pt: page.height,
        })
        .collect();
    Ok(PageRegions::new(worker, DocumentId(id), sizes))
}
