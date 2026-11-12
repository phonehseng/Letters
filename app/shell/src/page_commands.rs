//! the page tools, one command each.
//!
//! every command that changes the open document is one call through
//! `state.edit`: the tool from core/pages changes the working copy, and
//! the shell takes care of undo, showing the result and the new state.
//! `rotate_pages` is the example to copy.
//!
//! tools that make other files (extract, split, merge) change nothing in
//! the open document: they read it with `state.read_document` and write
//! their files beside it (see letters_shell_core::outputs for the rules
//! that keep existing files safe). page numbers are counted from 0.

use std::fs;
use std::path::Path;

use letters_pages::{
    self as page_tools, CropParams, DeleteParams, ExtractParams, HeaderFooterParams,
    ImageWatermarkParams, InsertBlankParams, InsertFromParams, MergeParams, MovePagesParams,
    PageNumberParams, RemoveStampsParams, RotateParams, SplitParams, StampKind,
    TextWatermarkParams, UncropParams,
};
use letters_pdf::DocumentId;
use letters_shell_core::outputs::{load_other_pdf, write_chosen_file, write_parts, NewPart};
use letters_shell_core::session::{file_name, with_pdf_extension};
use serde::Serialize;
use tauri::State;

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

/// turns pages by quarter turns: 1 is a quarter turn clockwise, -1 a
/// quarter turn anticlockwise.
///
/// the reference example every editing command follows: no checks here,
/// one call to a pure tool through `state.edit`, and the new state back.
#[tauri::command(async)]
pub fn rotate_pages(
    state: State<AppState>,
    id: u64,
    pages: Vec<u32>,
    quarter_turns: i32,
) -> Result<DocumentState, CommandError> {
    let params = RotateParams {
        pages,
        quarter_turns,
    };
    state.edit(DocumentId(id), |document| {
        page_tools::rotate(document, &params)
    })
}

/// moves pages, as a block, to just before the page now at `before_index`
/// (the page count puts them at the end). what dragging thumbnails does.
#[tauri::command(async)]
pub fn move_pages(
    state: State<AppState>,
    id: u64,
    pages: Vec<u32>,
    before_index: u32,
) -> Result<DocumentState, CommandError> {
    let params = MovePagesParams {
        pages,
        before_index,
    };
    state.edit(DocumentId(id), |document| {
        page_tools::move_pages(document, &params)
    })
}

#[tauri::command(async)]
pub fn delete_pages(
    state: State<AppState>,
    id: u64,
    pages: Vec<u32>,
) -> Result<DocumentState, CommandError> {
    let params = DeleteParams { pages };
    state.edit(DocumentId(id), |document| {
        page_tools::delete(document, &params)
    })
}

#[tauri::command(async)]
pub fn insert_blank_pages(
    state: State<AppState>,
    id: u64,
    params: InsertBlankParams,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        page_tools::insert_blank(document, &params)
    })
}

/// copies pages of another pdf in before the page now at `at_index`.
/// with no `other_pages` every page of the other pdf is copied.
#[tauri::command(async)]
pub fn insert_pages_from(
    state: State<AppState>,
    id: u64,
    at_index: u32,
    other_path: String,
    other_pages: Option<Vec<u32>>,
) -> Result<DocumentState, CommandError> {
    let other = load_other_pdf(Path::new(&other_path))?;
    let other_pages = other_pages.unwrap_or_else(|| all_pages(&other));
    let params = InsertFromParams {
        at_index,
        other_pages,
    };
    state.edit(DocumentId(id), |document| {
        page_tools::insert_from(document, &other, &params)
    })
}

#[tauri::command(async)]
pub fn crop_pages(
    state: State<AppState>,
    id: u64,
    params: CropParams,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        page_tools::crop(document, &params)
    })
}

#[tauri::command(async)]
pub fn uncrop_pages(
    state: State<AppState>,
    id: u64,
    params: UncropParams,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        page_tools::uncrop(document, &params)
    })
}

#[tauri::command(async)]
pub fn add_page_numbers(
    state: State<AppState>,
    id: u64,
    params: PageNumberParams,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        page_tools::add_page_numbers(document, &params)
    })
}

#[tauri::command(async)]
pub fn add_header_footer(
    state: State<AppState>,
    id: u64,
    params: HeaderFooterParams,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        page_tools::add_header_footer(document, &params)
    })
}

#[tauri::command(async)]
pub fn add_text_watermark(
    state: State<AppState>,
    id: u64,
    params: TextWatermarkParams,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        page_tools::add_text_watermark(document, &params)
    })
}

