//! the Edit tab: changing page text in place and the pictures on a page,
//! one command per operation of core/edit (letters-edit).
//!
//! every change is one call through `state.edit`, so it is one undo step
//! and only the working copy changes until Save. each answers the page's
//! objects as they are after the change, because the engine's ids are only
//! valid until the next edit of a page: the ui swaps them in straight away.
//!
//! an id the engine no longer knows ("That text or picture has changed
//! since it was selected") is not shown as an error: the answer says
//! `stale` and carries the page's fresh objects, so the ui can select the
//! same thing again quietly.

use std::path::{Path, PathBuf};

use letters_edit::{
    self as edit, Color, EditError, FontNotice, ImageEditResult, ImageFit, NewText, NewTextFont,
    PageObjects, Point, Rect, StandardFont, TextEditOptions, TextEditResult, TextOverflow,
    TextStyleChange,
};
use letters_pdf::DocumentId;
use letters_shell_core::edit::fit::replace_fitting;
use letters_shell_core::edit::fonts::display_font;
use letters_shell_core::edit::pictures::{check_picture, picture_kind, read_picture, PictureKind};
use letters_shell_core::forms::write_chosen_bytes;
use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::State;

use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

/// what a text change answers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEditAnswer {
    document: DocumentState,
    /// the page as it is now, with fresh ids
    objects: PageObjects,
    /// fallback fonts the engine used, each with its sentence
    notices: Vec<FontNotice>,
    /// set when re-set text needs more lines than its paragraph had
    overflow: Option<TextOverflow>,
    /// the id was out of date; nothing changed and `objects` are fresh
    stale: bool,
}

/// what a picture change answers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageEditAnswer {
    document: DocumentState,
    objects: PageObjects,
    /// the picture's id now (none after deleting it)
    image_id: Option<String>,
    stale: bool,
}

/// the font new text is written in: a standard font, or a TrueType font
/// file from the font library (only the glyphs used are copied in).
#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AddTextFont {
    Standard { font: StandardFont },
    File { path: String },
}

/// new page text as the ui sends it: where it starts (top left), the
/// words, and how they look.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddedText {
    position: Point,
    text: String,
    font: AddTextFont,
    size: f32,
    color: Color,
}

fn failed(error: impl ToString) -> CommandError {
    CommandError::failed(error.to_string())
}

fn page_objects_now(state: &AppState, id: u64, page: usize) -> Result<PageObjects, CommandError> {
    state.read_document(DocumentId(id), |document| {
        edit::page_objects(document, page).map_err(failed)
    })
}

/// runs one text change. a stale id answers the fresh page instead of an error.
fn text_change(
    state: &AppState,
    id: u64,
    page: usize,
    work: impl FnOnce(&mut lopdf::Document) -> Result<TextEditResult, EditError>,
) -> Result<TextEditAnswer, CommandError> {
    let mut made: Option<TextEditResult> = None;
    let mut stale = false;
    let changed = state.edit(DocumentId(id), |document| match work(document) {
        Ok(result) => {
            made = Some(result);
            Ok(())
        }
        Err(EditError::UnknownObject) => {
            stale = true;
            Err(EditError::UnknownObject)
        }
        Err(error) => Err(error),
    });
    match (changed, made) {
        (Ok(document), Some(result)) => Ok(TextEditAnswer {
            document,
            objects: result.objects,
            notices: result.notices,
            overflow: result.overflow,
            stale: false,
        }),
        (Err(_), _) if stale => Ok(TextEditAnswer {
            document: state.documents.state(DocumentId(id))?,
            objects: page_objects_now(state, id, page)?,
            notices: Vec::new(),
            overflow: None,
            stale: true,
        }),
        (Err(error), _) => Err(error),
        (Ok(_), None) => Err(CommandError::failed(
            "The change couldn’t be made. Try again.",
        )),
    }
}

/// runs one picture change, the same way.
fn image_change(
    state: &AppState,
    id: u64,
    page: usize,
    work: impl FnOnce(&mut lopdf::Document) -> Result<ImageEditResult, EditError>,
) -> Result<ImageEditAnswer, CommandError> {
    let mut made: Option<ImageEditResult> = None;
    let mut stale = false;
    let changed = state.edit(DocumentId(id), |document| match work(document) {
        Ok(result) => {
            made = Some(result);
            Ok(())
        }
        Err(EditError::UnknownObject) => {
            stale = true;
            Err(EditError::UnknownObject)
        }
        Err(error) => Err(error),
    });
    match (changed, made) {
        (Ok(document), Some(result)) => Ok(ImageEditAnswer {
            document,
            objects: result.objects,
            image_id: result.image_id,
            stale: false,
        }),
        (Err(_), _) if stale => Ok(ImageEditAnswer {
            document: state.documents.state(DocumentId(id))?,
            objects: page_objects_now(state, id, page)?,
            image_id: None,
            stale: true,
        }),
        (Err(error), _) => Err(error),
        (Ok(_), None) => Err(CommandError::failed(
            "The change couldn’t be made. Try again.",
        )),
    }
}

