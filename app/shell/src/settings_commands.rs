//! the Settings window: its own window, its stored choices, and the few
//! actions it offers (clear recovery copies, clear Recents, open the
//! system's Default Apps page).
//!
//! the choices live in shell-core/src/settings_store; settings that had a
//! home before (the name on comments, the writer's typing switches, the
//! Google Drive client id) are read and written through their own
//! commands, so nothing is stored twice.

use std::path::PathBuf;

use letters_shell_core::associations::default_app_explanation;
use letters_shell_core::settings_store::{AppSettings, SettingsStore};
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::ai_commands::AiState;
use crate::errors::CommandError;
use crate::state::AppState;

/// the label of the one Settings window. it matches "window-*" in the
/// capabilities, so it may use the same commands as any window.
pub const SETTINGS_WINDOW: &str = "window-settings";

pub struct SettingsState {
    pub store: SettingsStore,
    recovery_folder: PathBuf,
}

impl SettingsState {
    pub fn start(app: &AppHandle) -> SettingsState {
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        let cache_dir = app.path().app_cache_dir().unwrap_or_default();
        SettingsState {
            store: SettingsStore::load(data_dir.join("app-settings.json")),
            // the writer keeps its recovery copies here (writer/sessions.rs)
            recovery_folder: cache_dir.join("writer recovery"),
        }
    }
}

/// what the window shows besides the stored choices.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub settings: AppSettings,
    pub recovery_folder: String,
    pub recovery_copies: usize,
    pub default_apps_explanation: &'static str,
    /// "windows" or "macos"
    pub platform: &'static str,
}

fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "windows"
    }
}

fn recovery_copies(folder: &PathBuf) -> usize {
    std::fs::read_dir(folder)
        .map(|entries| entries.flatten().count())
        .unwrap_or(0)
}

#[tauri::command(async)]
pub fn settings_get(settings: State<SettingsState>) -> SettingsView {
    SettingsView {
        settings: settings.store.get(),
        recovery_folder: settings.recovery_folder.to_string_lossy().into_owned(),
        recovery_copies: recovery_copies(&settings.recovery_folder),
        default_apps_explanation: default_app_explanation(cfg!(target_os = "macos")),
        platform: platform(),
    }
}

#[tauri::command(async)]
pub fn settings_set(
    settings: State<SettingsState>,
    ai: State<AiState>,
    value: AppSettings,
) -> Result<AppSettings, CommandError> {
    let before = settings.store.get();
    let saved = settings.store.set(value).map_err(CommandError::failed)?;
    if before.ai != saved.ai {
        ai.settings_changed();
    }
    Ok(saved)
}

/// deletes the writer's recovery copies. returns how many were deleted.
#[tauri::command(async)]
pub fn settings_clear_recovery(settings: State<SettingsState>) -> usize {
    let mut deleted = 0;
    if let Ok(entries) = std::fs::read_dir(&settings.recovery_folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            let removed = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };
            if removed.is_ok() {
                deleted += 1;
            }
        }
    }
    deleted
}

/// empties Recents. the files themselves are not touched.
#[tauri::command(async)]
pub fn settings_clear_recents(state: State<AppState>) {
    let mut recents = state.recents.lock().unwrap();
    let paths: Vec<String> = recents
        .list()
        .iter()
        .map(|file| file.path.clone())
        .collect();
    for path in paths {
        recents.remove(&path);
    }
}

/// opens the Settings window, or brings it forward when it is open.
#[tauri::command(async)]
pub fn open_settings_window(app: AppHandle) {
    open_settings(&app);
}

pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = window.unminimize();
        let _ = window.set_focus();
        return;
    }
    let handle = app.clone();
    // windows are made on the main thread, as in windows.rs
    let _ = app.run_on_main_thread(move || {
        let built = WebviewWindowBuilder::new(
            &handle,
            SETTINGS_WINDOW,
            WebviewUrl::App("index.html".into()),
        )
        .title("Settings")
        .inner_size(720.0, 620.0)
        .min_inner_size(480.0, 420.0)
        .build();
        if let Err(error) = built {
            eprintln!("could not open the settings window: {error}");
        }
    });
}

/// opens the system's page for choosing default apps. windows lets only
/// the person choose; Letters never sets a default itself.
#[tauri::command(async)]
pub fn open_default_apps_settings() -> Result<(), CommandError> {
    #[cfg(windows)]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", "ms-settings:defaultapps"])
            .spawn()
            .map(drop)
            .map_err(|error| {
                CommandError::failed(format!("Settings couldn’t be opened ({error})."))
            })
    }
    #[cfg(not(windows))]
    {
        Err(CommandError::failed(
            "On a Mac, use the Finder’s Get Info window, as described above.",
        ))
    }
}
