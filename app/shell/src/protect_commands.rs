//! the Protect tools: redaction, hidden information and passwords.
//!
//! the engine is core/redact; the rules the app adds around it are in
//! letters_shell_core::protect. every command that changes the document
//! is one call through `state.edit`, like the page tools, and then tells
//! the session how to save from now on:
//!
//! - after a redaction or a scrub, every save writes the whole file from
//!   scratch (`save_clean`) and deletes the versions kept for undo, which
//!   still hold what was removed. that can't be switched off again.
//! - a password is not part of the working copy (the viewer must still
//!   be able to show it); it is applied to the file save writes.
//!
//! reading commands (the preview, finding text, the hidden information
//! list) go through `state.read_document` and make no undo step.

use std::fs;
use std::path::{Path, PathBuf};

use letters_pdf::DocumentId;
use letters_redact::{
    self as redact, find_text_areas, FindOptions, FoundText, LockOptions, RedactError,
    RedactOptions, RedactionArea, RedactionPreview, RedactionReport, ScrubItem, ScrubOptions,
    ScrubReport,
};
use letters_shell_core::protect::clean_save::{clean_writer, rewrite_clean, LockOnSave};
use letters_shell_core::protect::protection::{
    merge_lock, LockChange, Protection, ProtectionStatus,
};
use letters_shell_core::protect::unlock::{
    inspect_lock, unlock_for_editing, FileLock, PasswordUsed,
};
use letters_shell_core::recents::thumbnail_name;
use letters_shell_core::session::EditSession;
use serde::Serialize;
use tauri::State;

use crate::documents::{page_dto, DocumentState, PageDto};
use crate::errors::CommandError;
use crate::state::AppState;

/// what applying redactions answers: the document now, and what was removed.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionEdit {
    pub document: DocumentState,
    pub report: RedactionReport,
}

/// what removing hidden information answers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScrubEdit {
    pub document: DocumentState,
    pub report: ScrubReport,
}

/// a redacted copy opened for the review's before and after pictures.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRun {
    /// the engine's id for the copy; its tiles are drawn like any document's
    pub preview_id: u64,
    pub pages: Vec<PageDto>,
    /// what the redaction would remove and its warnings, from a real run
    pub report: RedactionReport,
}

fn engine_error(error: RedactError) -> CommandError {
    CommandError::failed(error.to_string())
}

// ---- redaction ----

/// boxes for a phrase or a kind of text (email addresses, phone
/// numbers, long numbers), on `pages` or on every page when empty.
#[tauri::command(async)]
pub fn find_redaction_text(
    state: State<AppState>,
    id: u64,
    query: redact::TextQuery,
    pages: Vec<u32>,
) -> Result<Vec<FoundText>, CommandError> {
    let options = FindOptions { pages };
    state.read_document(DocumentId(id), |document| {
        find_text_areas(document, &query, &options).map_err(engine_error)
    })
}

/// what redacting the marked areas would remove, area by area.
#[tauri::command(async)]
pub fn redaction_preview(
    state: State<AppState>,
    id: u64,
    areas: Vec<RedactionArea>,
) -> Result<RedactionPreview, CommandError> {
    state.read_document(DocumentId(id), |document| {
        redact::preview(document, &areas).map_err(engine_error)
    })
}

/// redacts a copy of the document, for the review's after picture and
/// its warnings. the working copy is not touched: the copy is written to
/// the session's folder (deleted with it) and opened in the engine on
/// its own, until `close_redaction_dry_run`.
#[tauri::command(async)]
pub fn redaction_dry_run(
    state: State<AppState>,
    id: u64,
    areas: Vec<RedactionArea>,
    options: RedactOptions,
) -> Result<DryRun, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let (bytes, report) = state.read_document(DocumentId(id), |document| {
        let mut copy = document.clone();
        let report = redact::redact(&mut copy, &areas, &options).map_err(engine_error)?;
        let bytes = redact::save_clean(&mut copy).map_err(engine_error)?;
        Ok((bytes, report))
    })?;
    let path = dry_run_path(&state, id)?;
    fs::write(&path, bytes).map_err(|error| CommandError::failed(error.to_string()))?;
    let info = match worker.open(&path, None) {
        Ok(info) => info,
        Err(error) => {
            let _ = fs::remove_file(&path);
            return Err(error.into());
        }
    };
    state
        .protection
        .update(id, |protection| protection.previews.push((info.id.0, path)));
    Ok(DryRun {
        preview_id: info.id.0,
        pages: info.page_sizes.iter().map(page_dto).collect(),
        report,
    })
}

