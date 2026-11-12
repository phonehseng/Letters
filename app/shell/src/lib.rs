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
// comparing two versions of a document (the Compare screen)
mod compare_commands;
// the accessibility checker and fix wizard (the Accessibility tab)
mod a11y_commands;
// editing page text and pictures (the Edit tab), text recognition and
// new from images (the Scan tab), and their macos menu items
mod content_menu;
mod edit_commands;
mod ocr_commands;
mod ocr_images_commands;
// edit tab, round two: text that reflows, shapes, picture crops, page
// setup, and scans made editable
mod edit_answers;
mod edit_flow_commands;
mod edit_page_commands;
mod edit_shape_commands;
mod ocr_editable_commands;
// conversion and export, the optional ai tools, the settings window, and
// opening every registered kind of file (several at once included)
mod ai_commands;
mod associations;
// every command the interface can call, one block per feature
mod command_routes;
mod conversion_menu;
mod export_commands;
mod settings_commands;
// search all documents, and reading and writing long documents
mod reading_commands;
mod search_commands;
// moving, copying, renaming and recycling documents (Recents and the library)
mod organise_commands;
// code and text files (the code editor screen)
mod code_commands;
// presentations: the slide editor and the slide show (.pptx and .odp)
mod slides_commands;
// spreadsheets (the sheets screen) and their formula help
mod sheets_commands;
mod sheets_help_commands;
// forms round two: form logic and Prepare Form (the Fill & Sign tab)
// forms round two: form logic, Prepare Form and Form Data (the Fill & Sign tab)
mod form_building_commands;
mod form_data_commands;
mod form_logic_commands;
// forms round two: signing options, Verify and the audit trail
mod signing_commands;

use tauri::{Manager, RunEvent, WindowEvent};

use crate::state::AppState;

pub fn run() {
    let builder = tauri::Builder::default()
        // must be the first plugin: when the app is already running it hands
        // the new file to the running copy and exits straight away
        .plugin(tauri_plugin_single_instance::init(
            |app, arguments, _cwd| {
                associations::open_files_from_arguments(app, &arguments);
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
            associations::open_files_from_arguments(handle, &arguments);
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
    // edit tab: the font library's fonts lent to edits that add letters
    app.manage(edit_flow_commands::EditFlowState::start(handle));
    // compare: the comparisons open in each window
    app.manage(compare_commands::CompareState::start(handle));
    // export, ai tools and settings
    app.manage(export_commands::ExportState::default());
    app.manage(ai_commands::AiState::default());
    app.manage(settings_commands::SettingsState::start(handle));
    // search all documents: the index and its background job
    app.manage(search_commands::SearchState::start(handle));
    app.manage(reading_commands::ReadingState::start(handle));
    // presentations: the decks open in each window
    app.manage(slides_commands::SlidesState::start(handle));
    // spreadsheets: the open workbooks and dismissed tips
    app.manage(sheets_commands::SheetsState::start(handle));
}

fn on_run_event(handle: &tauri::AppHandle, event: RunEvent) {
    if let RunEvent::Exit = &event {
        // ai tools: a model server Letters started stops with the app
        handle.state::<ai_commands::AiState>().stop_server();
        // search: the index writes a last checkpoint before the app ends
        handle.state::<search_commands::SearchState>().stop();
    }
    // a mac hands double clicked files over as an event, not arguments
    #[cfg(target_os = "macos")]
    if let RunEvent::Opened { urls } = &event {
        associations::open_urls(handle, urls);
    }
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
    // compare: the window's comparisons close with it
    compare_commands::close_window(handle, label);
    // presentations: the window's decks and their slide pictures close with it
    slides_commands::close_window(handle, label);
    // spreadsheets: the window's workbooks close with it
    sheets_commands::close_window(handle, label);
}