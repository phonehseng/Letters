//! creates document windows.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::state::AppState;

static NEXT_WINDOW_NUMBER: AtomicU32 = AtomicU32::new(1);

/// opens a new window. with a file it shows that document, without one
/// it shows the home screen.
///
/// window creation is handed to the main thread and this returns at
/// once: on windows, building a window from inside a command handler
/// can otherwise freeze the app.
pub fn open_window(app: &AppHandle, file: Option<PathBuf>) {
    // a file already open in a window: that window comes forward instead
    // of a second copy, which could save over the first one's changes
    if let Some(file) = &file {
        if focus_window_showing(app, file, None) {
            return;
        }
    }
    let label = format!(
        "window-{}",
        NEXT_WINDOW_NUMBER.fetch_add(1, Ordering::Relaxed)
    );
    if let Some(file) = file {
        // the ui asks for this once it has loaded (see take_startup_file)
        app.state::<AppState>().set_startup_file(&label, file);
    }

    let app_for_window = app.clone();
    let result = app.run_on_main_thread(move || {
        let built = WebviewWindowBuilder::new(
            &app_for_window,
            &label,
            WebviewUrl::App("index.html".into()),
        )
        .title("Letters")
        .inner_size(1100.0, 800.0)
        .min_inner_size(480.0, 360.0)
        .build();
        if let Err(error) = built {
            eprintln!("could not open a window: {error}");
        }
    });
    if let Err(error) = result {
        eprintln!("could not reach the main thread: {error}");
    }
}

/// brings forward a window other than `except` that shows `file`; false
/// when there is none (or it has gone), so the caller opens the file itself.
pub fn focus_window_showing(app: &AppHandle, file: &Path, except: Option<&str>) -> bool {
    let showing = app
        .state::<AppState>()
        .window_files
        .lock()
        .unwrap()
        .window_showing(file, except);
    let Some(window) = showing.and_then(|label| app.get_webview_window(&label)) else {
        return false;
    };
    // on the main thread, like building a window (see open_window)
    let result = app.run_on_main_thread(move || {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    });
    result.is_ok()
}
