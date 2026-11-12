//! the commands of the spreadsheet screen: opening, the cells on screen,
//! edits, undo, saving and recovery copies.
//!
//! the work is done in letters-shell-core (src/sheets), where it is
//! tested; these are the thin doors the ui calls through. the formula
//! help and tips have their own file, sheets_help_commands.rs.

use std::path::{Path, PathBuf};

use letters_sheets::analysis::SelectionStats;
use letters_sheets::edit::{FindOptions, Found};
use letters_sheets::view::ViewCell;
use letters_sheets::{CellRef, Range};
use letters_shell_core::sheets::document::EditOutcome;
use letters_shell_core::sheets::formats::{style_edit, StyleChange};
use letters_shell_core::sheets::history::Edit;
use letters_shell_core::sheets::layout::{resize, set_hidden, Axis};
use letters_shell_core::sheets::look::{self, CopiedCells, SheetInfo, StyleLook};
use letters_shell_core::sheets::navigate::{current_region, edge, Direction};
use letters_shell_core::sheets::paste::{paste_edits, split_tab_separated};
use letters_shell_core::sheets::sessions::{OpenedSheet, SavedSheet, SheetSessions};
use letters_shell_core::writer::thumbnail::save_generic_thumbnail;
use tauri::{AppHandle, Manager, State, Window};

use crate::errors::CommandError;
use crate::state::AppState;

/// the spreadsheets' part of the app's memory, managed beside `AppState`.
pub struct SheetsState {
    pub sessions: SheetSessions,
}

impl SheetsState {
    pub fn start(app: &AppHandle) -> SheetsState {
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        let cache_dir = app.path().app_cache_dir().unwrap_or_default();
        SheetsState {
            sessions: SheetSessions::new(&cache_dir, &data_dir),
        }
    }
}

/// a closed window's workbooks are closed with it (see lib.rs).
pub fn close_window(app: &AppHandle, window: &str) {
    if let Some(sheets) = app.try_state::<SheetsState>() {
        sheets.sessions.close_window(window);
    }
}

fn failed(message: String) -> CommandError {
    CommandError::failed(message)
}

/// Recents lists workbooks too, with the plain page picture the writer uses.
fn remember(state: &AppState, path: &Path) {
    state.recents.lock().unwrap().add(path, 0);
    save_generic_thumbnail(&state.thumbnail_dir, path);
}

#[tauri::command(async)]
pub fn sheets_open(
    window: Window,
    state: State<AppState>,
    sheets: State<SheetsState>,
    path: String,
    recovered: bool,
) -> Result<OpenedSheet, CommandError> {
    let path = PathBuf::from(path);
    let opened = sheets
        .sessions
        .open(window.label(), &path, recovered)
        .map_err(failed)?;
    remember(&state, &path);
    Ok(opened)
}

/// a new workbook: empty, or a starter workbook by name ("budget").
#[tauri::command(async)]
pub fn sheets_new(
    window: Window,
    sheets: State<SheetsState>,
    template: Option<String>,
) -> Result<OpenedSheet, CommandError> {
    sheets
        .sessions
        .new_workbook(window.label(), template.as_deref())
        .map_err(failed)
}

#[tauri::command(async)]
pub fn sheets_close(sheets: State<SheetsState>, id: u64) {
    sheets.sessions.close(id);
}

#[tauri::command(async)]
pub fn sheets_info(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
) -> Result<SheetInfo, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            look::sheet_info(&document.book, sheet)
                .ok_or_else(|| "That sheet doesn’t exist any more.".into())
        })
        .map_err(failed)
}

#[tauri::command(async)]
pub fn sheets_cells(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    range: Range,
) -> Result<Vec<ViewCell>, CommandError> {
    sheets
        .sessions
        .with(id, |document| Ok(look::cells(&document.book, sheet, range)))
        .map_err(failed)
}

/// the look of every style from `from` on (the ui keeps the ones it has).
#[tauri::command(async)]
pub fn sheets_styles(
    sheets: State<SheetsState>,
    id: u64,
    from: usize,
) -> Result<Vec<StyleLook>, CommandError> {
    sheets
        .sessions
        .with(id, |document| Ok(look::style_looks(&document.book, from)))
        .map_err(failed)
}

/// what the formula bar shows for a cell.
#[tauri::command(async)]
pub fn sheets_edit_text(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    row: u32,
    col: u32,
) -> Result<String, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            Ok(letters_sheets::view::edit_text(
                &document.book,
                sheet,
                row,
                col,
            ))
        })
        .map_err(failed)
}

/// one action: the engine's operations and size changes, undone together.
#[tauri::command(async)]
pub fn sheets_apply(
    sheets: State<SheetsState>,
    id: u64,
    edits: Vec<Edit>,
    label: String,
) -> Result<EditOutcome, CommandError> {
    sheets
        .sessions
        .with(id, |document| document.apply(&edits, &label))
        .map_err(failed)
}

/// number formats, bold, colours and presets on the selected ranges.
#[tauri::command(async)]
pub fn sheets_format(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    ranges: Vec<Range>,
    change: StyleChange,
    label: String,
) -> Result<EditOutcome, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            let edit = style_edit(&mut document.book, sheet, &ranges, &change)?;
            document.apply(&[edit], &label)
        })
        .map_err(failed)
}

