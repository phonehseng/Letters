//! every command the interface can call, one small block per feature.
//!
//! tauri takes a single invoke handler, so each feature's commands are
//! listed once in `feature!` below, which makes both the feature's handler
//! and the list of its names; `handler` sends each call to the feature
//! that has a command of that name. adding a command is one line in its
//! feature's block.

use tauri::ipc::Invoke;
use tauri::Wry;

use crate::{
    a11y_commands, ai_commands, annotate_commands, associations, commands, compare_commands,
    drive_commands, edit_commands, export_commands, fonts_commands, forms_commands, ocr_commands, ocr_images_commands,
    page_commands, protect_commands, reading_commands, search_commands, settings_commands,
    writer_commands,
};
// code and text files, kept on its own line so it never shifts the list above
use crate::code_commands;
// moving, copying and renaming documents, on its own line for the same reason
use crate::organise_commands;
// presentations (the slide editor), on its own line for the same reason
use crate::slides_commands;
// edit tab round two and make editable text, on their own line for the same reason
use crate::{edit_flow_commands, edit_page_commands, edit_shape_commands, ocr_editable_commands};
// spreadsheets, on their own line for the same reason
use crate::{sheets_commands, sheets_help_commands};

/// one feature's commands: its handler, and the names it answers to.
struct Feature {
    names: &'static [&'static str],
    handle: Box<dyn Fn(Invoke<Wry>) -> bool + Send + Sync>,
}

/// builds a `Feature` from `module::command` paths. the names come from the
/// same list as the handler, so the two can't disagree.
macro_rules! feature {
    ($($module:ident :: $command:ident),* $(,)?) => {
        Feature {
            names: &[$(stringify!($command)),*],
            handle: Box::new(tauri::generate_handler![$($module::$command),*]),
        }
    };
}

/// the one invoke handler: finds the feature that owns the command.
pub fn handler() -> impl Fn(Invoke<Wry>) -> bool + Send + Sync + 'static {
    let features = features();
    move |invoke| {
        let name = invoke.message.command().to_string();
        match features
            .iter()
            .find(|feature| feature.names.contains(&name.as_str()))
        {
            Some(feature) => (feature.handle)(invoke),
            // unknown names get tauri's own "command not found" answer
            None => false,
        }
    }
}

fn features() -> Vec<Feature> {
    vec![
        documents(),
        pages(),
        annotations(),
        windows_and_recents(),
        writer(),
        writer_drive(),
        forms(),
        protect(),
        fonts(),
        compare(),
        accessibility(),
        edit(),
        // edit tab round two: its own block so the list above stays as it was
        edit_round_two(),
        scan(),
        export(),
        ai_tools(),
        settings(),
        library_search(),
        reading_and_writing(),
        code_files(),
        organising(),
        // presentations: the slide editor and the slide show
        presentations(),
        // spreadsheets: the sheets screen and its formula help
        spreadsheets(),
        spreadsheet_help(),
    ]
}

/// opening, saving, undo and redo of a pdf.
fn documents() -> Feature {
    feature![
        commands::open_document,
        commands::close_document,
        commands::document_state,
        commands::undo,
        commands::redo,
        commands::save_document,
        commands::save_document_as,
        commands::set_viewport,
    ]
}

/// the Pages tab.
fn pages() -> Feature {
    feature![
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
    ]
}

/// the Annotate tab, the page text for selecting, the name on comments.
fn annotations() -> Feature {
    feature![
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
    ]
}

/// windows, recents, pinned folders, the outline and find.
fn windows_and_recents() -> Feature {
    feature![
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
    ]
}

/// word-processor mode.
fn writer() -> Feature {
    feature![
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
        writer_commands::writer_objects,
        writer_commands::writer_compatibility,
        writer_commands::writer_copy,
        writer_commands::writer_paste_picture,
        writer_commands::writer_paste_html,
        writer_commands::writer_formats,
        writer_commands::writer_system_print,
        writer_commands::writer_follow_move,
    ]
}

/// google drive, and the writer's layouts, copies and printing.
fn writer_drive() -> Feature {
    feature![
        drive_commands::writer_engine_layout,
        drive_commands::drive_status,
        drive_commands::drive_sign_in,
        drive_commands::drive_sign_out,
        drive_commands::drive_upload,
        drive_commands::open_link,
        drive_commands::writer_word_available,
        drive_commands::writer_word_layout,
        drive_commands::writer_export_copy,
        drive_commands::writer_print_pdf,
    ]
}

/// forms and signatures (Fill & Sign).
fn forms() -> Feature {
    feature![
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
    ]
}

/// redaction, hidden information and passwords (Protect).
fn protect() -> Feature {
    feature![
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
    ]
}

