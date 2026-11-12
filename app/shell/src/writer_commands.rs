//! the commands of word-processor mode (.docx and .odt).
//!
//! the work is done in letters-shell-core (src/writer), where it is unit
//! tested; these are the thin doors the ui calls through. like every
//! command they are `async`, so a big document never freezes a window.

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, State, Window};

use crate::errors::CommandError;
use crate::state::AppState;
use letters_shell_core::writer::engine::{apply_preset, ruler_for, PresetDetails};
use letters_shell_core::writer::export::ExportError;
use letters_shell_core::writer::sessions::{ExportedPdf, OpenedWriter, SavedWriter, WriterSessions};
use letters_shell_core::writer::settings::{WriterSettings, WriterSettingsStore};
use letters_shell_core::writer::thumbnail::save_generic_thumbnail;

/// the writer's part of the app's memory, managed beside `AppState`.
pub struct WriterState {
    sessions: WriterSessions,
    settings: WriterSettingsStore,
}

impl WriterState {
    pub fn start(app: &AppHandle) -> WriterState {
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        let cache_dir = app.path().app_cache_dir().unwrap_or_default();
        WriterState {
            sessions: WriterSessions::new(&cache_dir),
            settings: WriterSettingsStore::new(data_dir.join("writer-settings.json")),
        }
    }
}

impl WriterState {
    pub fn sessions(&self) -> &WriterSessions {
        &self.sessions
    }

    pub fn settings(&self) -> WriterSettings {
        self.settings.load()
    }
}

/// a closed window's documents are closed with it (see lib.rs).
pub fn close_window(app: &AppHandle, window: &str) {
    if let Some(writer) = app.try_state::<WriterState>() {
        writer.sessions.close_window(window);
    }
}

fn failed(message: String) -> CommandError {
    CommandError::failed(message)
}

/// Recents shows word documents too, with a plain page as their picture.
fn remember(state: &AppState, path: &Path) {
    state.recents.lock().unwrap().add(path, 0);
    save_generic_thumbnail(&state.thumbnail_dir, path);
}

#[tauri::command(async)]
pub fn writer_open(
    window: Window,
    state: State<AppState>,
    writer: State<WriterState>,
    path: String,
    recovered: bool,
) -> Result<OpenedWriter, CommandError> {
    let path = PathBuf::from(path);
    let opened = writer.sessions.open(window.label(), &path, recovered).map_err(failed)?;
    remember(&state, &path);
    Ok(opened)
}

#[tauri::command(async)]
pub fn writer_close(writer: State<WriterState>, id: u64) {
    writer.sessions.close(id);
}

/// saves to the document's file, or to `path` (Save As; .docx or .odt by name).
#[tauri::command(async)]
pub fn writer_save(
    state: State<AppState>,
    writer: State<WriterState>,
    id: u64,
    model: Value,
    path: Option<String>,
) -> Result<SavedWriter, CommandError> {
    let saved = writer
        .sessions
        .save(id, model, path.as_deref().map(Path::new))
        .map_err(failed)?;
    remember(&state, Path::new(&saved.path));
    Ok(saved)
}

#[tauri::command(async)]
pub fn writer_odt_losses(writer: State<WriterState>, id: u64, model: Value) -> Result<Option<String>, CommandError> {
    writer.sessions.odt_losses(id, model).map_err(failed)
}

#[tauri::command(async)]
pub fn writer_autosave(writer: State<WriterState>, id: u64, model: Value) -> Result<(), CommandError> {
    writer.sessions.autosave(id, model).map_err(failed)
}

#[tauri::command(async)]
pub fn writer_discard_recovery(writer: State<WriterState>, id: u64) -> Result<(), CommandError> {
    writer.sessions.discard_recovery(id).map_err(failed)
}

#[tauri::command(async)]
pub fn writer_media(writer: State<WriterState>, id: u64, media_id: String) -> Result<Option<String>, CommandError> {
    writer.sessions.media(id, &media_id).map_err(failed)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedPicture {
    media_id: String,
    url: String,
}

#[tauri::command(async)]
pub fn writer_add_picture(writer: State<WriterState>, id: u64, path: String) -> Result<AddedPicture, CommandError> {
    let (media_id, url) = writer.sessions.add_picture(id, Path::new(&path)).map_err(failed)?;
    Ok(AddedPicture { media_id, url })
}

/// a pasted or dropped picture, as base64 bytes.
#[tauri::command(async)]
pub fn writer_add_picture_data(writer: State<WriterState>, id: u64, name: String, data: String) -> Result<AddedPicture, CommandError> {
    let bytes = letters_shell_core::writer::document::decode_base64(&data).ok_or_else(|| failed("The picture couldn’t be read.".into()))?;
    let (media_id, url) = writer.sessions.add_picture_bytes(id, &name, bytes).map_err(failed)?;
    Ok(AddedPicture { media_id, url })
}

#[tauri::command(async)]
pub fn writer_apply_preset(model: Value, preset: String, details: PresetDetails) -> Result<Value, CommandError> {
    apply_preset(model, &preset, &details).map_err(failed)
}

#[tauri::command(async)]
pub fn writer_describe_ruler(model: Value, block_index: usize) -> Result<Value, CommandError> {
    let ruler = ruler_for(model, block_index).map_err(failed)?;
    serde_json::to_value(ruler).map_err(|error| failed(error.to_string()))
}

/// the answer to Export as PDF: the pdf made, or `noConverter` when neither
/// word nor libreoffice is installed (the ui then prints its own surface).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportAnswer {
    exported: Option<ExportedPdf>,
    no_converter: bool,
}

#[tauri::command(async)]
pub fn writer_export_pdf(writer: State<WriterState>, id: u64, model: Value, path: String) -> Result<ExportAnswer, CommandError> {
    match writer.sessions.export_pdf(id, model, Path::new(&path)) {
        Ok(exported) => Ok(ExportAnswer {
            exported: Some(exported),
            no_converter: false,
        }),
        Err(ExportError::NoConverter) => Ok(ExportAnswer {
            exported: None,
            no_converter: true,
        }),
        Err(ExportError::Failed(message)) => Err(failed(message)),
    }
}

#[tauri::command(async)]
pub fn writer_settings(writer: State<WriterState>) -> WriterSettings {
    writer.settings.load()
}

#[tauri::command(async)]
pub fn writer_set_settings(writer: State<WriterState>, settings: WriterSettings) -> WriterSettings {
    writer.settings.save(&settings);
    writer.settings.load()
}
