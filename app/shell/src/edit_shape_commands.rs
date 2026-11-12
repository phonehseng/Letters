//! the Edit tab's vector shapes and the newer picture tools: one command
//! per letters-edit operation, each one undo step through `state.edit`.
//!
//! shapes are named by a list of path ids, so a logo (one of the page's
//! suggested path groups) moves, restyles or restacks as one. every
//! command takes the person's consent for signed files (see
//! edit_answers.rs) and answers the page's fresh objects.

use letters_edit::{
    bring_paths_to_front, crop_image, delete_paths, duplicate_paths, flip_image, move_paths,
    resize_paths, rotate_paths, send_paths_to_back, set_image_opacity, set_path_points,
    set_path_style, EditConsent, FlipDirection, PathStyle, Point, Rect,
};
use tauri::State;

use crate::edit_answers::{content_change, ContentAnswer};
use crate::errors::CommandError;
use crate::state::AppState;

/// where a duplicate lands: a little down and to the right of the original,
/// so it is seen to be a copy (the way drawing programs place one).
const DUPLICATE_OFFSET: f32 = 10.0;

#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn edit_move_paths(
    state: State<AppState>,
    id: u64,
    page: usize,
    ids: Vec<String>,
    dx: f32,
    dy: f32,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        move_paths(document, page, &ids, dx, dy, &consent)
    })
}

/// stretches the shapes so the box around them becomes `rect`.
#[tauri::command(async)]
pub fn edit_resize_paths(
    state: State<AppState>,
    id: u64,
    page: usize,
    ids: Vec<String>,
    rect: Rect,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        resize_paths(document, page, &ids, rect, &consent)
    })
}

/// turns the shapes clockwise on screen around their middle.
#[tauri::command(async)]
pub fn edit_rotate_paths(
    state: State<AppState>,
    id: u64,
    page: usize,
    ids: Vec<String>,
    degrees: f32,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        rotate_paths(document, page, &ids, degrees, &consent)
    })
}

/// fill, outline, line width, dash or opacity; what is left out stays.
#[tauri::command(async)]
pub fn edit_style_paths(
    state: State<AppState>,
    id: u64,
    page: usize,
    ids: Vec<String>,
    style: PathStyle,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        set_path_style(document, page, &ids, &style, &consent)
    })
}

/// a rectangle's four corners or a line's two ends, on screen.
#[tauri::command(async)]
pub fn edit_path_points(
    state: State<AppState>,
    id: u64,
    page: usize,
    path_id: String,
    points: Vec<Point>,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        set_path_points(document, page, &path_id, &points, &consent)
    })
}

#[tauri::command(async)]
pub fn edit_delete_paths(
    state: State<AppState>,
    id: u64,
    page: usize,
    ids: Vec<String>,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        delete_paths(document, page, &ids, &consent)
    })
}

/// draws the shapes above (`to_front`) or below everything else.
#[tauri::command(async)]
pub fn edit_restack_paths(
    state: State<AppState>,
    id: u64,
    page: usize,
    ids: Vec<String>,
    to_front: bool,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        if to_front {
            bring_paths_to_front(document, page, &ids, &consent)
        } else {
            send_paths_to_back(document, page, &ids, &consent)
        }
    })
}

#[tauri::command(async)]
pub fn edit_duplicate_paths(
    state: State<AppState>,
    id: u64,
    page: usize,
    ids: Vec<String>,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        duplicate_paths(
            document,
            page,
            &ids,
            DUPLICATE_OFFSET,
            DUPLICATE_OFFSET,
            &consent,
        )
    })
}

/// shows only `rect` (on screen) of a picture; every pixel stays in the
/// file, so the crop can be changed again.
#[tauri::command(async)]
pub fn edit_crop_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_id: String,
    rect: Rect,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        crop_image(document, page, &image_id, rect, &consent)
    })
}

/// mirrors a picture left to right (`horizontal`) or top to bottom.
#[tauri::command(async)]
pub fn edit_flip_image(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_id: String,
    direction: FlipDirection,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        flip_image(document, page, &image_id, direction, &consent)
    })
}

/// 0 is invisible, 1 fully opaque.
#[tauri::command(async)]
pub fn edit_image_opacity(
    state: State<AppState>,
    id: u64,
    page: usize,
    image_id: String,
    opacity: f32,
    consent: Option<EditConsent>,
) -> Result<ContentAnswer, CommandError> {
    let consent = consent.unwrap_or_default();
    content_change(&state, id, page, |document| {
        set_image_opacity(document, page, &image_id, opacity, &consent)
    })
}
