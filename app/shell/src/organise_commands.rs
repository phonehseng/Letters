//! organising documents from Recents, pinned folders and search results:
//! move, copy, rename, undo, recycle, reveal and new folder.
//!
//! the file work is in shell-core/src/organise; this file runs it and then
//! keeps the rest of the app in step with each change: Recents, the
//! search index, bookmarks and reading positions, saved versions, and any
//! open pdf of the file (which follows it and keeps working).

use std::path::{Path, PathBuf};

use letters_shell_core::organise::{self, Conflict, Done, OrganiseError, Outcome};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::errors::CommandError;
use crate::reading_commands::ReadingState;
use crate::search_commands::SearchState;
use crate::state::AppState;

/// what a batch did: the finished operations (for Undo), the files left
/// alone, and the first name clash, which stops the batch so the person
/// can choose Keep Both, Replace or Skip for it and the rest.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrganiseReport {
    done: Vec<Done>,
    skipped: Vec<String>,
    /// the file that met a clash, and the files after it that were not tried
    conflict: Option<ConflictReport>,
    problem: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictReport {
    path: String,
    existing: String,
    remaining: Vec<String>,
}

/// the event every window gets when an open document's file moved, so it
/// shows the new name and saves to the new place.
pub const DOCUMENT_MOVED_EVENT: &str = "document-moved";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MovedPayload {
    ids: Vec<u64>,
    from: String,
    to: String,
}

/// after a move or rename: everything that knew the old path learns the new one.
fn follow(app: &AppHandle, from: &str, to: &str) {
    let state = app.state::<AppState>();
    {
        let mut recents = state.recents.lock().unwrap();
        let known = recents
            .list()
            .iter()
            .find(|f| f.path == from)
            .map(|f| f.page_count);
        if let Some(page_count) = known {
            recents.remove(from);
            recents.add(Path::new(to), page_count);
        }
    }
    let ids = state.documents.follow_move(Path::new(from), Path::new(to));
    if let Some(search) = app.try_state::<SearchState>() {
        search.document_moved(from, to);
    }
    if let Some(reading) = app.try_state::<ReadingState>() {
        reading.document_moved(from, to);
    }
    if !ids.is_empty() {
        let payload = MovedPayload {
            ids: ids.into_iter().map(|id| id.0).collect(),
            from: from.into(),
            to: to.into(),
        };
        let _ = app.emit(DOCUMENT_MOVED_EVENT, payload);
    }
}

fn record(app: &AppHandle, report: &mut OrganiseReport, outcome: Outcome) {
    match outcome {
        Outcome::Done { done } => {
            if let Some((from, to)) = done.change() {
                follow(app, from, to);
            }
            report.done.push(done);
        }
        Outcome::Skipped { path } => report.skipped.push(path),
    }
}

/// runs `operation` on each file in turn, stopping at a name clash.
fn batch(
    app: &AppHandle,
    paths: Vec<String>,
    operation: impl Fn(&Path) -> Result<Outcome, OrganiseError>,
) -> OrganiseReport {
    let mut report = OrganiseReport::default();
    for (index, path) in paths.iter().enumerate() {
        match operation(Path::new(path)) {
            Ok(outcome) => record(app, &mut report, outcome),
            Err(OrganiseError::Exists { existing }) => {
                report.conflict = Some(ConflictReport {
                    path: path.clone(),
                    existing,
                    remaining: paths[index + 1..].to_vec(),
                });
                break;
            }
            Err(OrganiseError::Message(message)) => {
                report.problem = Some(message);
                break;
            }
        }
    }
    report
}

#[tauri::command(async)]
pub fn organise_move(
    app: AppHandle,
    paths: Vec<String>,
    folder: String,
    conflict: Conflict,
) -> OrganiseReport {
    let folder = PathBuf::from(folder);
    batch(&app, paths, |path| {
        organise::move_into(path, &folder, conflict)
    })
}

#[tauri::command(async)]
pub fn organise_copy(
    app: AppHandle,
    paths: Vec<String>,
    folder: String,
    conflict: Conflict,
) -> OrganiseReport {
    let folder = PathBuf::from(folder);
    batch(&app, paths, |path| {
        organise::copy_into(path, &folder, conflict)
    })
}

#[tauri::command(async)]
pub fn organise_rename(app: AppHandle, path: String, name: String) -> OrganiseReport {
    batch(&app, vec![path], |path| organise::rename(path, &name))
}

/// Undo: the operations are taken back newest first.
#[tauri::command(async)]
pub fn organise_undo(app: AppHandle, done: Vec<Done>) -> OrganiseReport {
    let mut report = OrganiseReport::default();
    for operation in done.iter().rev() {
        match organise::undo(operation) {
            Ok(undone) => record(&app, &mut report, Outcome::Done { done: undone }),
            Err(error) => {
                report.problem = Some(error.to_string());
                break;
            }
        }
    }
    report
}

/// "Move to Recycle Bin": the system's recycle bin, never a permanent delete.
#[tauri::command(async)]
pub fn organise_recycle(state: State<AppState>, paths: Vec<String>) -> Result<usize, CommandError> {
    let mut recycled = 0;
    for path in &paths {
        organise::trash::to_recycle_bin(Path::new(path))
            .map_err(|e| CommandError::failed(e.to_string()))?;
        state.recents.lock().unwrap().remove(path);
        recycled += 1;
    }
    Ok(recycled)
}

/// shows the file selected in File Explorer (the Finder on a mac).
#[tauri::command(async)]
pub fn organise_reveal(path: String) -> Result<(), CommandError> {
    #[cfg(windows)]
    let spawned = std::process::Command::new("explorer")
        .arg(format!("/select,{path}"))
        .spawn();
    #[cfg(not(windows))]
    let spawned = std::process::Command::new("open")
        .args(["-R", &path])
        .spawn();
    spawned
        .map(drop)
        .map_err(|e| CommandError::failed(format!("The folder couldn\u{2019}t be shown ({e}).")))
}

/// "New Folder" inside `parent`, named "New Folder" (or "New Folder (2)").
#[tauri::command(async)]
pub fn organise_new_folder(parent: String, name: Option<String>) -> Result<String, CommandError> {
    let wanted = name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| "New Folder".into());
    let folder = organise::unique_name(Path::new(&parent), wanted.trim());
    std::fs::create_dir(&folder)
        .map_err(|e| CommandError::failed(format!("The folder couldn\u{2019}t be made ({e}).")))?;
    Ok(folder.to_string_lossy().into_owned())
}
