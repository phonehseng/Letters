//! the desktop shell.
//!
//! one process serves every window. opening a second file, or starting
//! the app a second time, adds a window to this same process, so all
//! windows share one pdf engine and one tile cache.

mod annotate_commands;
mod commands;
mod documents;
mod engine_location;
mod errors;
mod menu;
mod page_commands;
mod state;
mod tile_protocol;
mod windows;
// word-processor mode (.docx and .odt)
mod writer_commands;
mod writer_menu;
// save to google drive, word's layout, copies in other formats, printing
mod drive_commands;
// forms and signatures (Fill & Sign)
mod forms_commands;
mod forms_menu;
// redaction, hidden information and passwords (Protect)
mod protect_commands;
mod protect_menu;
// the font library (system, added, bundled and downloadable fonts)
mod fonts_commands;
// every command the interface can call, one block per feature
mod command_routes;

use tauri::{Manager, RunEvent, WindowEvent};

use letters_shell_core::startup::file_argument;

use crate::state::AppState;

pub fn run() {
    let builder = tauri::Builder::default()
        // must be the first plugin: when the app is already running it hands
        // the new file to the running copy and exits straight away
        .plugin(tauri_plugin_single_instance::init(
            |app, arguments, _cwd| {
                windows::open_window(app, file_argument(&arguments));
            },
        ))
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("tile", tile_protocol::handle)
        // every feature's commands, listed per feature in command_routes.rs
        .invoke_handler(command_routes::handler());

    // a menu bar only on macos: on windows it would sit inside every
    // window (see menu.rs)
    #[cfg(target_os = "macos")]
    let builder = builder.menu(menu::build).on_menu_event(menu::handle_event);

    let app = builder
        .setup(|app| {
            let handle = app.handle();
            manage_feature_states(app);
            // a window for each file the app was started with
            let arguments: Vec<String> = std::env::args().collect();
            windows::open_window(handle, file_argument(&arguments));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the app could not start");

    app.run(on_run_event);
}

/// what each feature keeps while the app runs. AppState comes first:
/// other features use its pdf engine and recents.
fn manage_feature_states(app: &tauri::App) {
    let handle = app.handle();
    app.manage(AppState::start(handle));
    app.manage(writer_commands::WriterState::start(handle));
    app.manage(forms_commands::FormsState::start(handle));
    // the font library starts scanning system fonts on its own thread
    app.manage(fonts_commands::FontsState::start(handle));
}

fn on_run_event(handle: &tauri::AppHandle, event: RunEvent) {
    if let RunEvent::WindowEvent {
        label,
        event: WindowEvent::Destroyed,
        ..
    } = event
    {
        close_window_parts(handle, &label);
    }
}

/// a closed window must not keep its documents (their cached tiles and
/// working copies) alive. unsaved changes were already offered for saving
/// by the window's close sheet.
fn close_window_parts(handle: &tauri::AppHandle, label: &str) {
    handle.state::<AppState>().close_documents_of_window(label);
    writer_commands::close_window(handle, label);
}