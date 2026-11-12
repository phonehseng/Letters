//! the commands of the code editor screen (code and text files).
//!
//! the work is done in letters-shell-core (src/code), where it is tested;
//! these are the thin doors the ui calls through. nothing here runs the
//! person's code: compilers are only asked for syntax checks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use letters_pdf::DocumentId;
use letters_shell_core::code::checks::{run_check, CheckOutcome, CheckRequest};
use letters_shell_core::code::document_code::{code_in_office_document, code_in_pdf, FoundCode};
use letters_shell_core::code::export::files::{export_document, CodeExported};
use letters_shell_core::code::export::CodeExportFormat;
use letters_shell_core::code::open_elsewhere::{open_in_vscode, reveal_in_folder};
use letters_shell_core::code::recovery::{CodeRecovery, CodeRecoveryStore};
use letters_shell_core::code::runner::CheckGeneration;
use letters_shell_core::code::sources::{office_document, SourceKind};
use letters_shell_core::code::text_file::{read_text_file, save_text_file, OpenedText, TextFormat};
use letters_shell_core::code::tools::{find_tools, this_computers_folders, FoundTool, ToolKind};
use letters_shell_core::code::untitled::{is_untitled, write_untitled};
use letters_shell_core::disk_stamp::{changed_on_disk_message, DiskStamp};
use letters_shell_core::session::file_name;
use letters_shell_core::writer::thumbnail::save_generic_thumbnail;
use serde::Serialize;
use tauri::{AppHandle, Manager, State, Window};

use crate::errors::CommandError;
use crate::state::AppState;

fn failed(message: String) -> CommandError {
    CommandError::failed(message)
}

// finding tools asks each one its version, which takes a moment, so the
// answer is kept for the app's run (Check Again in the ui refreshes it)
static FOUND_TOOLS: OnceLock<Mutex<Option<Vec<FoundTool>>>> = OnceLock::new();

// one "newest check" ticket per window, so typing in one window never
// cancels another window's check
static CHECKS_BY_WINDOW: OnceLock<Mutex<HashMap<String, CheckGeneration>>> = OnceLock::new();

fn found_tools(refresh: bool) -> Vec<FoundTool> {
    let cache = FOUND_TOOLS.get_or_init(|| Mutex::new(None));
    let mut cached = cache.lock().unwrap();
    if refresh || cached.is_none() {
        *cached = Some(find_tools(&this_computers_folders()));
    }
    cached.clone().unwrap_or_default()
}

fn generation_for(window: &str) -> CheckGeneration {
    let checks = CHECKS_BY_WINDOW.get_or_init(|| Mutex::new(HashMap::new()));
    checks
        .lock()
        .unwrap()
        .entry(window.to_owned())
        .or_default()
        .clone()
}

fn recovery_store(app: &AppHandle) -> CodeRecoveryStore {
    let cache_dir = app.path().app_cache_dir().unwrap_or_default();
    CodeRecoveryStore::new(cache_dir.join("code recovery"))
}