/// a new file name in the document's session folder.
fn dry_run_path(state: &AppState, id: u64) -> Result<PathBuf, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let (folder, _) = state
        .documents
        .with_session(worker, DocumentId(id), |session, _| {
            Ok((session_folder(session), None))
        })?;
    let taken = state.protection.get(id).previews.len();
    Ok(folder.join(format!(
        "redaction-preview-{}-{taken}.pdf",
        std::process::id()
    )))
}

fn session_folder(session: &EditSession) -> PathBuf {
    session
        .working_copy()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// closes the review's copy and deletes it.
#[tauri::command(async)]
pub fn close_redaction_dry_run(state: State<AppState>, id: u64, preview_id: u64) {
    let found = state.protection.update(id, |protection| {
        let index = protection
            .previews
            .iter()
            .position(|(each, _)| *each == preview_id)?;
        Some(protection.previews.remove(index))
    });
    if let Some(preview) = found {
        close_preview(&state, preview);
    }
}

fn close_preview(state: &AppState, (preview_id, path): (u64, PathBuf)) {
    if let Ok(worker) = state.worker() {
        worker.close(DocumentId(preview_id));
    }
    let _ = fs::remove_file(path);
}

/// removes everything under the areas for good, then draws the boxes.
/// from then on every save of this document is a clean one.
#[tauri::command(async)]
pub fn apply_redactions(
    state: State<AppState>,
    id: u64,
    areas: Vec<RedactionArea>,
    options: RedactOptions,
) -> Result<RedactionEdit, CommandError> {
    let mut report = None;
    let document = state.edit(DocumentId(id), |document| {
        report = Some(redact::redact(document, &areas, &options)?);
        // the stored version must not keep the old content in unused objects
        rewrite_clean(document)
    })?;
    after_removing_content(&state, id)?;
    Ok(RedactionEdit {
        document,
        report: report.unwrap_or_default(),
    })
}

/// what every hidden kind of information in the document is.
#[tauri::command(async)]
pub fn hidden_information(state: State<AppState>, id: u64) -> Result<ScrubReport, CommandError> {
    state.read_document(DocumentId(id), |document| {
        redact::scrub_preview(document).map_err(engine_error)
    })
}

/// removes the chosen kinds of hidden information.
#[tauri::command(async)]
pub fn remove_hidden_information(
    state: State<AppState>,
    id: u64,
    items: Vec<ScrubItem>,
) -> Result<ScrubEdit, CommandError> {
    let options = ScrubOptions { items };
    let mut report = None;
    let document = state.edit(DocumentId(id), |document| {
        report = Some(redact::scrub(document, &options)?);
        rewrite_clean(document)
    })?;
    after_removing_content(&state, id)?;
    Ok(ScrubEdit {
        document,
        report: report.unwrap_or_default(),
    })
}

/// a redaction or a scrub was made: saves are clean from now on, and
/// the history goes once saved.
fn after_removing_content(state: &AppState, id: u64) -> Result<(), CommandError> {
    let lock = state.protection.update(id, |protection| {
        protection.removed_content = true;
        lock_on_save(protection)
    });
    set_writer(state, id, lock, true).map(drop)
}

/// what save should do about a password, as things stand.
fn lock_on_save(protection: &Protection) -> LockOnSave {
    protection.lock.clone().unwrap_or(LockOnSave::Unlocked)
}

/// gives the session its clean save writer.
fn set_writer(
    state: &AppState,
    id: u64,
    lock: LockOnSave,
    forget_history: bool,
) -> Result<DocumentState, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let ((), document) = state
        .documents
        .with_session(worker, DocumentId(id), |session, _| {
            session.write_saves_with(clean_writer(lock), forget_history);
            Ok(((), None))
        })?;
    Ok(document)
}

// ---- passwords ----

/// how the document is protected, and what save will do.
#[tauri::command(async)]
pub fn protection_status(
    state: State<AppState>,
    id: u64,
) -> Result<ProtectionStatus, CommandError> {
    status_of(&state, id)
}

fn status_of(state: &AppState, id: u64) -> Result<ProtectionStatus, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let (status, _) = state
        .documents
        .with_session(worker, DocumentId(id), |session, _| {
            let known = state.protection.get(id).working_lock;
            let working_lock = known.unwrap_or_else(|| working_copy_lock(session));
            let protection = state.protection.update(id, |protection| {
                protection.working_lock = Some(working_lock.clone());
                protection.clone()
            });
            let status = protection.status(working_lock.as_ref(), session.saves_are_rewritten());
            Ok((status, None))
        })?;
    Ok(status)
}