/// column widths (characters) or row heights (points); none is the default.
#[tauri::command(async)]
pub fn sheets_resize(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    axis: Axis,
    indices: Vec<u32>,
    size: Option<f64>,
) -> Result<EditOutcome, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            let change = resize(&document.book, sheet, axis, &indices, size)?;
            document.apply(&[Edit::Layout(change)], "Resize")
        })
        .map_err(failed)
}

#[tauri::command(async)]
pub fn sheets_hide(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    axis: Axis,
    indices: Vec<u32>,
    hidden: bool,
) -> Result<EditOutcome, CommandError> {
    let label = if hidden { "Hide" } else { "Unhide" };
    sheets
        .sessions
        .with(id, |document| {
            let change = set_hidden(&document.book, sheet, axis, &indices, hidden)?;
            document.apply(&[Edit::Layout(change)], label)
        })
        .map_err(failed)
}

/// cells from another app: tab-separated `text`, or `rows` the ui read
/// from an html table (google sheets and excel put both on the clipboard).
#[tauri::command(async)]
pub fn sheets_paste_text(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    row: u32,
    col: u32,
    text: Option<String>,
    rows: Option<Vec<Vec<String>>>,
) -> Result<EditOutcome, CommandError> {
    let rows = rows.unwrap_or_else(|| split_tab_separated(text.as_deref().unwrap_or_default()));
    let edits = paste_edits(sheet, row, col, &rows).map_err(failed)?;
    sheets
        .sessions
        .with(id, |document| document.apply(&edits, "Paste"))
        .map_err(failed)
}

/// a range as text and html for the clipboard.
#[tauri::command(async)]
pub fn sheets_copy(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    range: Range,
) -> Result<CopiedCells, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            Ok(look::copy_cells(&document.book, sheet, range))
        })
        .map_err(failed)
}

#[tauri::command(async)]
pub fn sheets_undo(
    sheets: State<SheetsState>,
    id: u64,
) -> Result<Option<EditOutcome>, CommandError> {
    sheets
        .sessions
        .with(id, |document| document.undo())
        .map_err(failed)
}

#[tauri::command(async)]
pub fn sheets_redo(
    sheets: State<SheetsState>,
    id: u64,
) -> Result<Option<EditOutcome>, CommandError> {
    sheets
        .sessions
        .with(id, |document| document.redo())
        .map_err(failed)
}

/// saves to the workbook's file, or to `path` (Save As). csv saves `sheet`.
#[tauri::command(async)]
pub fn sheets_save(
    state: State<AppState>,
    sheets: State<SheetsState>,
    id: u64,
    path: Option<String>,
    sheet: usize,
) -> Result<SavedSheet, CommandError> {
    let saved = sheets
        .sessions
        .save(id, path.as_deref().map(Path::new), sheet)
        .map_err(failed)?;
    remember(&state, Path::new(&saved.path));
    Ok(saved)
}

/// what saving under `path` would leave out, said before saving.
#[tauri::command(async)]
pub fn sheets_save_losses(
    sheets: State<SheetsState>,
    id: u64,
    path: String,
) -> Result<Option<String>, CommandError> {
    sheets
        .sessions
        .save_losses(id, Path::new(&path))
        .map_err(failed)
}

#[tauri::command(async)]
pub fn sheets_autosave(sheets: State<SheetsState>, id: u64) -> Result<(), CommandError> {
    sheets.sessions.autosave(id).map_err(failed)
}

#[tauri::command(async)]
pub fn sheets_discard_recovery(sheets: State<SheetsState>, id: u64) -> Result<(), CommandError> {
    sheets.sessions.discard_recovery(id).map_err(failed)
}

/// the status bar's sum, average and count.
#[tauri::command(async)]
pub fn sheets_stats(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    ranges: Vec<Range>,
) -> Result<Option<SelectionStats>, CommandError> {
    sheets
        .sessions
        .with(id, |document| Ok(document.stats(sheet, &ranges)))
        .map_err(failed)
}

/// every cell matching `text`; `sheet` none searches the whole workbook.
#[tauri::command(async)]
pub fn sheets_find(
    sheets: State<SheetsState>,
    id: u64,
    sheet: Option<usize>,
    text: String,
    options: FindOptions,
) -> Result<Vec<Found>, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            Ok(letters_sheets::edit::find(
                &document.book,
                sheet,
                &text,
                &options,
            ))
        })
        .map_err(failed)
}

/// the values a column of a range shows, for the filter menu.
#[tauri::command(async)]
pub fn sheets_column_values(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    range: Range,
    column: u32,
) -> Result<Vec<String>, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            Ok(look::column_values(&document.book, sheet, range, column))
        })
        .map_err(failed)
}

/// where ctrl and an arrow land from `at`.
#[tauri::command(async)]
pub fn sheets_edge(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    at: CellRef,
    direction: Direction,
) -> Result<CellRef, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            let target = document
                .book
                .sheets
                .get(sheet)
                .ok_or("That sheet doesn’t exist any more.")?;
            Ok(edge(target, at, direction))
        })
        .map_err(failed)
}

/// the block of data around `at` (ctrl a's first press).
#[tauri::command(async)]
pub fn sheets_region(
    sheets: State<SheetsState>,
    id: u64,
    sheet: usize,
    at: CellRef,
) -> Result<Range, CommandError> {
    sheets
        .sessions
        .with(id, |document| {
            let target = document
                .book
                .sheets
                .get(sheet)
                .ok_or("That sheet doesn’t exist any more.")?;
            Ok(current_region(target, at))
        })
        .map_err(failed)
}
