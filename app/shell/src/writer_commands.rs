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
use letters_shell_core::writer::clipboard::{Copied, PastedHtml, PastedPicture};
use letters_shell_core::writer::engine::{apply_preset, ruler_for, PresetDetails};
use letters_shell_core::writer::export::ExportError;
use letters_shell_core::writer::objects::{Compatibility, ObjectAnswer, ObjectOp};
use letters_shell_core::writer::sessions::{
    ExportedPdf, OpenedWriter, SavedWriter, WriterSessions,
};
use letters_shell_core::writer::settings::{WriterSettings, WriterSettingsStore};
use letters_shell_core::writer::thumbnail::save_generic_thumbnail;
use letters_shell_core::writer::{format_capabilities, FormatCapability};

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
    let opened = writer
        .sessions
        .open(window.label(), &path, recovered)
        .map_err(failed)?;
    remember(&state, &path);
    Ok(opened)
}

/// an open document's file moved: Save follows it (the ui hears the
/// "document-moved" event and calls this).
#[tauri::command(async)]
pub fn writer_follow_move(
    writer: State<WriterState>,
    id: u64,
    to: String,
) -> Result<SavedWriter, CommandError> {
    writer
        .sessions
        .follow_move(id, Path::new(&to))
        .map_err(failed)
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
    replace_changed: Option<bool>,
) -> Result<SavedWriter, CommandError> {
    let replace_changed = replace_changed.unwrap_or(false);
    // Save over its own file: a change made elsewhere is asked about first
    if path.is_none() && !replace_changed {
        if let Some(message) = writer.sessions.changed_on_disk(id).map_err(failed)? {
            return Err(CommandError::changed_on_disk(message));
        }
    }
    let saved = writer
        .sessions
        .save(id, model, path.as_deref().map(Path::new), replace_changed)
        .map_err(failed)?;
    remember(&state, Path::new(&saved.path));
    Ok(saved)
}

#[tauri::command(async)]
pub fn writer_odt_losses(
    writer: State<WriterState>,
    id: u64,
    model: Value,
) -> Result<Option<String>, CommandError> {
    writer.sessions.odt_losses(id, model).map_err(failed)
}

#[tauri::command(async)]
pub fn writer_autosave(
    writer: State<WriterState>,
    id: u64,
    model: Value,
) -> Result<(), CommandError> {
    writer.sessions.autosave(id, model).map_err(failed)
}

#[tauri::command(async)]
pub fn writer_discard_recovery(writer: State<WriterState>, id: u64) -> Result<(), CommandError> {
    writer.sessions.discard_recovery(id).map_err(failed)
}

