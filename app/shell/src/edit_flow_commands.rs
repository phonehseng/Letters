//! the Edit tab's text that flows: typing in a paragraph re-wraps it and
//! moves the column below by whole lines (letters-edit's `reflow_paragraph`),
//! typing in a ruled table cell grows its row (`edit_table_cell`), and new
//! text can take the style of the text nearby (`add_text_matching`).
//!
//! letters the document's font lacks are added to that same font, drawn by
//! letters-fonts' completion from every font of the font library, which is
//! lent to the engine for the length of the edit (`with_font_donors`).
//!
//! when the re-set text no longer fits its column the engine changes
//! nothing and answers the choices; the ui shows them and sends the edit
//! again with the one the person picked.

use letters_edit::{
    self as edit, add_text_matching, content_edit_check, edit_table_cell, reflow_paragraph,
    table_cell_of, with_font_donors, ChangesSummary, EditCheck, EditConsent, FontDonors,
    FontNotice, MatchedText, OverflowReport, PageObjects, ReflowOptions, ReflowResult,
    TextEditOptions,
};
use letters_pdf::DocumentId;
use letters_shell_core::edit::donors::DonorCache;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::documents::DocumentState;
use crate::edit_answers::{content_change, page_objects_now, run_change, ContentAnswer, Outcome};
use crate::errors::CommandError;
use crate::fonts_commands::FontsState;
use crate::state::AppState;

/// what the flowing-text commands keep while the app runs.
pub struct EditFlowState {
    donors: DonorCache,
}

impl EditFlowState {
    pub fn start(app: &AppHandle) -> Self {
        // fitted fonts are remembered here, so the next edit in the same
        // font doesn't fit it again
        let cache = app
            .path()
            .app_cache_dir()
            .ok()
            .map(|dir| dir.join("fonts").join("completed"));
        Self {
            donors: DonorCache::new(cache),
        }
    }

    fn donors(&self, fonts: &FontsState) -> FontDonors {
        self.donors.donors_for(fonts.index())
    }
}

/// what typing in a paragraph or cell answers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowAnswer {
    document: DocumentState,
    objects: PageObjects,
    /// false when nothing changed because the text doesn't fit and the
    /// person must choose (see `overflow`)
    applied: bool,
    notices: Vec<FontNotice>,
    summary: ChangesSummary,
    /// the paragraph's id now
    block_id: Option<String>,
    lines_before: usize,
    lines_after: usize,
    /// points the paragraph grew (negative: shrank)
    height_change: f32,
    overflow: Option<OverflowReport>,
    /// the text was in a ruled table cell
    in_table: bool,
    stale: bool,
}

impl FlowAnswer {
    fn from(document: DocumentState, result: ReflowResult, in_table: bool) -> Self {
        FlowAnswer {
            document,
            objects: result.objects,
            applied: result.applied,
            notices: result.notices,
            summary: result.summary,
            block_id: result.block_id,
            lines_before: result.lines_before,
            lines_after: result.lines_after,
            height_change: result.height_change,
            overflow: result.overflow,
            in_table,
            stale: false,
        }
    }
}

/// whether the file is locked or signed, before the first edit: the ui
/// asks before changing a signed file, naming the signer.
#[tauri::command(async)]
pub fn edit_check(state: State<AppState>, id: u64) -> Result<EditCheck, CommandError> {
    state.read_document(DocumentId(id), |document| Ok(content_edit_check(document)))
}

/// sets a paragraph's (or table cell's) new text, re-wrapped to its width,
/// moving what is below by whole lines. see the module comment for overflow.
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn edit_reflow_text(
    state: State<AppState>,
    flow: State<EditFlowState>,
    fonts: State<FontsState>,
    id: u64,
    page: usize,
    block_id: String,
    text: String,
    options: Option<ReflowOptions>,
) -> Result<FlowAnswer, CommandError> {
    let options = options.unwrap_or_default();
    let donors = flow.donors(&fonts);
    let in_table = state.read_document(DocumentId(id), |document| {
        let objects = edit::page_objects(document, page)
            .map_err(|error| CommandError::failed(error.to_string()))?;
        Ok(table_cell_of(&objects, &block_id).is_some())
    })?;
    let outcome = run_change(
        &state,
        id,
        |document| {
            with_font_donors(&donors, || {
                if in_table {
                    edit_table_cell(document, page, &block_id, &text, &options)
                } else {
                    reflow_paragraph(document, page, &block_id, &text, &options)
                }
            })
        },
        |result| result.applied,
    )?;
    match outcome {
        Outcome::Made(document, result) | Outcome::Held(document, result) => {
            Ok(FlowAnswer::from(document, result, in_table))
        }
        Outcome::Stale(document) => Ok(FlowAnswer {
            document,
            objects: page_objects_now(&state, id, page)?,
            applied: false,
            notices: Vec::new(),
            summary: ChangesSummary::default(),
            block_id: None,
            lines_before: 0,
            lines_after: 0,
            height_change: 0.0,
            overflow: None,
            in_table,
            stale: true,
        }),
    }
}

/// adds new text in the style of the nearest paragraph (or `likeBlock`):
/// its font, size, spacing and colour, adding letters to its font as needed.
#[tauri::command(async)]
pub fn edit_add_matching_text(
    state: State<AppState>,
    flow: State<EditFlowState>,
    fonts: State<FontsState>,
    id: u64,
    page: usize,
    added: MatchedText,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let donors = flow.donors(&fonts);
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        with_font_donors(&donors, || {
            add_text_matching(
                document,
                page,
                &added,
                &TextEditOptions::default(),
                &consent,
            )
        })
    })
}
