//! the desktop shell.
//!
//! one process serves every window. opening a second file, or starting
//! the app a second time, adds a window to this same process, so all
//! windows share one pdf engine and one tile cache.

mod commands;
mod documents;
mod engine_location;
mod errors;
mod menu;
mod page_commands;
mod state;
mod tile_protocol;
mod windows;

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
        .invoke_handler(tauri::generate_handler![
            commands::open_document,
            commands::close_document,
            commands::document_state,
            commands::undo,
            commands::redo,
            commands::save_document,
            commands::save_document_as,
            page_commands::rotate_pages,
            page_commands::move_pages,
            page_commands::delete_pages,
            page_commands::insert_blank_pages,
            page_commands::insert_pages_from,
            page_commands::crop_pages,
            page_commands::uncrop_pages,
            page_commands::add_page_numbers,
            page_commands::add_header_footer,
            page_commands::add_text_watermark,
            page_commands::add_image_watermark,
            page_commands::remove_stamps,
            page_commands::extract_pages,
            page_commands::split_document,
            page_commands::merge_documents,
            page_commands::pdf_page_count,
            commands::take_startup_file,
            commands::new_window,
            commands::recent_files,
            commands::forget_recent_file,
            commands::engine_stats,
            commands::document_outline,
            commands::search_text,
            commands::pinned_folders,
            commands::pin_folder,
            commands::unpin_folder,
        ]);

    // a menu bar only on macos: on windows it would sit inside every
    // window (see menu.rs)
    #[cfg(target_os = "macos")]
    let builder = builder.menu(menu::build).on_menu_event(menu::handle_event);

    let app = builder
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
            // a closed window must not keep its documents (their cached
            // tiles and working copies) alive. unsaved changes were
            // already offered for saving by the window's close sheet
            handle.state::<AppState>().close_documents_of_window(&label);
        }
    });
}