#[tauri::command(async)]
pub fn writer_media(
    writer: State<WriterState>,
    id: u64,
    media_id: String,
) -> Result<Option<String>, CommandError> {
    writer.sessions.media(id, &media_id).map_err(failed)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedPicture {
    media_id: String,
    url: String,
}

#[tauri::command(async)]
pub fn writer_add_picture(
    writer: State<WriterState>,
    id: u64,
    path: String,
) -> Result<AddedPicture, CommandError> {
    let (media_id, url) = writer
        .sessions
        .add_picture(id, Path::new(&path))
        .map_err(failed)?;
    Ok(AddedPicture { media_id, url })
}

/// a pasted or dropped picture, as base64 bytes.
#[tauri::command(async)]
pub fn writer_add_picture_data(
    writer: State<WriterState>,
    id: u64,
    name: String,
    data: String,
) -> Result<AddedPicture, CommandError> {
    let bytes = letters_shell_core::writer::document::decode_base64(&data)
        .ok_or_else(|| failed("The picture couldn’t be read.".into()))?;
    let (media_id, url) = writer
        .sessions
        .add_picture_bytes(id, &name, bytes)
        .map_err(failed)?;
    Ok(AddedPicture { media_id, url })
}

#[tauri::command(async)]
pub fn writer_apply_preset(
    model: Value,
    preset: String,
    details: PresetDetails,
) -> Result<Value, CommandError> {
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
pub fn writer_export_pdf(
    writer: State<WriterState>,
    id: u64,
    model: Value,
    path: String,
) -> Result<ExportAnswer, CommandError> {
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

/// text blocks, free pictures, stacking, aligning and word's fixes (see
/// shell-core/src/writer/objects.rs).
#[tauri::command(async)]
pub fn writer_objects(
    writer: State<WriterState>,
    id: u64,
    model: Value,
    op: Value,
) -> Result<ObjectAnswer, CommandError> {
    let op: ObjectOp = serde_json::from_value(op)
        .map_err(|error| failed(format!("Letters didn’t understand that command. ({error})")))?;
    writer.sessions.objects(id, model, &op).map_err(failed)
}

/// the engine's feature table and what word would show differently.
#[tauri::command(async)]
pub fn writer_compatibility(
    writer: State<WriterState>,
    id: u64,
    model: Value,
) -> Result<Compatibility, CommandError> {
    writer.sessions.compatibility(id, model).map_err(failed)
}

/// the clipboard forms of the copied blocks, a lone picture's bytes included.
#[tauri::command(async)]
pub fn writer_copy(
    writer: State<WriterState>,
    id: u64,
    model: Value,
    blocks: Value,
) -> Result<Copied, CommandError> {
    writer.sessions.copy(id, model, blocks).map_err(failed)
}

/// picture bytes (base64) from the clipboard or a dropped file.
#[tauri::command(async)]
pub fn writer_paste_picture(
    writer: State<WriterState>,
    id: u64,
    model: Value,
    data: String,
    description: Option<String>,
) -> Result<PastedPicture, CommandError> {
    let bytes = letters_shell_core::writer::document::decode_base64(&data)
        .ok_or_else(|| failed("The picture couldn’t be read.".into()))?;
    writer
        .sessions
        .paste_picture(id, model, &bytes, description)
        .map_err(failed)
}

/// html from the clipboard, with the pictures it names.
#[tauri::command(async)]
pub fn writer_paste_html(
    writer: State<WriterState>,
    id: u64,
    html: String,
) -> Result<PastedHtml, CommandError> {
    writer.sessions.paste_html(id, &html).map_err(failed)
}

/// every format Save As offers, with the engine's sentence on each, and
/// which converters this computer has (for .doc).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Formats {
    formats: Vec<FormatCapability>,
    word: bool,
    libre_office: bool,
}

#[tauri::command(async)]
pub fn writer_formats() -> Formats {
    Formats {
        formats: format_capabilities(),
        word: letters_shell_core::writer::export::word_installed(),
        libre_office: letters_shell_core::writer::export::find_libreoffice().is_some(),
    }
}

/// opens the system's own print dialog (not the web view's preview) for
/// what the window shows for printing: the print view print.ts lays out.
/// the person picks the printer there; nothing about the default printer
/// or any other setting changes. Err on systems without it, and the ui
/// then uses the web view's own printing.
///
/// it runs off the main thread (async), because the web view is reached on
/// the main thread and this command waits for the answer.
#[tauri::command(async)]
pub fn writer_system_print(window: tauri::WebviewWindow) -> Result<(), CommandError> {
    #[cfg(windows)]
    {
        let (done, answer) = std::sync::mpsc::channel();
        window
            .with_webview(move |platform| {
                show_system_print(&platform, |result| drop(done.send(result)))
            })
            .map_err(|error| failed(error.to_string()))?;
        answer
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| failed("The print dialog didn’t open.".into()))?
            .map_err(failed)
    }
    // a mac's web view ignores window.print(), so Cmd P did nothing there;
    // the web view's print operation opens the mac's print panel instead,
    // with Save as PDF in it (needs macOS 11, the app's minimum)
    #[cfg(target_os = "macos")]
    {
        window
            .print()
            .map_err(|error| failed(format!("The print dialog didn’t open. ({error})")))
    }
    // linux: the web view's own window.print() works, so the ui uses it
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = window;
        Err(failed(
            "The system print dialog isn’t available here.".into(),
        ))
    }
}

/// the answer is given before the dialog shows: the dialog is modal and
/// stays open as long as the person needs.
#[cfg(windows)]
fn show_system_print(
    platform: &tauri::webview::PlatformWebview,
    answer: impl FnOnce(Result<(), String>),
) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_16, COREWEBVIEW2_PRINT_DIALOG_KIND_SYSTEM,
    };
    use windows_core::Interface;
    // ShowPrintUI arrived with web view 2 runtime 1.0.1518; older runtimes say so
    let printing = unsafe { platform.controller().CoreWebView2() }
        .map_err(|error| error.to_string())
        .and_then(|core| {
            core.cast::<ICoreWebView2_16>().map_err(|_| {
                "This version of the web view can’t open the system print dialog.".to_string()
            })
        });
    match printing {
        Ok(printing) => {
            answer(Ok(()));
            let _ = unsafe { printing.ShowPrintUI(COREWEBVIEW2_PRINT_DIALOG_KIND_SYSTEM) };
        }
        Err(problem) => answer(Err(problem)),
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
