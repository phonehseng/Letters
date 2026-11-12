//! the spreadsheet screen's help for newcomers: autocomplete, argument
//! hints, reference colours, F4, "explain this formula", "why is this
//! cell an error?", tips, shortcuts, friendly formats and style presets.
//!
//! all of it is the engine's (core/sheets `help`); the cursor offsets go
//! through shell-core's sheets/entry.rs so they count as javascript does.

use letters_sheets::help::catalogue::{function_info, FunctionInfo};
use letters_sheets::help::entry::{auto_close, ArgumentHint};
use letters_sheets::help::errors::{circular, diagnose, ErrorHelp};
use letters_sheets::help::explain::{evaluate_steps, explain_formula, Step};
use letters_sheets::help::presets::{friendly_formats, style_presets, FriendlyFormat};
use letters_sheets::help::search::{complete, Completion};
use letters_sheets::help::shortcuts::{shortcuts, Shortcut};
use letters_sheets::help::suggest::{RecentAction, Suggestion};
use letters_sheets::{CellRef, Range};
use letters_shell_core::sheets::entry::{self, ColouredReference, Typed};
use tauri::State;

use crate::errors::CommandError;
use crate::sheets_commands::SheetsState;

fn failed(message: String) -> CommandError {
    CommandError::failed(message)
}

/// functions for what is typed: by name, by a close spelling, or by purpose.
#[tauri::command(async)]
pub fn sheets_complete(typed: String, limit: usize) -> Vec<Completion> {
    complete(&typed, limit)
}

#[tauri::command(async)]
pub fn sheets_function_info(name: String) -> Option<FunctionInfo> {
    function_info(&name)
}

#[tauri::command(async)]
pub fn sheets_argument_hint(formula: String, caret: usize) -> Option<ArgumentHint> {
    entry::argument_hint(&formula, caret)
}

#[tauri::command(async)]
pub fn sheets_reference_spans(formula: String) -> Vec<ColouredReference> {
    entry::reference_spans(&formula)
}

/// a range clicked while typing a formula, put in at the cursor.
#[tauri::command(async)]
pub fn sheets_insert_reference(formula: String, caret: usize, reference: String) -> Typed {
    entry::insert_reference(&formula, caret, &reference)
}

#[tauri::command(async)]
pub fn sheets_expects_reference(formula: String, caret: usize) -> bool {
    entry::expects_reference(&formula, caret)
}

/// F4: A1, $A$1, A$1, $A1.
#[tauri::command(async)]
pub fn sheets_cycle_reference(formula: String, caret: usize) -> Typed {
    entry::cycle_reference(&formula, caret)
}

/// closes brackets and quotes left open, as excel does on Enter.
#[tauri::command(async)]
pub fn sheets_auto_close(formula: String) -> String {
    auto_close(&formula)
}

/// "explain this formula": each part in a sentence, outermost first, with
/// its value when the formula is a cell's.
#[tauri::command(async)]
pub fn sheets_explain(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    row: u32,
    col: u32,
    text: String,
) -> Result<Vec<Step>, CommandError> {
    let text = text.trim_start_matches('=').to_string();
    sheets
        .sessions
        .with(id, |document| {
            let steps = evaluate_steps(&document.book, sheet, CellRef::new(row, col), &text);
            Ok(if steps.is_empty() {
                explain_formula(&text)
            } else {
                steps
            })
        })
        .map_err(failed)
}

/// "why is this cell an error?", with one-click fixes; a cell in a loop
/// gets the circular reference help instead.
#[tauri::command(async)]
pub fn sheets_diagnose(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    row: u32,
    col: u32,
) -> Result<Option<ErrorHelp>, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            let key = (sheet, row, col);
            if document.circular_cells().contains(&key) {
                return Ok(Some(circular(document.circular_cells(), &document.book)));
            }
            Ok(diagnose(&document.book, sheet, CellRef::new(row, col)))
        })
        .map_err(failed)
}

/// the tips for the selection, without the ones dismissed.
#[tauri::command(async)]
pub fn sheets_tips(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    selection: Range,
    recent: Vec<RecentAction>,
) -> Result<Vec<Suggestion>, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            Ok(sheets
                .sessions
                .tips
                .tips_for(&document.book, sheet, selection, &recent))
        })
        .map_err(failed)
}

/// `whole_kind`: never offer this kind of tip again.
#[tauri::command(async)]
pub fn sheets_dismiss_tip(
    sheets: State<SheetsState>,
    tip: String,
    whole_kind: bool,
) -> Result<(), CommandError> {
    sheets
        .sessions
        .tips
        .dismiss(&tip, whole_kind)
        .map_err(failed)
}

#[tauri::command(async)]
pub fn sheets_shortcuts() -> Vec<Shortcut> {
    shortcuts()
}

#[tauri::command(async)]
pub fn sheets_friendly_formats() -> Vec<FriendlyFormat> {
    friendly_formats()
}

/// the names of the engine's cell style presets ("Title", "Heading 1"...).
#[tauri::command(async)]
pub fn sheets_style_presets() -> Vec<&'static str> {
    style_presets()
        .into_iter()
        .map(|preset| preset.name)
        .collect()
}
