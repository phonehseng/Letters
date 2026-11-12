//! the Edit tab's page setup: margins, paper size, extra space on one side
//! and the five page boxes, through letters-edit, each one undo step.
//!
//! every answer carries the page's layout as it is afterwards (its margins
//! and boxes), so the sheet shows the new numbers without asking again.

use letters_edit::{
    add_page_space, page_boxes, page_margins, resize_page, set_page_boxes, set_page_margins,
    ChangesSummary, EditConsent, EditError, MarginFit, Margins, PageBoxes, PageMargins, PaperFit,
    Side,
};
use letters_pdf::DocumentId;
use serde::Serialize;
use tauri::State;

use crate::documents::DocumentState;
use crate::edit_answers::{run_change, Outcome};
use crate::errors::CommandError;
use crate::state::AppState;

/// where a page's content sits and the boxes it has.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLayout {
    margins: PageMargins,
    boxes: PageBoxes,
}

/// what a page setup change answers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageAnswer {
    document: DocumentState,
    layout: PageLayout,
    summary: ChangesSummary,
}

fn layout_of(document: &lopdf::Document, page: usize) -> Result<PageLayout, EditError> {
    Ok(PageLayout {
        margins: page_margins(document, page)?,
        boxes: page_boxes(document, page)?,
    })
}

/// runs one page change and reads the page's layout as it is afterwards.
fn page_change(
    state: &AppState,
    id: u64,
    page: usize,
    work: impl FnOnce(&mut lopdf::Document) -> Result<ChangesSummary, EditError>,
) -> Result<PageAnswer, CommandError> {
    let outcome = run_change(
        state,
        id,
        |document| {
            let summary = work(document)?;
            Ok((summary, layout_of(document, page)?))
        },
        |_| true,
    )?;
    match outcome {
        Outcome::Made(document, (summary, layout)) | Outcome::Held(document, (summary, layout)) => {
            Ok(PageAnswer {
                document,
                layout,
                summary,
            })
        }
        // page changes take no ids, so nothing can be out of date
        Outcome::Stale(_) => Err(CommandError::failed(EditError::UnknownObject.to_string())),
    }
}

/// a page's margins and boxes, for the Page Setup sheet. changes nothing.
#[tauri::command(async)]
pub fn edit_page_layout(
    state: State<AppState>,
    id: u64,
    page: usize,
) -> Result<PageLayout, CommandError> {
    state.read_document(DocumentId(id), |document| {
        layout_of(document, page).map_err(|error| CommandError::failed(error.to_string()))
    })
}

/// moves the content to new margins, scaled to fit or only shifted.
#[tauri::command(async)]
pub fn edit_set_margins(
    state: State<AppState>,
    id: u64,
    page: usize,
    margins: Margins,
    fit: MarginFit,
    consent: Option<EditConsent>,
) -> Result<PageAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    page_change(&state, id, page, |document| {
        Ok(set_page_margins(document, page, &margins, fit, &consent)?.summary)
    })
}

/// puts the page on new paper (points), its content fitted, filling or
/// kept at its size.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn edit_resize_page(
    state: State<AppState>,
    id: u64,
    page: usize,
    width: f32,
    height: f32,
    fit: PaperFit,
    consent: Option<EditConsent>,
) -> Result<PageAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    page_change(&state, id, page, |document| {
        Ok(resize_page(document, page, width, height, fit, &consent)?.summary)
    })
}

/// adds (or, negative, takes away) white space on one side as seen.
#[tauri::command(async)]
pub fn edit_add_page_space(
    state: State<AppState>,
    id: u64,
    page: usize,
    side: Side,
    points: f32,
    consent: Option<EditConsent>,
) -> Result<PageAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    page_change(&state, id, page, |document| {
        add_page_space(document, page, side, points, &consent)?;
        Ok(changed_page(page))
    })
}

/// sets the given page boxes to exact numbers.
#[tauri::command(async)]
pub fn edit_set_page_boxes(
    state: State<AppState>,
    id: u64,
    page: usize,
    boxes: PageBoxes,
    consent: Option<EditConsent>,
) -> Result<PageAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    page_change(&state, id, page, |document| {
        set_page_boxes(document, page, &boxes, &consent)?;
        Ok(changed_page(page))
    })
}

/// a summary for a change that only touched the page's boxes.
fn changed_page(page: usize) -> ChangesSummary {
    ChangesSummary {
        pages_changed: vec![page],
        ..ChangesSummary::default()
    }
}