/// stamps a picture the person chose. `params.image` is filled from the
/// file here, so the picture never has to travel through the ui.
#[tauri::command(async)]
pub fn add_image_watermark(
    state: State<AppState>,
    id: u64,
    image_path: String,
    mut params: ImageWatermarkParams,
) -> Result<DocumentState, CommandError> {
    let path = Path::new(&image_path);
    params.image = fs::read(path).map_err(|error| {
        CommandError::failed(format!(
            "â€œ{}â€ canâ€™t be read. Choose the picture again. ({error})",
            file_name(path)
        ))
    })?;
    state.edit(DocumentId(id), |document| {
        page_tools::add_image_watermark(document, &params)
    })
}

/// takes off the page numbers, headers and footers, or watermarks that
/// Letters added. when there are none, it says so instead of making an
/// undo step that changes nothing.
#[tauri::command(async)]
pub fn remove_stamps(
    state: State<AppState>,
    id: u64,
    params: RemoveStampsParams,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        let changed = page_tools::remove_stamps(document, &params).map_err(|e| e.to_string())?;
        if changed == 0 {
            return Err(nothing_to_remove(params.kind));
        }
        Ok(())
    })
}

fn nothing_to_remove(kind: StampKind) -> String {
    let what = match kind {
        StampKind::PageNumbers => "page numbers",
        StampKind::HeaderFooter => "headers or footers",
        StampKind::Watermark => "watermarks",
    };
    format!("These pages have no {what} added by Letters, so there is nothing to remove.")
}

/// saves copies of some pages as a new pdf at a place chosen in the save
/// panel. the open document is not changed. returns the file written.
#[tauri::command(async)]
pub fn extract_pages(
    state: State<AppState>,
    id: u64,
    pages: Vec<u32>,
    save_path: String,
) -> Result<String, CommandError> {
    let destination = with_pdf_extension(Path::new(&save_path));
    let params = ExtractParams { pages };
    state.read_document(DocumentId(id), |document| {
        let mut extracted = page_tools::extract(document, &params)?;
        write_chosen_file(&mut extracted, &destination)?;
        Ok(path_text(&destination))
    })
}

/// one file a split made.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitFile {
    pub path: String,
    pub name: String,
    pub first_page: u32,
    pub page_count: u32,
    /// one page alone was already bigger than the size asked for
    pub over_size_limit: bool,
}

/// splits the document into new files in `folder`, named after
/// `base_name`. existing files are never replaced: a taken name gets a
/// number. the open document is not changed.
#[tauri::command(async)]
pub fn split_document(
    state: State<AppState>,
    id: u64,
    params: SplitParams,
    folder: String,
    base_name: String,
) -> Result<Vec<SplitFile>, CommandError> {
    state.read_document(DocumentId(id), |document| {
        let parts = page_tools::split(document, &params)?;
        let facts: Vec<(u32, u32, bool)> = parts
            .iter()
            .map(|part| (part.first_page, part.page_count, part.over_size_limit))
            .collect();
        let new_parts = parts
            .into_iter()
            .map(|part| NewPart {
                document: part.document,
                name_suffix: part.name_suffix,
            })
            .collect();
        let written = write_parts(Path::new(&folder), &base_name, new_parts)?;
        Ok(written
            .iter()
            .zip(facts)
            .map(
                |(path, (first_page, page_count, over_size_limit))| SplitFile {
                    path: path_text(path),
                    name: file_name(path),
                    first_page,
                    page_count,
                    over_size_limit,
                },
            )
            .collect())
    })
}

/// combines whole pdfs, in the order given, into a new file at a place
/// chosen in the save panel. nothing is opened; the ui offers that.
/// `names` label each document's bookmark in the new file.
#[tauri::command(async)]
pub fn merge_documents(
    paths: Vec<String>,
    names: Vec<String>,
    save_path: String,
) -> Result<String, CommandError> {
    let destination = with_pdf_extension(Path::new(&save_path));
    let documents = paths
        .iter()
        .map(|path| load_other_pdf(Path::new(path)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut merged = page_tools::merge(&documents, &MergeParams { names })?;
    write_chosen_file(&mut merged, &destination)?;
    Ok(path_text(&destination))
}

/// how many pages another pdf has, so the ui can describe it before
/// inserting or merging. a locked pdf gives the page tools' message.
#[tauri::command(async)]
pub fn pdf_page_count(path: String) -> Result<u32, CommandError> {
    let document = load_other_pdf(Path::new(&path))?;
    Ok(document.get_pages().len() as u32)
}

fn all_pages(document: &lopdf::Document) -> Vec<u32> {
    (0..document.get_pages().len() as u32).collect()
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
