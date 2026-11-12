//! the functions the ui can call.
//!
//! every command is marked `async` so it runs on a background thread:
//! opening a big file must never freeze a window.

use std::fs;
use std::path::{Path, PathBuf};

use letters_pdf::{
    DocumentId, OutlineItem, RenderWorker, SearchMatch, SearchResults, TextRect, TileKey,
};
use serde::Serialize;
use tauri::{AppHandle, State, Window};

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;
use crate::windows;
use letters_shell_core::pinned::{describe_folders, PinnedFolder};
use letters_shell_core::recents::{thumbnail_name, RecentFile};
use letters_shell_core::session::with_pdf_extension;

/// thumbnails on the home screen are drawn about this wide.
const THUMBNAIL_WIDTH_PIXELS: f32 = 240.0;

fn engine_unavailable(reason: String) -> CommandError {
    CommandError::engine_unavailable(reason)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStats {
    open_documents: usize,
    cached_tiles: usize,
    cached_bytes: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineItemDto {
    title: String,
    /// zero based
    page: u32,
    /// 0 for top level entries
    level: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResultsDto {
    matches: Vec<SearchMatchDto>,
    /// true when the search stopped at 500 matches and there were more
    truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatchDto {
    /// zero based
    page: u32,
    rects: Vec<RectDto>,
    snippet: String,
}

/// a rectangle in points from the page's top left corner, y downward.
#[derive(Serialize)]
pub struct RectDto {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

/// opens a pdf for viewing and editing.
///
/// the engine shows a working copy in the app's cache, never the file
/// itself, so edits stay away from the person's file until they save.
#[tauri::command(async)]
pub fn open_document(
    window: Window,
    state: State<AppState>,
    path: String,
    password: Option<String>,
) -> Result<DocumentState, CommandError> {
    let worker = state.worker().map_err(engine_unavailable)?;
    let path = PathBuf::from(path);
    let opened = state.documents.open(worker, &path, password.as_deref())?;
    state.remember_document(window.label(), DocumentId(opened.id));
    remember_in_recents(&state, worker, &opened);
    Ok(opened)
}

/// closes a document and throws away any unsaved changes. the ui asks
/// the person before it calls this when there are some.
#[tauri::command(async)]
pub fn close_document(window: Window, state: State<AppState>, id: u64) {
    state.close_document(window.label(), DocumentId(id));
}

/// the document as it is now: pages, revision, and whether it has
/// unsaved changes or anything to undo.
#[tauri::command(async)]
pub fn document_state(state: State<AppState>, id: u64) -> Result<DocumentState, CommandError> {
    state.documents.state(DocumentId(id))
}

#[tauri::command(async)]
pub fn undo(state: State<AppState>, id: u64) -> Result<DocumentState, CommandError> {
    let worker = state.worker().map_err(engine_unavailable)?;
    state.documents.undo(worker, DocumentId(id))
}

#[tauri::command(async)]
pub fn redo(state: State<AppState>, id: u64) -> Result<DocumentState, CommandError> {
    let worker = state.worker().map_err(engine_unavailable)?;
    state.documents.redo(worker, DocumentId(id))
}

/// writes the changes to the document's file.
#[tauri::command(async)]
pub fn save_document(state: State<AppState>, id: u64) -> Result<DocumentState, CommandError> {
    let saved = state.documents.save(DocumentId(id))?;
    if let Ok(worker) = state.worker() {
        remember_in_recents(&state, worker, &saved);
    }
    Ok(saved)
}

/// writes the document to a new file, which it then belongs to.
#[tauri::command(async)]
pub fn save_document_as(
    state: State<AppState>,
    id: u64,
    path: String,
) -> Result<DocumentState, CommandError> {
    let destination = with_pdf_extension(Path::new(&path));
    let saved = state.documents.save_as(DocumentId(id), &destination)?;
    if let Ok(worker) = state.worker() {
        remember_in_recents(&state, worker, &saved);
    }
    Ok(saved)
}

/// the file this window was opened for, if any. answers only once, so
/// reloading the window does not reopen the file a second time.
#[tauri::command(async)]
pub fn take_startup_file(window: Window, state: State<AppState>) -> Option<String> {
    state
        .take_startup_file(window.label())
        .map(|file| file.to_string_lossy().into_owned())
}

#[tauri::command(async)]
pub fn new_window(app: AppHandle, path: Option<String>) {
    windows::open_window(&app, path.map(PathBuf::from));
}

/// recent files that still exist on disk, newest first.
#[tauri::command(async)]
pub fn recent_files(state: State<AppState>) -> Vec<RecentFile> {
    let recents = state.recents.lock().unwrap();
    recents
        .list()
        .iter()
        .filter(|file| Path::new(&file.path).is_file())
        .cloned()
        .collect()
}

#[tauri::command(async)]
pub fn forget_recent_file(state: State<AppState>, path: String) {
    state.recents.lock().unwrap().remove(&path);
}

#[tauri::command(async)]
pub fn engine_stats(state: State<AppState>) -> Result<EngineStats, CommandError> {
    let stats = state.worker().map_err(engine_unavailable)?.stats()?;
    Ok(EngineStats {
        open_documents: stats.open_documents,
        cached_tiles: stats.cached_tiles,
        cached_bytes: stats.cached_bytes,
    })
}

/// the document's table of contents, flattened in reading order.
/// empty when the pdf has none.
#[tauri::command(async)]
pub fn document_outline(
    state: State<AppState>,
    id: u64,
) -> Result<Vec<OutlineItemDto>, CommandError> {
    let worker = state.worker().map_err(engine_unavailable)?;
    let outline = worker.outline(DocumentId(id))?;
    Ok(outline.into_iter().map(outline_item_dto).collect())
}

/// every place `query` appears in the document, ignoring case.
#[tauri::command(async)]
pub fn search_text(
    state: State<AppState>,
    id: u64,
    query: String,
) -> Result<SearchResultsDto, CommandError> {
    let worker = state.worker().map_err(engine_unavailable)?;
    let results = worker.search(DocumentId(id), &query)?;
    Ok(search_results_dto(results))
}

/// the pinned folders with the pdfs in each, read fresh from disk.
#[tauri::command(async)]
pub fn pinned_folders(state: State<AppState>) -> Vec<PinnedFolder> {
    // copy the paths and let go of the lock before reading the disk,
    // which can be slow for a network folder
    let paths = state.pinned_folders.lock().unwrap().paths().to_vec();
    describe_folders(&paths)
}

#[tauri::command(async)]
pub fn pin_folder(state: State<AppState>, path: String) -> Vec<PinnedFolder> {
    state.pinned_folders.lock().unwrap().pin(&path);
    pinned_folders(state)
}

#[tauri::command(async)]
pub fn unpin_folder(state: State<AppState>, path: String) -> Vec<PinnedFolder> {
    state.pinned_folders.lock().unwrap().unpin(&path);
    pinned_folders(state)
}

fn outline_item_dto(item: OutlineItem) -> OutlineItemDto {
    OutlineItemDto {
        title: item.title,
        page: item.page,
        level: item.level,
    }
}

fn search_results_dto(results: SearchResults) -> SearchResultsDto {
    SearchResultsDto {
        matches: results.matches.into_iter().map(search_match_dto).collect(),
        truncated: results.truncated,
    }
}

fn search_match_dto(found: SearchMatch) -> SearchMatchDto {
    SearchMatchDto {
        page: found.page,
        rects: found.rects.iter().map(rect_dto).collect(),
        snippet: found.snippet,
    }
}

fn rect_dto(rect: &TextRect) -> RectDto {
    RectDto {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

/// puts the document at the top of Recents with a fresh picture of its
/// first page. called after opening and after saving, so Recents shows
/// the file as it is on disk.
fn remember_in_recents(state: &AppState, worker: &RenderWorker, document: &DocumentState) {
    let path = Path::new(&document.path);
    state
        .recents
        .lock()
        .unwrap()
        .add(path, document.pages.len());
    save_thumbnail(worker, document, &state.thumbnail_dir, path);
}

/// draws a small picture of the first page for the home screen.
///
/// a failed thumbnail is not worth failing the open for, so errors are ignored.
fn save_thumbnail(
    worker: &RenderWorker,
    document: &DocumentState,
    thumbnail_dir: &Path,
    path: &Path,
) {
    let Some(first_page) = document.pages.first() else {
        return;
    };
    let scale = THUMBNAIL_WIDTH_PIXELS / first_page.width.max(1.0);
    let key = TileKey {
        document: DocumentId(document.id),
        page: 0,
        scale_milli: (scale * 1000.0) as u32,
        column: 0,
        row: 0,
    };
    if let Ok(tile) = worker.tile(key) {
        let _ = fs::create_dir_all(thumbnail_dir);
        let _ = fs::write(
            thumbnail_dir.join(format!("{}.png", thumbnail_name(path))),
            tile.as_slice(),
        );
    }
}
