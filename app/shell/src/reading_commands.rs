//! reading and writing long documents: where the reader stopped, their
//! bookmarks and reading speed, a pdf's printed page numbers, the page
//! finder, the writer's goals and daily log, and saved versions.
//!
//! everything is kept in the app's data folder (shell-core/src/reading);
//! the only command that changes a file is "Save Bookmarks into the PDF",
//! and that goes through an edit like every other tool, so it can be
//! undone and waits for Save.

use letters_pdf::DocumentId;
use letters_search::{resolve_jump, JumpCandidate, JumpDocument};
use letters_shell_core::reading::labels::page_labels;
use letters_shell_core::reading::outline::{add_to_outline, OutlineBookmark};
use letters_shell_core::reading::{
    Bookmark, DocumentReading, ReadingStore, Version, VersionStore, WritingLog,
};
use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

pub struct ReadingState {
    store: ReadingStore,
    versions: VersionStore,
}

impl ReadingState {
    pub fn start(app: &AppHandle) -> ReadingState {
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        ReadingState {
            store: ReadingStore::load(data_dir.join("reading.json")),
            versions: VersionStore::new(data_dir.join("versions")),
        }
    }

    /// organise: bookmarks, position, speed, goals and versions follow a moved file.
    pub fn document_moved(&self, from: &str, to: &str) {
        self.store.move_document(from, to);
        if let Err(error) = self.versions.move_document(from, to) {
            eprintln!("versions did not follow {from}: {error}");
        }
    }
}

#[tauri::command(async)]
pub fn reading_document(reading: State<ReadingState>, path: String) -> DocumentReading {
    reading.store.document(&path)
}

#[tauri::command(async)]
pub fn reading_set_position(
    reading: State<ReadingState>,
    path: String,
    page: u32,
    fraction: f32,
    block: Option<u32>,
) {
    reading.store.set_position(&path, page, fraction, block);
}

#[tauri::command(async)]
pub fn reading_clear_position(reading: State<ReadingState>, path: String) {
    reading.store.clear_position(&path);
}

#[tauri::command(async)]
pub fn reading_add_bookmark(
    reading: State<ReadingState>,
    path: String,
    name: String,
    page: u32,
    block: Option<u32>,
) -> Bookmark {
    reading.store.add_bookmark(&path, &name, page, block)
}

#[tauri::command(async)]
pub fn reading_rename_bookmark(
    reading: State<ReadingState>,
    path: String,
    id: String,
    name: String,
) -> bool {
    reading.store.rename_bookmark(&path, &id, &name)
}

#[tauri::command(async)]
pub fn reading_remove_bookmark(reading: State<ReadingState>, path: String, id: String) -> bool {
    reading.store.remove_bookmark(&path, &id)
}

/// one measured stretch of time on a page; returns the reader's speed.
#[tauri::command(async)]
pub fn reading_record_page_time(
    reading: State<ReadingState>,
    path: String,
    seconds: f32,
) -> Option<f32> {
    reading.store.record_page_time(&path, seconds)
}

/// the printed page numbers of an open pdf, or none when it has none.
#[tauri::command(async)]
pub fn reading_page_labels(state: State<AppState>, id: u64) -> Result<Vec<String>, CommandError> {
    state.read_document(DocumentId(id), |document| Ok(page_labels(document)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkToSave {
    name: String,
    page: u32,
}

/// "Save Bookmarks into the PDF": writes them as outline entries through
/// an edit. the file changes only when the document is saved.
#[tauri::command(async)]
pub fn reading_save_bookmarks_to_pdf(
    state: State<AppState>,
    id: u64,
    bookmarks: Vec<BookmarkToSave>,
) -> Result<DocumentState, CommandError> {
    let entries: Vec<OutlineBookmark> = bookmarks
        .into_iter()
        .map(|b| OutlineBookmark {
            title: b.name,
            page: b.page,
        })
        .collect();
    state.edit(DocumentId(id), |document| {
        add_to_outline(document, &entries).map(drop)
    })
}

/// the page finder's answer for what the reader typed. the interface has
/// the same rules in lib/reading/jump.ts and answers on its own while
/// typing; this is the engine's version, for anything without the ui.
#[tauri::command(async)]
pub fn reading_resolve_jump(document: JumpDocument, text: String) -> Vec<JumpCandidate> {
    resolve_jump(&document, &text)
}

#[tauri::command(async)]
pub fn writing_log(reading: State<ReadingState>) -> WritingLog {
    reading.store.writing_log()
}

#[tauri::command(async)]
pub fn writing_record_words(reading: State<ReadingState>, day: String, words: u32) -> WritingLog {
    reading.store.record_words(&day, words)
}

#[tauri::command(async)]
pub fn writing_set_daily_goal(reading: State<ReadingState>, words: u32) -> WritingLog {
    reading.store.set_daily_goal(words)
}

#[tauri::command(async)]
pub fn writing_set_goals(
    reading: State<ReadingState>,
    path: String,
    word_goal: Option<u32>,
    session_goal: Option<u32>,
) -> DocumentReading {
    reading.store.set_goals(&path, word_goal, session_goal);
    reading.store.document(&path)
}

#[tauri::command(async)]
pub fn versions_list(reading: State<ReadingState>, path: String) -> Vec<Version> {
    reading.versions.list(&path)
}

#[tauri::command(async)]
pub fn versions_save(
    reading: State<ReadingState>,
    path: String,
    name: String,
    words: u32,
) -> Result<Version, CommandError> {
    reading
        .versions
        .save(&path, &name, words)
        .map_err(CommandError::failed)
}

#[tauri::command(async)]
pub fn versions_restore(
    reading: State<ReadingState>,
    path: String,
    id: String,
    words: u32,
) -> Result<Version, CommandError> {
    reading
        .versions
        .restore(&path, &id, words)
        .map_err(CommandError::failed)
}

#[tauri::command(async)]
pub fn versions_delete(
    reading: State<ReadingState>,
    path: String,
    id: String,
) -> Result<(), CommandError> {
    reading
        .versions
        .delete(&path, &id)
        .map_err(CommandError::failed)
}
