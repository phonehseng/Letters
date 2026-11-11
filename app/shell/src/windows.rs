//! creates document windows.

use std::path::PathBuf;
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