/// the font library.
fn fonts() -> Feature {
    feature![
        fonts_commands::fonts_status,
        fonts_commands::fonts_families,
        fonts_commands::fonts_search,
        fonts_commands::fonts_details,
        fonts_commands::fonts_writing_systems,
        fonts_commands::fonts_face_bytes,
        fonts_commands::fonts_preview,
        fonts_commands::fonts_add,
        fonts_commands::fonts_remove,
        fonts_commands::fonts_set_favourite,
        fonts_commands::fonts_note_used,
        fonts_commands::fonts_personal,
        fonts_commands::fonts_accept_download_notice,
        fonts_commands::fonts_download,
        fonts_commands::fonts_cancel_download,
        fonts_commands::fonts_fetch_preview,
        fonts_commands::fonts_missing,
        fonts_commands::fonts_resolve,
        fonts_commands::fonts_similar,
    ]
}

/// comparing two versions (the Compare screen).
fn compare() -> Feature {
    feature![
        compare_commands::compare_start,
        compare_commands::compare_status,
        compare_commands::compare_result,
        compare_commands::compare_cancel,
        compare_commands::compare_close,
        compare_commands::compare_pair_pictures,
        compare_commands::compare_save_report,
    ]
}

/// the accessibility checker and fix wizard.
fn accessibility() -> Feature {
    feature![
        a11y_commands::a11y_check,
        a11y_commands::a11y_fix,
        a11y_commands::a11y_heading_proposal,
        a11y_commands::a11y_reading_blocks,
        a11y_commands::a11y_region_picture,
        a11y_commands::a11y_save_report,
        a11y_commands::a11y_words,
    ]
}

/// editing page text and pictures (the Edit tab).
fn edit() -> Feature {
    feature![
        edit_commands::edit_page_objects,
        edit_commands::edit_replace_text,
        edit_commands::edit_insert_text,
        edit_commands::edit_delete_chars,
        edit_commands::edit_delete_text,
        edit_commands::edit_set_text_style,
        edit_commands::edit_add_text,
        edit_commands::edit_add_image,
        edit_commands::edit_replace_image,
        edit_commands::edit_move_image,
        edit_commands::edit_resize_image,
        edit_commands::edit_rotate_image,
        edit_commands::edit_delete_image,
        edit_commands::edit_restack_image,
        edit_commands::edit_save_image,
        edit_commands::edit_font_face,
    ]
}

/// the Edit tab's round two: text that reflows, table cells, text in the
/// page's style, shapes, picture crops, page setup, and scans made editable.
fn edit_round_two() -> Feature {
    feature![
        edit_flow_commands::edit_check,
        edit_flow_commands::edit_reflow_text,
        edit_flow_commands::edit_add_matching_text,
        edit_shape_commands::edit_move_paths,
        edit_shape_commands::edit_resize_paths,
        edit_shape_commands::edit_rotate_paths,
        edit_shape_commands::edit_style_paths,
        edit_shape_commands::edit_path_points,
        edit_shape_commands::edit_delete_paths,
        edit_shape_commands::edit_restack_paths,
        edit_shape_commands::edit_duplicate_paths,
        edit_shape_commands::edit_crop_image,
        edit_shape_commands::edit_flip_image,
        edit_shape_commands::edit_image_opacity,
        edit_page_commands::edit_page_layout,
        edit_page_commands::edit_set_margins,
        edit_page_commands::edit_resize_page,
        edit_page_commands::edit_add_page_space,
        edit_page_commands::edit_set_page_boxes,
        ocr_editable_commands::ocr_make_editable,
    ]
}

/// text recognition and new from images (the Scan tab).
fn scan() -> Feature {
    feature![
        ocr_commands::ocr_status,
        ocr_commands::ocr_scan_overview,
        ocr_commands::ocr_start,
        ocr_commands::ocr_job_status,
        ocr_commands::ocr_cancel,
        ocr_commands::ocr_discard,
        ocr_commands::ocr_apply,
        ocr_commands::ocr_remove_text,
        ocr_commands::ocr_overlay,
        ocr_commands::ocr_correct_word,
        ocr_images_commands::ocr_images_to_pdf,
        ocr_images_commands::ocr_images_status,
        ocr_images_commands::ocr_images_cancel,
        ocr_images_commands::ocr_open_new_document,
    ]
}

/// conversion and export, and opening each registered kind of file.
fn export() -> Feature {
    feature![
        export_commands::export_capabilities_list,
        export_commands::export_document,
        export_commands::export_cancel,
        export_commands::convert_office_to_pdf,
        export_commands::show_office_as_pdf,
        export_commands::open_pdf_as_document_window,
        export_commands::take_open_as_document,
        export_commands::open_pdf_file_as_document,
        associations::open_route,
    ]
}

/// the optional ai tools, and find a topic (no ai).
fn ai_tools() -> Feature {
    feature![
        ai_commands::ai_status,
        ai_commands::ai_plan,
        ai_commands::ai_run,
        ai_commands::ai_stop,
        ai_commands::ai_find_offline,
        ai_commands::ai_test_connection,
        ai_commands::ai_save_key,
        ai_commands::ai_has_key,
        ai_commands::ai_start_server,
        ai_commands::ai_stop_server,
        ai_commands::ai_server_running,
    ]
}

/// the settings window.
fn settings() -> Feature {
    feature![
        settings_commands::settings_get,
        settings_commands::settings_set,
        settings_commands::settings_clear_recovery,
        settings_commands::settings_clear_recents,
        settings_commands::open_settings_window,
        settings_commands::open_default_apps_settings,
    ]
}