/// Recents lists code files too, with the plain page picture the writer uses.
fn remember(state: &AppState, path: &Path) {
    state.recents.lock().unwrap().add(path, 0);
    save_generic_thumbnail(&state.thumbnail_dir, path);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedCode {
    #[serde(flatten)]
    pub file: OpenedText,
    /// unsaved text a run that ended without saving left behind
    pub recovery: Option<CodeRecovery>,
    /// a block from a document with no file yet (see shell-core code/untitled.rs)
    pub untitled: bool,
}

#[tauri::command(async)]
pub fn code_open(
    app: AppHandle,
    state: State<AppState>,
    path: String,
) -> Result<OpenedCode, CommandError> {
    let path = PathBuf::from(path);
    let file = read_text_file(&path).map_err(failed)?;
    let untitled = is_untitled(&std::env::temp_dir(), &path);
    if untitled {
        return Ok(OpenedCode {
            file,
            recovery: None,
            untitled,
        });
    }
    let recovery = recovery_store(&app)
        .find(&path)
        .filter(|copy| copy.text != file.text);
    remember(&state, &path);
    Ok(OpenedCode {
        file,
        recovery,
        untitled,
    })
}

/// "Open Block in Code Editor": a new window showing `text`, untitled.
#[tauri::command(async)]
pub fn code_open_untitled(
    app: AppHandle,
    text: String,
    extension: String,
) -> Result<(), CommandError> {
    let path = write_untitled(&std::env::temp_dir(), &text, &extension).map_err(failed)?;
    crate::windows::open_window(&app, Some(path));
    Ok(())
}

#[tauri::command(async)]
pub fn code_save(
    app: AppHandle,
    state: State<AppState>,
    path: String,
    text: String,
    format: TextFormat,
    expected: Option<DiskStamp>,
) -> Result<Option<DiskStamp>, CommandError> {
    let path = PathBuf::from(path);
    // `expected` is the file as this window last read or wrote it: a change
    // made since by another window or program isn't written over
    if expected.is_some_and(|stamp| stamp.changed_since(&path)) {
        return Err(failed(changed_on_disk_message(&file_name(&path))));
    }
    save_text_file(&path, &text, &format).map_err(failed)?;
    recovery_store(&app).discard(&path);
    remember(&state, &path);
    Ok(DiskStamp::of(&path))
}

#[tauri::command(async)]
pub fn code_autosave(app: AppHandle, path: String, text: String) -> Result<(), CommandError> {
    recovery_store(&app)
        .write(Path::new(&path), &text)
        .map_err(failed)
}

#[tauri::command(async)]
pub fn code_discard_recovery(app: AppHandle, path: String) {
    recovery_store(&app).discard(Path::new(&path));
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeTools {
    pub tools: Vec<FoundTool>,
    pub vscode: bool,
}

#[tauri::command(async)]
pub fn code_tools(refresh: bool) -> CodeTools {
    let tools = found_tools(refresh);
    let vscode = tools.iter().any(|tool| tool.kind == ToolKind::VsCode);
    CodeTools { tools, vscode }
}

/// a syntax check by an installed compiler. a newer check from the same
/// window stops this one, so only the latest text is ever checked.
#[tauri::command(async)]
pub fn code_check(window: Window, request: CheckRequest) -> Result<CheckOutcome, CommandError> {
    let generation = generation_for(window.label());
    let ticket = generation.start_new();
    let tools = found_tools(false);
    run_check(&request, &tools, &|| generation.is_current(ticket)).map_err(failed)
}

#[tauri::command(async)]
pub fn code_cancel_check(window: Window) {
    generation_for(window.label()).cancel_all();
}

#[tauri::command(async)]
pub fn code_open_in_vscode(path: String, line: u32, column: u32) -> Result<(), CommandError> {
    let tools = found_tools(false);
    let Some(vscode) = tools.iter().find(|tool| tool.kind == ToolKind::VsCode) else {
        return Err(failed(
            "Visual Studio Code isn’t installed on this computer.".into(),
        ));
    };
    open_in_vscode(&vscode.path, Path::new(&path), line, column).map_err(failed)
}

#[tauri::command(async)]
pub fn code_reveal(path: String) -> Result<(), CommandError> {
    reveal_in_folder(Path::new(&path)).map_err(failed)
}

/// "Export as Code File…": the saved document written as `format` to `destination`.
#[tauri::command(async)]
pub fn code_export_document(
    source: String,
    kind: SourceKind,
    format: CodeExportFormat,
    destination: String,
) -> Result<CodeExported, CommandError> {
    let document = office_document(Path::new(&source), kind).map_err(failed)?;
    export_document(&document, format, Path::new(&destination)).map_err(failed)
}

/// "Check Code in This Document": the code a document holds. a pdf open in
/// this window is read as it is now (with unsaved edits) through its id.
#[tauri::command(async)]
pub fn code_find_in_document(
    state: State<AppState>,
    source: String,
    kind: SourceKind,
    pdf_id: Option<u64>,
) -> Result<Vec<FoundCode>, CommandError> {
    match (kind, pdf_id) {
        (SourceKind::Pdf, Some(id)) => {
            state.read_document(DocumentId(id), |document| Ok(code_in_pdf(document)))
        }
        _ => {
            let document = office_document(Path::new(&source), kind).map_err(failed)?;
            Ok(code_in_office_document(&document))
        }
    }
}

/// the macos menu bar's Code menu. like every menu item each one sends its
/// command's name to the window in front (see menu.rs); the code screen,
/// the viewer and the writer decide what it does there.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub mod menu {
    use tauri::menu::{MenuItemBuilder, Submenu, SubmenuBuilder};
    use tauri::{AppHandle, Wry};

    /// (id, label, shortcut). the editor's own keys (Command F, G, /,
    /// Option Z, F8) work inside the editor, so only the two commands of
    /// the viewer and the writer have a menu shortcut
    const ITEMS: [(&str, &str, Option<&str>); 10] = [
        (
            "export-code-file",
            "Export as Code File…",
            Some("CmdOrCtrl+Alt+X"),
        ),
        (
            "check-document-code",
            "Check Code in This Document",
            Some("CmdOrCtrl+Alt+K"),
        ),
        ("code-go-to-line", "Go to Line…", None),
        ("code-toggle-comment", "Comment or Uncomment Lines", None),
        ("code-toggle-wrap", "Wrap Lines", None),
        ("code-toggle-problems", "Show Problems", None),
        ("code-next-problem", "Next Problem", None),
        ("code-check-now", "Check Now", None),
        ("code-open-in-vscode", "Open in VS Code", None),
        ("code-reveal", "Show in Finder", None),
    ];

    pub fn code_menu(app: &AppHandle) -> tauri::Result<Submenu<Wry>> {
        let mut menu = SubmenuBuilder::new(app, "Code");
        for (index, (id, label, shortcut)) in ITEMS.iter().enumerate() {
            // the document commands, then the editor's, then other apps
            if index == 2 || index == 8 {
                menu = menu.separator();
            }
            let item = MenuItemBuilder::with_id(*id, *label);
            let item = match shortcut {
                Some(keys) => item.accelerator(*keys).build(app)?,
                None => item.build(app)?,
            };
            menu = menu.item(&item);
        }
        menu.build()
    }

    pub fn is_code_command(id: &str) -> bool {
        ITEMS.iter().any(|(item, _, _)| *item == id)
    }
}
