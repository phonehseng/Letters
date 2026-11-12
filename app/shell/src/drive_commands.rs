//! save to google drive, and the writer's other ways out: Word's layout,
//! export a copy in another format, and print-ready pdfs.
//!
//! the work is in letters-shell-core (src/drive, src/writer). nothing is
//! uploaded unless the ui asks for exactly that file after the person
//! confirmed it.

use std::path::{Path, PathBuf};
use std::time::Duration;

use letters_shell_core::drive::local::drive_folders;
use letters_shell_core::drive::oauth::{Client, Endpoints};
use letters_shell_core::drive::tokens::{SystemStore, TokenStore};
use letters_shell_core::drive::upload::{UploadRequest, Uploaded};
use letters_shell_core::drive::{content_type, Drive};
use letters_shell_core::writer::export::word_installed;
use letters_shell_core::writer::FileKind;
use letters_shell_core::writer::layout::WordLayout;
use serde::Serialize;
use serde_json::Value;
use tauri::State;

use crate::errors::CommandError;
use crate::writer_commands::WriterState;

fn failed(message: String) -> CommandError {
    CommandError::failed(message)
}

/// the credential store entry Letters keeps its google sign-in under.
const STORE: &str = "Letters Google Drive";

fn drive_client(writer: &WriterState) -> Client {
    let settings = writer.settings();
    Client { client_id: settings.drive_client_id, client_secret: settings.drive_client_secret }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveStatus {
    /// Google Drive for desktop's "My Drive" folders on this computer
    folders: Vec<String>,
    /// a client id is set, so direct upload can be offered
    can_upload: bool,
    signed_in: bool,
}

#[tauri::command(async)]
pub fn drive_status(writer: State<WriterState>) -> DriveStatus {
    let store = SystemStore::new(STORE);
    let client = drive_client(&writer);
    let drive = Drive { endpoints: Endpoints::google(), client: client.clone(), store: &store };
    DriveStatus {
        folders: drive_folders().iter().map(|path| path.to_string_lossy().into_owned()).collect(),
        can_upload: !client.client_id.trim().is_empty(),
        signed_in: drive.signed_in(),
    }
}

/// opens google's sign-in in the system browser and waits for it.
#[tauri::command(async)]
pub fn drive_sign_in(writer: State<WriterState>) -> Result<(), CommandError> {
    let store = SystemStore::new(STORE);
    let drive = Drive { endpoints: Endpoints::google(), client: drive_client(&writer), store: &store };
    drive.sign_in(open_in_browser, Duration::from_secs(300)).map_err(failed)
}

#[tauri::command(async)]
pub fn drive_sign_out() {
    SystemStore::new(STORE).clear();
}

/// uploads the document as .docx, .pdf or a google doc.
#[tauri::command(async)]
pub fn drive_upload(
    writer: State<WriterState>,
    id: u64,
    model: Value,
    name: String,
    format: String,
    folder: Option<String>,
) -> Result<Uploaded, CommandError> {
    let (bytes, file_name, as_google_doc) = match format.as_str() {
        "docx" | "googleDoc" => (writer.sessions().bytes_as(id, model, FileKind::Docx).map_err(failed)?, format!("{name}.docx"), format == "googleDoc"),
        "pdf" => {
            let pdf = std::env::temp_dir().join("letters-writer-export").join(format!("upload-{id}.pdf"));
            writer.sessions().export_pdf(id, model, &pdf).map_err(|error| failed(format!("{error:?}")))?;
            let bytes = std::fs::read(&pdf).map_err(|error| failed(error.to_string()))?;
            let _ = std::fs::remove_file(&pdf);
            (bytes, format!("{name}.pdf"), false)
        }
        other => return Err(failed(format!("Letters can’t upload .{other}."))),
    };
    let store = SystemStore::new(STORE);
    let drive = Drive { endpoints: Endpoints::google(), client: drive_client(&writer), store: &store };
    let request = UploadRequest {
        name: if as_google_doc { name.clone() } else { file_name.clone() },
        content_type: content_type(&file_name).into(),
        as_google_doc,
        folder_id: None,
    };
    drive.upload(request, folder.as_deref(), &bytes).map_err(failed)
}

fn open_in_browser(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    let result = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();
    result.map(drop).map_err(|error| format!("The browser couldn’t be opened ({error})."))
}

#[tauri::command(async)]
pub fn open_link(url: String) -> Result<(), CommandError> {
    if !url.starts_with("https://") {
        return Err(failed("Only web addresses can be opened.".into()));
    }
    open_in_browser(&url).map_err(failed)
}

// --- the writer's layout, copies and printing

#[tauri::command(async)]
pub fn writer_word_available() -> bool {
    word_installed()
}

#[tauri::command(async)]
pub fn writer_word_layout(writer: State<WriterState>, id: u64, model: Value) -> Result<WordLayout, CommandError> {
    writer.sessions().word_layout(id, model).map_err(failed)
}

/// writes a copy as .docx, .odt, .html or .txt; the document keeps its own file.
#[tauri::command(async)]
pub fn writer_export_copy(writer: State<WriterState>, id: u64, model: Value, path: String) -> Result<String, CommandError> {
    writer.sessions().export_copy(id, model, Path::new(&path)).map_err(failed)?;
    Ok(path)
}

/// a print-ready pdf of the document through word or libreoffice, in the
/// temp folder, for printing. Err with kind "failed" when neither exists.
#[tauri::command(async)]
pub fn writer_print_pdf(writer: State<WriterState>, id: u64, model: Value) -> Result<String, CommandError> {
    let pdf: PathBuf = std::env::temp_dir().join("letters-writer-export").join(format!("print-{id}.pdf"));
    writer
        .sessions()
        .export_pdf(id, model, &pdf)
        .map(|_| pdf.to_string_lossy().into_owned())
        .map_err(|_| failed("Word and LibreOffice aren’t installed, so Letters prints its own pages.".into()))
}