/// the page's text blocks and pictures, with ids for the commands below.
#[tauri::command(async)]
pub fn edit_page_objects(
    state: State<AppState>,
    id: u64,
    page: usize,
) -> Result<PageObjects, CommandError> {
    page_objects_now(&state, id, page)
}

/// replaces a line's or a block's text. `fit` puts a block's new text into
/// the lines it had, smaller where needed ("Shrink to Fit").
#[tauri::command(async)]
pub fn edit_replace_text(
    state: State<AppState>,
    id: u64,
    page: usize,
    target: String,
    text: String,
    options: Option<TextEditOptions>,
    fit: Option<bool>,
) -> Result<TextEditAnswer, CommandError> {
    let options = options.unwrap_or_default();
    text_change(&state, id, page, |document| {
        if fit.unwrap_or(false) {
            replace_fitting(document, page, &target, &text, &options)
        } else {
            edit::replace_text(document, page, &target, &text, &options)
        }
    })
}

#[tauri::command(async)]
pub fn edit_insert_text(
    state: State<AppState>,
    id: u64,
    page: usize,
    line_id: String,
    index: usize,
    text: String,
    options: Option<TextEditOptions>,
) -> Result<TextEditAnswer, CommandError> {
    let options = options.unwrap_or_default();
    text_change(&state, id, page, |document| {
        edit::insert_text(document, page, &line_id, index, &text, &options)
    })
}

#[tauri::command(async)]
pub fn edit_delete_chars(
    state: State<AppState>,
    id: u64,
    page: usize,
    line_id: String,
    start: usize,
    end: usize,
) -> Result<TextEditAnswer, CommandError> {
    text_change(&state, id, page, |document| {
        edit::delete_chars(document, page, &line_id, start, end)
    })
}

/// removes a block, line or run of text.
#[tauri::command(async)]
pub fn edit_delete_text(
    state: State<AppState>,
    id: u64,
    page: usize,
    target: String,
) -> Result<TextEditAnswer, CommandError> {
    text_change(&state, id, page, |document| {
        edit::delete_text(document, page, &target)
    })
}

/// changes the size, colour or font of a run, line, block or characters.
#[tauri::command(async)]
pub fn edit_set_text_style(
    state: State<AppState>,
    id: u64,
    page: usize,
    target: String,
    range: Option<(usize, usize)>,
    style: TextStyleChange,
    options: Option<TextEditOptions>,
) -> Result<TextEditAnswer, CommandError> {
    let options = options.unwrap_or_default();
    text_change(&state, id, page, |document| {
        edit::set_text_style(document, page, &target, range, &style, &options)
    })
}

/// adds new text to the page itself (not a comment on top of it).
#[tauri::command(async)]
pub fn edit_add_text(
    state: State<AppState>,
    id: u64,
    page: usize,
    added: AddedText,
) -> Result<TextEditAnswer, CommandError> {
    let font = match added.font {
        AddTextFont::Standard { font } => NewTextFont::Standard { font },
        AddTextFont::File { path } => NewTextFont::Embedded {
            bytes: read_font_file(Path::new(&path))?,
        },
    };
    let new_text = NewText {
        position: added.position,
        text: added.text,
        font,
        size: added.size,
        color: added.color,
    };
    text_change(&state, id, page, |document| {
        edit::add_text(document, page, &new_text)
    })
}

/// a font file from the font library, for Add Text.
fn read_font_file(path: &Path) -> Result<Vec<u8>, CommandError> {
    const MAX_FONT_BYTES: u64 = 40 * 1024 * 1024;
    letters_shell_core::forms::read_chosen_file(path, MAX_FONT_BYTES).map_err(CommandError::failed)
}

