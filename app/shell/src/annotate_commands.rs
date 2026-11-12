//! the annotation tools, one command each, and the page text they select.
//!
//! every command that changes the document is one call through
//! `state.edit`, exactly like the page tools (see page_commands.rs): the
//! engine in core/annotate changes the working copy, and the shell takes
//! care of undo, showing the result and the new state. reading commands
//! (the list of annotations) go through `state.read_document` and make no
//! undo step. pages are counted from 0 and positions are in points from
//! the top left of the page as shown, y down.
//!
//! new annotations get the author name from the settings unless the ui
//! named one, so the name lives in one place.

use std::fs;
use std::path::Path;

use letters_annotate::{
    self as annotate, AnnotateError, AnnotationChanges, AnnotationInfo, FlattenScope,
    FlattenSummary, ImageParams, InkParams, LineParams, NoteParams, PolygonParams, ReplyParams,
    ShapeParams, TextBoxParams, TextMarkupParams,
};
use letters_pdf::{DocumentId, PageText};
use letters_shell_core::picture::{picture_size_of_file, PictureSize};
use letters_shell_core::session::file_name;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::RectDto;
use crate::documents::DocumentState;
use crate::errors::CommandError;
use crate::state::AppState;

/// what an edit that makes or changes one annotation answers: the
/// document as it is now, and the annotation as the engine left it, so
/// the ui can select it straight away.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationEdit {
    pub document: DocumentState,
    pub annotation: Option<AnnotationInfo>,
}

/// what an edit that makes several annotations at once answers.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationsEdit {
    pub document: DocumentState,
    pub annotations: Vec<AnnotationInfo>,
}

/// one page's part of a text selection that runs across pages.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMarkup {
    pub page: usize,
    pub params: TextMarkupParams,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlattenEdit {
    pub document: DocumentState,
    pub summary: FlattenSummary,
}

/// one character of a page and where it is drawn.
#[derive(Serialize)]
pub struct TextCharDto {
    ch: char,
    rect: RectDto,
    /// the line it belongs to, counted from 0
    line: u32,
}

#[derive(Serialize)]
pub struct PageTextDto {
    chars: Vec<TextCharDto>,
    /// one box per line, covering its characters
    lines: Vec<RectDto>,
}

/// runs one edit that makes or changes an annotation, keeping what the
/// engine says about it.
fn edit_annotation(
    state: &AppState,
    id: u64,
    make: impl FnOnce(&mut lopdf::Document) -> Result<AnnotationInfo, AnnotateError>,
) -> Result<AnnotationEdit, CommandError> {
    let mut made = None;
    let document = state.edit(DocumentId(id), |document| {
        made = Some(make(document)?);
        Ok::<(), AnnotateError>(())
    })?;
    Ok(AnnotationEdit {
        document,
        annotation: made,
    })
}

/// the settings' author name when the ui left it out.
fn with_author(state: &AppState, author: &mut Option<String>) {
    if author.is_none() {
        *author = Some(state.author_name());
    }
}

/// highlight, underline, strike through or squiggle text: one rectangle
/// per selected line.
#[tauri::command(async)]
pub fn add_text_markup(
    state: State<AppState>,
    id: u64,
    page: usize,
    mut params: TextMarkupParams,
) -> Result<AnnotationEdit, CommandError> {
    with_author(&state, &mut params.author);
    edit_annotation(&state, id, |document| {
        annotate::add_text_markup(document, page, &params)
    })
}

/// marks a selection that runs across pages: one annotation per page,
/// all made in one edit, so one undo takes the whole mark back.
#[tauri::command(async)]
pub fn add_text_markups(
    state: State<AppState>,
    id: u64,
    mut marks: Vec<PageMarkup>,
) -> Result<AnnotationsEdit, CommandError> {
    for mark in &mut marks {
        with_author(&state, &mut mark.params.author);
    }
    let mut made = Vec::new();
    let document = state.edit(DocumentId(id), |document| {
        for mark in &marks {
            made.push(annotate::add_text_markup(
                document,
                mark.page,
                &mark.params,
            )?);
        }
        Ok::<(), AnnotateError>(())
    })?;
    Ok(AnnotationsEdit {
        document,
        annotations: made,
    })
}

/// a sticky note with its comment.
#[tauri::command(async)]
pub fn add_note(
    state: State<AppState>,
    id: u64,
    page: usize,
    mut params: NoteParams,
) -> Result<AnnotationEdit, CommandError> {
    with_author(&state, &mut params.author);
    edit_annotation(&state, id, |document| {
        annotate::add_note(document, page, &params)
    })
}

/// a reply in the comment thread of any annotation.
#[tauri::command(async)]
pub fn add_reply(
    state: State<AppState>,
    id: u64,
    page: usize,
    reply_to: String,
    mut params: ReplyParams,
) -> Result<AnnotationEdit, CommandError> {
    with_author(&state, &mut params.author);
    edit_annotation(&state, id, |document| {
        annotate::add_reply(document, page, &reply_to, &params)
    })
}

#[tauri::command(async)]
pub fn add_text_box(
    state: State<AppState>,
    id: u64,
    page: usize,
    mut params: TextBoxParams,
) -> Result<AnnotationEdit, CommandError> {
    with_author(&state, &mut params.author);
    edit_annotation(&state, id, |document| {
        annotate::add_text_box(document, page, &params)
    })
}

/// a rectangle or an ellipse.
#[tauri::command(async)]
pub fn add_shape(
    state: State<AppState>,
    id: u64,
    page: usize,
    mut params: ShapeParams,
) -> Result<AnnotationEdit, CommandError> {
    with_author(&state, &mut params.author);
    edit_annotation(&state, id, |document| {
        annotate::add_shape(document, page, &params)
    })
}

