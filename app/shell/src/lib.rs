//! the desktop shell.
//!
//! one process serves every window. opening a second file, or starting
//! the app a second time, adds a window to this same process, so all
//! windows share one pdf engine and one tile cache.

mod commands;
mod engine_location;
mod state;
mod tile_protocol;
mod windows;

use tauri::{Manager, RunEvent, WindowEvent};

use letters_shell_core::startup::file_argument;

use crate::state::AppState;

pub fn run() {
    let app = tauri::Builder::default()
        // must be the first plugin: when the app is already running it hands
        // the new file to the running copy and exits straight away
        .plugin(tauri_plugin_single_instance::init(
            |app, arguments, _cwd| {
                windows::open_window(app, file_argument(&arguments));
            },
        ))
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("tile", tile_protocol::handle)
        .invoke_handler(tauri::generate_handler![
            commands::open_document,
            commands::close_document,
            commands::take_startup_file,
            commands::new_window,
            commands::recent_files,
            commands::forget_recent_file,
            commands::engine_stats,
        ])
        .setup(|app| {
            let handle = app.handle();
            app.manage(AppState::start(handle));
            let arguments: Vec<String> = std::env::args().collect();
            windows::open_window(handle, file_argument(&arguments));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the app could not start");

    app.run(|handle, event| {
        if let RunEvent::WindowEvent {
            label,
            event: WindowEvent::Destroyed,
            ..
        } = event
        {
            // a closed window must not keep its documents (and their
            // cached tiles) alive
            handle.state::<AppState>().close_documents_of_window(&label);
        }
    });
}