/// puts a picture on the page: from a file the person chose or dropped,
/// or from bytes pasted from the clipboard.
#[tauri::command(async)]
pub fn edit_add_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    rect: Rect,
    path: Option<String>,
    bytes: Option<Vec<u8>>,
) -> Result<ImageEditAnswer, CommandError> {
    let picture = picture_from(path, bytes)?;
    image_change(&state, id, page, |document| {
        edit::add_image(document, page, rect, &picture)
    })
}

fn picture_from(path: Option<String>, bytes: Option<Vec<u8>>) -> Result<Vec<u8>, CommandError> {
    match (path, bytes) {
        (Some(path), _) => read_picture(Path::new(&path)).map_err(CommandError::failed),
        (None, Some(bytes)) => {
            check_picture(&bytes, "The pasted picture").map_err(CommandError::failed)?;
            Ok(bytes)
        }
        (None, None) => Err(CommandError::failed("Choose a picture first.")),
    }
}

/// draws another picture in one picture's place.
#[tauri::command(async)]
pub fn edit_replace_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_id: String,
    path: String,
    fit: ImageFit,
) -> Result<ImageEditAnswer, CommandError> {
    let picture = read_picture(Path::new(&path)).map_err(CommandError::failed)?;
    image_change(&state, id, page, |document| {
        edit::replace_image(document, page, &image_id, &picture, fit)
    })
}

#[tauri::command(async)]
pub fn edit_move_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_id: String,
    dx: f32,
    dy: f32,
) -> Result<ImageEditAnswer, CommandError> {
    image_change(&state, id, page, |document| {
        edit::move_image(document, page, &image_id, dx, dy)
    })
}

#[tauri::command(async)]
pub fn edit_resize_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_id: String,
    rect: Rect,
) -> Result<ImageEditAnswer, CommandError> {
    image_change(&state, id, page, |document| {
        edit::resize_image(document, page, &image_id, rect)
    })
}

/// turns a picture by `degrees`, clockwise on screen.
#[tauri::command(async)]
pub fn edit_rotate_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_id: String,
    degrees: f32,
) -> Result<ImageEditAnswer, CommandError> {
    image_change(&state, id, page, |document| {
        edit::rotate_image(document, page, &image_id, degrees)
    })
}

#[tauri::command(async)]
pub fn edit_delete_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_id: String,
) -> Result<ImageEditAnswer, CommandError> {
    image_change(&state, id, page, |document| {
        edit::delete_image(document, page, &image_id)
    })
}

/// draws a picture above (`to_front`) or below everything else on its page.
#[tauri::command(async)]
pub fn edit_restack_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_id: String,
    to_front: bool,
) -> Result<ImageEditAnswer, CommandError> {
    image_change(&state, id, page, |document| {
        if to_front {
            edit::bring_image_to_front(document, page, &image_id)
        } else {
            edit::send_image_to_back(document, page, &image_id)
        }
    })
}

/// saves a picture of the page as its own file ("Save Image As…"). the
/// extension follows what the picture really is, so a jpeg is never saved
/// as .png. answers the path written.
#[tauri::command(async)]
pub fn edit_save_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_id: String,
    path: String,
) -> Result<String, CommandError> {
    let extracted = state.read_document(DocumentId(id), |document| {
        edit::extract_image(document, page, &image_id).map_err(failed)
    })?;
    let kind = picture_kind(&extracted.bytes).unwrap_or(PictureKind::Png);
    let destination = with_extension(PathBuf::from(path), kind);
    write_chosen_bytes(&destination, &extracted.bytes).map_err(CommandError::failed)?;
    Ok(destination.to_string_lossy().into_owned())
}

fn with_extension(path: PathBuf, kind: PictureKind) -> PathBuf {
    let current = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    let fits = match kind {
        PictureKind::Png => current.as_deref() == Some("png"),
        PictureKind::Jpeg => matches!(current.as_deref(), Some("jpg" | "jpeg")),
    };
    if fits {
        path
    } else {
        path.with_extension(kind.extension())
    }
}

/// the embedded font of some page text as a font file for the web view,
/// when it draws every character of `sample`. refused otherwise, and the
/// ui then uses the closest installed font.
#[tauri::command(async)]
pub fn edit_font_face(
    state: State<AppState>,
    id: u64,
    font_id: String,
    sample: String,
) -> Result<Response, CommandError> {
    let font = state.read_document(DocumentId(id), |document| {
        Ok(display_font(document, &font_id, &sample))
    })?;
    match font {
        Some(font) => Ok(Response::new(font.bytes)),
        None => Err(CommandError::failed(
            "This text’s font can’t be shown while typing.",
        )),
    }
}