/// search all documents.
fn library_search() -> Feature {
    feature![
        search_commands::library_search,
        search_commands::library_overview,
        search_commands::library_refresh,
        search_commands::library_change,
        search_commands::library_control,
    ]
}

/// reading and writing long documents.
fn reading_and_writing() -> Feature {
    feature![
        reading_commands::reading_document,
        reading_commands::reading_set_position,
        reading_commands::reading_clear_position,
        reading_commands::reading_add_bookmark,
        reading_commands::reading_rename_bookmark,
        reading_commands::reading_remove_bookmark,
        reading_commands::reading_record_page_time,
        reading_commands::reading_page_labels,
        reading_commands::reading_save_bookmarks_to_pdf,
        reading_commands::reading_resolve_jump,
        reading_commands::writing_log,
        reading_commands::writing_record_words,
        reading_commands::writing_set_daily_goal,
        reading_commands::writing_set_goals,
        reading_commands::versions_list,
        reading_commands::versions_save,
        reading_commands::versions_restore,
        reading_commands::versions_delete,
    ]
}

/// code and text files: the code editor screen, its checks and exports.
fn code_files() -> Feature {
    feature![
        code_commands::code_open,
        code_commands::code_open_untitled,
        code_commands::code_save,
        code_commands::code_autosave,
        code_commands::code_discard_recovery,
        code_commands::code_tools,
        code_commands::code_check,
        code_commands::code_cancel_check,
        code_commands::code_open_in_vscode,
        code_commands::code_reveal,
        code_commands::code_export_document,
        code_commands::code_find_in_document,
    ]
}

/// organising documents: move, copy, rename, undo, recycle, reveal, new folder.
fn organising() -> Feature {
    feature![
        organise_commands::organise_move,
        organise_commands::organise_copy,
        organise_commands::organise_rename,
        organise_commands::organise_undo,
        organise_commands::organise_recycle,
        organise_commands::organise_reveal,
        organise_commands::organise_new_folder,
    ]
}

/// presentations: the slide editor, its saves and exports, the slide show.
fn presentations() -> Feature {
    feature![
        slides_commands::slides_open,
        slides_commands::slides_close,
        slides_commands::slides_deck,
        slides_commands::slides_slide,
        slides_commands::slides_apply,
        slides_commands::slides_set_hidden,
        slides_commands::slides_duplicate,
        slides_commands::slides_font_face,
        slides_commands::slides_undo,
        slides_commands::slides_redo,
        slides_commands::slides_insert,
        slides_commands::slides_insert_picture,
        slides_commands::slides_pictures,
        slides_commands::slides_picture_without,
        slides_commands::slides_find,
        slides_commands::slides_replace,
        slides_commands::slides_save,
        slides_commands::slides_autosave,
        slides_commands::slides_discard_recovery,
        slides_commands::slides_export_pdf,
        slides_commands::slides_screens,
        slides_commands::slides_full_screen,
        slides_commands::slides_open_audience,
        slides_commands::slides_close_audience,
    ]
}

/// spreadsheets: opening, the cells on screen, edits, undo, saving.
fn spreadsheets() -> Feature {
    feature![
        sheets_commands::sheets_open,
        sheets_commands::sheets_new,
        sheets_commands::sheets_close,
        sheets_commands::sheets_info,
        sheets_commands::sheets_cells,
        sheets_commands::sheets_styles,
        sheets_commands::sheets_edit_text,
        sheets_commands::sheets_apply,
        sheets_commands::sheets_format,
        sheets_commands::sheets_resize,
        sheets_commands::sheets_hide,
        sheets_commands::sheets_paste_text,
        sheets_commands::sheets_copy,
        sheets_commands::sheets_undo,
        sheets_commands::sheets_redo,
        sheets_commands::sheets_save,
        sheets_commands::sheets_save_losses,
        sheets_commands::sheets_autosave,
        sheets_commands::sheets_discard_recovery,
        sheets_commands::sheets_stats,
        sheets_commands::sheets_find,
        sheets_commands::sheets_column_values,
        sheets_commands::sheets_edge,
        sheets_commands::sheets_region,
    ]
}

/// spreadsheets: formula help, explanations, error help, tips and shortcuts.
fn spreadsheet_help() -> Feature {
    feature![
        sheets_help_commands::sheets_complete,
        sheets_help_commands::sheets_function_info,
        sheets_help_commands::sheets_argument_hint,
        sheets_help_commands::sheets_reference_spans,
        sheets_help_commands::sheets_insert_reference,
        sheets_help_commands::sheets_expects_reference,
        sheets_help_commands::sheets_cycle_reference,
        sheets_help_commands::sheets_auto_close,
        sheets_help_commands::sheets_explain,
        sheets_help_commands::sheets_diagnose,
        sheets_help_commands::sheets_tips,
        sheets_help_commands::sheets_dismiss_tip,
        sheets_help_commands::sheets_shortcuts,
        sheets_help_commands::sheets_friendly_formats,
        sheets_help_commands::sheets_style_presets,
    ]
}
