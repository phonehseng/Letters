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
            annotate_commands::add_text_markup,
            annotate_commands::add_text_markups,
            annotate_commands::add_note,
            annotate_commands::add_reply,
            annotate_commands::add_text_box,
            annotate_commands::add_shape,
            annotate_commands::add_line,
            annotate_commands::add_polygon,
            annotate_commands::add_ink,
            annotate_commands::add_image,
            annotate_commands::update_annotation,
            annotate_commands::delete_annotation,
            annotate_commands::flatten_annotations,
            annotate_commands::list_annotations,
            annotate_commands::list_all_annotations,
            annotate_commands::flatten_explanation,
            annotate_commands::page_text,
            annotate_commands::picture_size,
            annotate_commands::annotation_author,
            annotate_commands::set_annotation_author,
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
            // word-processor mode
            writer_commands::writer_open,
            writer_commands::writer_close,
            writer_commands::writer_save,
            writer_commands::writer_odt_losses,
            writer_commands::writer_autosave,
            writer_commands::writer_discard_recovery,
            writer_commands::writer_media,
            writer_commands::writer_add_picture,
            writer_commands::writer_apply_preset,
            writer_commands::writer_describe_ruler,
            writer_commands::writer_export_pdf,
            writer_commands::writer_settings,
            writer_commands::writer_set_settings,
            writer_commands::writer_add_picture_data,
            drive_commands::drive_status,
            drive_commands::drive_sign_in,
            drive_commands::drive_sign_out,
            drive_commands::drive_upload,
            drive_commands::open_link,
            drive_commands::writer_word_available,
            drive_commands::writer_word_layout,
            drive_commands::writer_export_copy,
            drive_commands::writer_print_pdf,
            // forms and signatures (Fill & Sign)
            forms_commands::forms_overview,
            forms_commands::forms_set_value,
            forms_commands::forms_add_fields,
            forms_commands::forms_update_field,
            forms_commands::forms_remove_field,
            forms_commands::forms_detect_fields,
            forms_commands::forms_flatten,
            forms_commands::forms_place_signature,
            forms_commands::forms_signature_pictures,
            forms_commands::forms_keep_signature_picture,
            forms_commands::forms_delete_signature_picture,
            forms_commands::forms_read_picture,
            forms_commands::forms_create_certificate,
            forms_commands::forms_sign,
            forms_commands::forms_release_signature_guard,
            // redaction, hidden information and passwords (Protect)
            protect_commands::find_redaction_text,
            protect_commands::redaction_preview,
            protect_commands::redaction_dry_run,
            protect_commands::close_redaction_dry_run,
            protect_commands::apply_redactions,
            protect_commands::hidden_information,
            protect_commands::remove_hidden_information,
            protect_commands::protection_status,
            protect_commands::unlock_document,
            protect_commands::change_lock_on_save,
        ]);

    // a menu bar only on macos: on windows it would sit inside every
    // window (see menu.rs)
    #[cfg(target_os = "macos")]
    let builder = builder.menu(menu::build).on_menu_event(menu::handle_event);

    let app = builder
        .setup(|app| {
            let handle = app.handle();
            app.manage(AppState::start(handle));
            app.manage(writer_commands::WriterState::start(handle));
            app.manage(forms_commands::FormsState::start(handle));
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
            writer_commands::close_window(handle, &label);
        }
    });
}