/// a line, with arrowheads when asked.
#[tauri::command(async)]
pub fn add_line(
    state: State<AppState>,
    id: u64,
    page: usize,
    mut params: LineParams,
) -> Result<AnnotationEdit, CommandError> {
    with_author(&state, &mut params.author);
    edit_annotation(&state, id, |document| {
        annotate::add_line(document, page, &params)
    })
}

#[tauri::command(async)]
pub fn add_polygon(
    state: State<AppState>,
    id: u64,
    page: usize,
    mut params: PolygonParams,
) -> Result<AnnotationEdit, CommandError> {
    with_author(&state, &mut params.author);
    edit_annotation(&state, id, |document| {
        annotate::add_polygon(document, page, &params)
    })
}

/// freehand drawing: every stroke of one gesture in one annotation.
#[tauri::command(async)]
pub fn add_ink(
    state: State<AppState>,
    id: u64,
    page: usize,
    mut params: InkParams,
) -> Result<AnnotationEdit, CommandError> {
    with_author(&state, &mut params.author);
    edit_annotation(&state, id, |document| {
        annotate::add_ink(document, page, &params)
    })
}

/// a picture the person chose. `params.image` is filled from the file
/// here, so the picture never has to travel through the ui.
#[tauri::command(async)]
pub fn add_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_path: String,
    mut params: ImageParams,
) -> Result<AnnotationEdit, CommandError> {
    let path = Path::new(&image_path);
    params.image = fs::read(path).map_err(|error| {
        CommandError::failed(format!(
            "“{}” can’t be read. Choose the picture again. ({error})",
            file_name(path)
        ))
    })?;
    with_author(&state, &mut params.author);
    edit_annotation(&state, id, |document| {
        annotate::add_image(document, page, &params)
    })
}

/// moves, resizes, restyles or rewrites one annotation.
#[tauri::command(async)]
pub fn update_annotation(
    state: State<AppState>,
    id: u64,
    page: usize,
    annotation_id: String,
    changes: AnnotationChanges,
) -> Result<AnnotationEdit, CommandError> {
    edit_annotation(&state, id, |document| {
        annotate::update(document, page, &annotation_id, &changes)
    })
}

/// removes an annotation with its pop-up and every reply to it.
#[tauri::command(async)]
pub fn delete_annotation(
    state: State<AppState>,
    id: u64,
    page: usize,
    annotation_id: String,
) -> Result<DocumentState, CommandError> {
    state.edit(DocumentId(id), |document| {
        annotate::delete(document, page, &annotation_id).map(|_| ())
    })
}

/// paints comments and marks into their pages for good. when there are
/// none it says so instead of making an undo step that changes nothing.
#[tauri::command(async)]
pub fn flatten_annotations(
    state: State<AppState>,
    id: u64,
    scope: FlattenScope,
) -> Result<FlattenEdit, CommandError> {
    let mut summary = FlattenSummary::default();
    let document = state.edit(DocumentId(id), |document| {
        summary = annotate::flatten(document, &scope).map_err(|error| error.to_string())?;
        if summary.flattened == 0 {
            return Err("There are no comments or marks to flatten here.".to_owned());
        }
        Ok(())
    })?;
    Ok(FlattenEdit { document, summary })
}

/// every annotation on one page, as the document is shown now.
#[tauri::command(async)]
pub fn list_annotations(
    state: State<AppState>,
    id: u64,
    page: usize,
) -> Result<Vec<AnnotationInfo>, CommandError> {
    state.read_document(DocumentId(id), |document| {
        Ok(annotate::list(document, page)?)
    })
}

/// every annotation in the document, page by page, from one reading of
/// the file. what the comments list and clicking on pages use.
#[tauri::command(async)]
pub fn list_all_annotations(
    state: State<AppState>,
    id: u64,
) -> Result<Vec<AnnotationInfo>, CommandError> {
    state.read_document(DocumentId(id), |document| {
        let page_count = document.get_pages().len();
        let mut all = Vec::new();
        for page in 0..page_count {
            all.extend(annotate::list(document, page)?);
        }
        Ok(all)
    })
}

/// the sentence the ui shows before flattening, from the engine.
#[tauri::command]
pub fn flatten_explanation() -> &'static str {
    annotate::FLATTEN_EXPLANATION
}

/// every character of a page with its box, for selecting text.
#[tauri::command(async)]
pub fn page_text(state: State<AppState>, id: u64, page: u32) -> Result<PageTextDto, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let text = worker.page_text(DocumentId(id), page)?;
    Ok(page_text_dto(text))
}

/// the size of a picture in pixels, so the ui can keep its proportions.
#[tauri::command(async)]
pub fn picture_size(path: String) -> Result<PictureSize, CommandError> {
    picture_size_of_file(Path::new(&path)).map_err(CommandError::failed)
}

/// the name written on new comments and marks.
#[tauri::command(async)]
pub fn annotation_author(state: State<AppState>) -> String {
    state.author_name()
}

/// changes the name written on new comments. a blank name goes back to
/// the computer's user name. returns the name now in use.
#[tauri::command(async)]
pub fn set_annotation_author(state: State<AppState>, name: String) -> String {
    state.set_author_name(&name)
}

fn page_text_dto(text: PageText) -> PageTextDto {
    PageTextDto {
        chars: text
            .chars
            .iter()
            .map(|character| TextCharDto {
                ch: character.ch,
                rect: RectDto::from(&character.rect),
                line: character.line,
            })
            .collect(),
        lines: text.lines.iter().map(RectDto::from).collect(),
    }
}
