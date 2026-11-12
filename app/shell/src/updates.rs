//! in-app updates through tauri-plugin-updater, from github releases.
//!
//! the updater reads latest.json from the newest release (the release
//! workflow uploads it), checks the installer's signature against the
//! public key in tauri.conf.json, and only then installs it: an installer
//! that isn't signed by the release key is refused.
//!
//! nothing is installed over unsaved work. "update and restart" first
//! downloads and checks the update, then the ui closes every window through
//! its usual close path, which asks about unsaved changes; only when the
//! last window has closed is the update installed, on the way out
//! (`install_on_exit`). settings, recent files and saved versions live in
//! the app's data and cache folders, which installing doesn't touch.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use letters_shell_core::updates::{short_notes, UpdateSchedule};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::errors::CommandError;
use crate::settings_commands::SettingsState;

/// a release to check against instead of the newest one, for trying an
/// update before it is published (the address of a latest.json)
const ENDPOINT_OVERRIDE: &str = "LETTERS_UPDATE_ENDPOINT";

/// told to every window when an update is ready to install: each closes
/// the way it always does, asking about unsaved changes first
pub const CLOSE_FOR_UPDATE: &str = "letters-close-for-update";

/// the settings window, which holds no document and closes for an update
const SETTINGS_WINDOW: &str = "window-settings";

pub struct UpdateState {
    schedule: UpdateSchedule,
    /// the update the last check found
    found: Mutex<Option<Update>>,
    /// its installer, downloaded and checked, waiting for the windows to close
    downloaded: Mutex<Option<Vec<u8>>>,
    /// set once every window has been asked to close for the update
    install_on_exit: AtomicBool,
}

impl UpdateState {
    pub fn start(app: &AppHandle) -> UpdateState {
        let data_dir = app.path().app_data_dir().unwrap_or_default();
        UpdateState {
            schedule: UpdateSchedule::new(data_dir.join("update-check.txt")),
            found: Mutex::new(None),
            downloaded: Mutex::new(None),
            install_on_exit: AtomicBool::new(false),
        }
    }
}

/// what the update note shows.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNote {
    version: String,
    current: String,
    /// what changed, from the release notes, in a few lines
    notes: String,
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn sentence(error: impl std::fmt::Display) -> CommandError {
    CommandError::failed(format!(
        "Letters couldn’t check for updates. ({error}) Check that this computer is online, then try again."
    ))
}

/// asks for a newer version. `quiet` is the check on launch: at most once
/// a day, only when settings allow it, and silent when it fails (the
/// computer may simply be offline); Check for Updates… always asks and
/// says what went wrong.
#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, UpdateState>,
    settings: State<'_, SettingsState>,
    quiet: bool,
) -> Result<Option<UpdateNote>, CommandError> {
    if quiet
        && (!settings.store.get().general.check_for_updates
            || !state.schedule.take_due(now_seconds()))
    {
        return Ok(None);
    }
    let mut builder = app.updater_builder();
    if let Ok(endpoint) = std::env::var(ENDPOINT_OVERRIDE) {
        let url = endpoint.parse().map_err(sentence)?;
        builder = builder.endpoints(vec![url]).map_err(sentence)?;
    }
    let checked = match builder.build() {
        Ok(updater) => updater.check().await,
        Err(error) => Err(error),
    };
    let update = match checked {
        Ok(update) => update,
        Err(_) if quiet => return Ok(None),
        Err(error) => return Err(sentence(error)),
    };
    let note = update.as_ref().map(|update| UpdateNote {
        version: update.version.clone(),
        current: update.current_version.clone(),
        notes: short_notes(update.body.as_deref().unwrap_or("")),
    });
    *state.found.lock().unwrap() = update;
    Ok(note)
}

/// downloads the update found and checks its signature, marks it to be
/// installed when the last window closes, and asks every window to close.
#[tauri::command]
pub async fn update_prepare(
    app: AppHandle,
    state: State<'_, UpdateState>,
) -> Result<(), CommandError> {
    let update = state.found.lock().unwrap().clone().ok_or_else(|| {
        CommandError::failed("There is no update to install. Choose Check for Updates… again.")
    })?;
    // download() checks the signature before it returns the bytes
    let bytes = update.download(|_, _| {}, || {}).await.map_err(|error| {
        CommandError::failed(format!(
            "The update couldn’t be downloaded or wasn’t signed by Letters, so nothing was changed. ({error})"
        ))
    })?;
    *state.downloaded.lock().unwrap() = Some(bytes);
    state.install_on_exit.store(true, Ordering::SeqCst);
    // the settings window and other helpers hold no document: they close
    // now; document windows are asked, and close the way they always do
    for (label, window) in app.webview_windows() {
        let document_window = label.starts_with("window-") && label != SETTINGS_WINDOW;
        if !document_window {
            let _ = window.close();
        }
    }
    let _ = app.emit(CLOSE_FOR_UPDATE, ());
    Ok(())
}

/// a window kept its unsaved changes: the update waits for another time.
#[tauri::command]
pub fn update_cancel(state: State<'_, UpdateState>) {
    state.install_on_exit.store(false, Ordering::SeqCst);
}

/// called as letters exits: installs the waiting update and starts the
/// new version. windows' installer ends this process itself; on a mac the
/// app is replaced in place and started again.
pub fn install_on_exit(app: &AppHandle) {
    let Some(state) = app.try_state::<UpdateState>() else {
        return;
    };
    if !state.install_on_exit.swap(false, Ordering::SeqCst) {
        return;
    }
    let update = state.found.lock().unwrap().clone();
    let bytes = state.downloaded.lock().unwrap().take();
    if let (Some(update), Some(bytes)) = (update, bytes) {
        if let Err(error) = update.install(bytes) {
            eprintln!("the update couldn’t be installed: {error}");
            return;
        }
        app.restart();
    }
}