/// how the working copy is locked, if it is.
fn working_copy_lock(session: &EditSession) -> Option<FileLock> {
    let bytes = fs::read(session.working_copy()).ok()?;
    inspect_lock(&bytes).ok().flatten()
}

/// unlocks a locked document so it can be changed. the working copy is
/// replaced by a decrypted one in the session's folder, deleted when the
/// document closes. saving locks it again the way it was locked, unless
/// the person changes that.
#[tauri::command(async)]
pub fn unlock_document(
    state: State<AppState>,
    id: u64,
    password: String,
) -> Result<DocumentState, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let ((lock, used), document) =
        state
            .documents
            .with_session(worker, DocumentId(id), |session, show| {
                let bytes = fs::read(session.working_copy())
                    .map_err(|error| CommandError::failed(error.to_string()))?;
                let unlocked = unlock_for_editing(&bytes, &password)
                    .map_err(|error| CommandError::failed(error.to_string()))?;
                let shown = session.start_over_unlocked(&unlocked.bytes, show)?;
                Ok(((unlocked.lock, unlocked.used), Some(shown)))
            })?;
    let keep = kept_lock(&lock, used, &password);
    state.protection.update(id, |protection| {
        protection.unlocked_from = Some((lock, used));
        protection.lock = Some(keep.clone());
        protection.working_lock = Some(None);
    });
    set_writer(&state, id, keep, false)?;
    Ok(document)
}

/// what save does after unlocking, until the person changes it: the same
/// lock again, when Letters knows enough to make it.
fn kept_lock(lock: &FileLock, used: PasswordUsed, password: &str) -> LockOnSave {
    let restricted = lock.permissions != redact::PermissionOptions::default();
    match used {
        PasswordUsed::Open => LockOnSave::Lock(LockOptions {
            open_password: password.to_owned(),
            owner_password: String::new(),
            permissions: lock.permissions.clone(),
        }),
        // the open password is not known, so the person has to choose
        PasswordUsed::Owner if lock.needs_open_password => LockOnSave::Undecided,
        PasswordUsed::Owner | PasswordUsed::None if restricted => LockOnSave::Lock(LockOptions {
            open_password: String::new(),
            owner_password: if used == PasswordUsed::Owner {
                password.to_owned()
            } else {
                String::new()
            },
            permissions: lock.permissions.clone(),
        }),
        PasswordUsed::Owner | PasswordUsed::None => LockOnSave::Unlocked,
    }
}

/// changes what save does about a password: a new open password, new
/// restrictions, or none at all. what the change leaves out is kept. the
/// document counts as changed, so Save is offered.
#[tauri::command(async)]
pub fn change_lock_on_save(
    state: State<AppState>,
    id: u64,
    change: LockChange,
) -> Result<DocumentState, CommandError> {
    if status_of(&state, id)?.locked {
        return Err(CommandError::failed(
            "This PDF is still locked. Unlock it first, then change its password.",
        ));
    }
    let plan = state.protection.update(id, |protection| {
        let plan = merge_lock(protection.lock.as_ref(), &change);
        protection.lock = Some(plan.clone());
        plan
    });
    set_writer(&state, id, plan, false)?;
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let ((), document) = state
        .documents
        .with_session(worker, DocumentId(id), |session, _| {
            session.mark_unsaved();
            Ok(((), None))
        })?;
    Ok(document)
}

// ---- what the rest of the shell asks ----

/// the window opened this document with a password.
pub fn note_opened_with_password(state: &AppState, id: DocumentId) {
    state
        .protection
        .update(id.0, |protection| protection.opened_with_password = true);
}

/// true when no picture of the document may be kept for Recents; any
/// picture already there is deleted.
pub fn hides_thumbnail(state: &AppState, id: u64, thumbnail_dir: &Path, path: &Path) -> bool {
    if !state.protection.get(id).hides_thumbnail() {
        return false;
    }
    let _ = fs::remove_file(thumbnail_dir.join(format!("{}.png", thumbnail_name(path))));
    true
}

/// a document is closing: its review copies are closed and deleted, and
/// any password typed for it is forgotten.
pub fn forget_document(state: &AppState, id: DocumentId) {
    for preview in state.protection.forget(id.0) {
        close_preview(state, preview);
    }
}
