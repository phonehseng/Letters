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
    a11y_commands, annotate_commands, commands, compare_commands, drive_commands, edit_commands,
    fonts_commands, forms_commands, ocr_commands, ocr_images_commands, page_commands,
    protect_commands, writer_commands,
};

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
        scan(),
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
    ]
}

/// google drive, and the writer's layouts, copies and printing.
fn writer_drive() -> Feature {
    feature![
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







