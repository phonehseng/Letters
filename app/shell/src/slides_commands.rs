//! the commands of presentation mode (.pptx and .odp): the slide editor's
//! doors into letters-shell-core (src/slides), where the work is done and
//! tested, and the slide show's full screen and audience window.

use std::path::{Path, PathBuf};

use letters_pdf::{DocumentId, RenderWorker, TileKey};
use letters_shell_core::recents::thumbnail_name;
use letters_shell_core::slides::find::Found;
use letters_shell_core::slides::new_objects::{NewObject, PointBox};
use letters_shell_core::slides::pictures::{LeftOut, PictureViewer, SlidePicture};
use letters_shell_core::slides::sessions::{Edited, OpenedDeck, SavedDeck, SlidesSessions};
use letters_shell_core::slides::view::{DeckView, SlideView};
use letters_shell_core::slides::{Operation, PdfLayout};
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder, Window};

use crate::errors::CommandError;
use crate::state::AppState;

/// how wide the Recents picture of a deck's first slide is, in pixels
/// (the same width as a pdf's first page there, see commands.rs)
const THUMBNAIL_WIDTH_PIXELS: f64 = 240.0;

/// the windows of a slide show's audience are named for their deck, and
/// start with "window-" so the app's permissions cover them
const AUDIENCE_PREFIX: &str = "window-audience-";

/// the decks open in any window, managed beside `AppState`.
pub struct SlidesState {
    sessions: SlidesSessions,
}

impl SlidesState {
    pub fn start(app: &AppHandle) -> SlidesState {
        let cache_dir = app.path().app_cache_dir().unwrap_or_default();
        SlidesState {
            sessions: SlidesSessions::new(&cache_dir),
        }
    }
}

/// the app's pdf renderer, as the slides' pictures use it.
struct Renderer<'a>(&'a RenderWorker);

impl PictureViewer for Renderer<'_> {
    fn open(&self, pdf: &Path) -> Result<u64, String> {
        let info = self.0.open(pdf, None).map_err(|error| error.to_string())?;
        Ok(info.id.0)
    }

    fn reload(&self, document: u64, pdf: &Path) -> Result<(), String> {
        self.0
            .reload(DocumentId(document), pdf)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn close(&self, document: u64) {
        self.0.close(DocumentId(document));
    }
}

fn failed(message: String) -> CommandError {
    CommandError::failed(message)
}

fn renderer(state: &AppState) -> Result<Renderer<'_>, CommandError> {
    state
        .worker()
        .map(Renderer)
        .map_err(CommandError::engine_unavailable)
}

/// a closed window's decks close with it (see lib.rs).
pub fn close_window(app: &AppHandle, window: &str) {
    let (Some(slides), Some(state)) = (app.try_state::<SlidesState>(), app.try_state::<AppState>())
    else {
        return;
    };
    if let Ok(viewer) = renderer(&state) {
        slides.sessions.close_window(window, &viewer);
    }
}

/// Recents lists the deck with a picture of its first slide.
fn remember(state: &AppState, slides: &SlidesState, id: u64, path: &Path, slide_count: usize) {
    state.recents.lock().unwrap().add(path, slide_count);
    let Ok(viewer) = renderer(state) else {
        return;
    };
    let Ok(deck) = slides.sessions.deck(id) else {
        return;
    };
    let Some(first) = deck.slides.first() else {
        return;
    };
    let Ok(pictures) = slides
        .sessions
        .pictures(id, std::slice::from_ref(&first.id), &viewer)
    else {
        return;
    };
    if let Some(picture) = pictures.first() {
        save_thumbnail(viewer.0, picture, &state.thumbnail_dir, path);
    }
}

fn save_thumbnail(worker: &RenderWorker, picture: &SlidePicture, folder: &Path, path: &Path) {
    let scale = THUMBNAIL_WIDTH_PIXELS / picture.width.max(1.0);
    let key = TileKey {
        document: DocumentId(picture.document),
        page: 0,
        scale_milli: (scale * 1000.0) as u32,
        column: 0,
        row: 0,
    };
    if let Ok(tile) = worker.tile(key) {
        let _ = std::fs::create_dir_all(folder);
        let _ = std::fs::write(
            folder.join(format!("{}.png", thumbnail_name(path))),
            tile.as_slice(),
        );
    }
}

#[tauri::command(async)]
pub fn slides_open(
    window: Window,
    state: State<AppState>,
    slides: State<SlidesState>,
    path: String,
    recovered: bool,
) -> Result<OpenedDeck, CommandError> {
    let path = PathBuf::from(path);
    let opened = slides
        .sessions
        .open(window.label(), &path, recovered)
        .map_err(failed)?;
    remember(&state, &slides, opened.id, &path, opened.deck.slides.len());
    Ok(opened)
}

#[tauri::command(async)]
pub fn slides_close(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
) -> Result<(), CommandError> {
    slides.sessions.close(id, &renderer(&state)?);
    Ok(())
}

#[tauri::command(async)]
pub fn slides_deck(slides: State<SlidesState>, id: u64) -> Result<DeckView, CommandError> {
    slides.sessions.deck(id).map_err(failed)
}

#[tauri::command(async)]
pub fn slides_slide(
    slides: State<SlidesState>,
    id: u64,
    slide_id: String,
) -> Result<SlideView, CommandError> {
    slides.sessions.slide(id, &slide_id).map_err(failed)
}

/// one of the engine's editing operations, as json tagged by `op`.
#[tauri::command(async)]
pub fn slides_apply(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
    operation: Operation,
) -> Result<Edited, CommandError> {
    slides
        .sessions
        .apply(id, &operation, &renderer(&state)?)
        .map_err(failed)
}

/// Duplicate: copies of the selected objects beside them.
#[tauri::command(async)]
pub fn slides_duplicate(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
    slide_id: String,
    object_ids: Vec<u32>,
) -> Result<Edited, CommandError> {
    slides
        .sessions
        .duplicate(id, &slide_id, &object_ids, &renderer(&state)?)
        .map_err(failed)
}

/// the font file a family is drawn with, as raw bytes for the typing
/// field's FontFace; an empty answer when the web view can't use it.
#[tauri::command(async)]
pub fn slides_font_face(
    slides: State<SlidesState>,
    id: u64,
    family: String,
    bold: bool,
    italic: bool,
) -> Result<Response, CommandError> {
    let bytes = slides
        .sessions
        .face_bytes(id, &family, bold, italic)
        .map_err(failed)?;
    Ok(Response::new(bytes.unwrap_or_default()))
}

/// Hide Slide and Show Slide.
#[tauri::command(async)]
pub fn slides_set_hidden(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
    slide_id: String,
    hidden: bool,
) -> Result<Edited, CommandError> {
    slides
        .sessions
        .set_hidden(id, &slide_id, hidden, &renderer(&state)?)
        .map_err(failed)
}

#[tauri::command(async)]
pub fn slides_undo(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
) -> Result<Edited, CommandError> {
    slides.sessions.undo(id, &renderer(&state)?).map_err(failed)
}

#[tauri::command(async)]
pub fn slides_redo(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
) -> Result<Edited, CommandError> {
    slides.sessions.redo(id, &renderer(&state)?).map_err(failed)
}

#[tauri::command(async)]
pub fn slides_insert(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
    slide_id: String,
    object: NewObject,
    at: PointBox,
) -> Result<Edited, CommandError> {
    slides
        .sessions
        .insert(id, &slide_id, &object, at, &renderer(&state)?)
        .map_err(failed)
}

#[tauri::command(async)]
pub fn slides_insert_picture(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
    slide_id: String,
    path: String,
    at: PointBox,
) -> Result<Edited, CommandError> {
    slides
        .sessions
        .insert_picture(id, &slide_id, Path::new(&path), at, &renderer(&state)?)
        .map_err(failed)
}

/// the pictures of some slides, drawn now if they are new or changed.
#[tauri::command(async)]
pub fn slides_pictures(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
    slide_ids: Vec<String>,
) -> Result<Vec<SlidePicture>, CommandError> {
    slides
        .sessions
        .pictures(id, &slide_ids, &renderer(&state)?)
        .map_err(failed)
}

/// a slide's picture without the text being typed into.
#[tauri::command(async)]
pub fn slides_picture_without(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
    slide_id: String,
    left_out: LeftOut,
) -> Result<SlidePicture, CommandError> {
    slides
        .sessions
        .picture_without(id, &slide_id, left_out, &renderer(&state)?)
        .map_err(failed)
}

#[tauri::command(async)]
pub fn slides_find(
    slides: State<SlidesState>,
    id: u64,
    words: String,
    match_case: bool,
) -> Result<Vec<Found>, CommandError> {
    slides.sessions.find(id, &words, match_case).map_err(failed)
}

#[tauri::command(async)]
pub fn slides_replace(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
    found: Found,
    words: String,
    replacement: String,
    match_case: bool,
) -> Result<Edited, CommandError> {
    slides
        .sessions
        .replace(
            id,
            &found,
            &words,
            &replacement,
            match_case,
            &renderer(&state)?,
        )
        .map_err(failed)
}

/// saves to the deck's own file, or to `to` (Save As, .pptx or .odp).
#[tauri::command(async)]
pub fn slides_save(
    state: State<AppState>,
    slides: State<SlidesState>,
    id: u64,
    to: Option<String>,
    replace_changed: Option<bool>,
) -> Result<SavedDeck, CommandError> {
    let replace_changed = replace_changed.unwrap_or(false);
    // Save over its own file: a change another program made is asked about first
    if to.is_none() && !replace_changed {
        if let Some(message) = slides.sessions.changed_on_disk(id).map_err(failed)? {
            return Err(CommandError::changed_on_disk(message));
        }
    }
    let saved = slides
        .sessions
        .save(id, to.as_deref().map(Path::new), replace_changed)
        .map_err(failed)?;
    let count = slides
        .sessions
        .deck(id)
        .map(|deck| deck.slides.len())
        .unwrap_or(0);
    remember(&state, &slides, id, Path::new(&saved.path), count);
    Ok(saved)
}

#[tauri::command(async)]
pub fn slides_autosave(slides: State<SlidesState>, id: u64) -> Result<(), CommandError> {
    slides.sessions.autosave(id).map_err(failed)
}

#[tauri::command(async)]
pub fn slides_discard_recovery(slides: State<SlidesState>, id: u64) -> Result<(), CommandError> {
    slides.sessions.discard_recovery(id).map_err(failed)
}

/// Export as PDF; returns what the drawing had to approximate.
#[tauri::command(async)]
pub fn slides_export_pdf(
    slides: State<SlidesState>,
    id: u64,
    layout: PdfLayout,
    path: String,
) -> Result<Vec<String>, CommandError> {
    slides
        .sessions
        .export_pdf(id, layout, Path::new(&path))
        .map_err(failed)
}

/// the screens the slide show can use.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screens {
    pub count: usize,
}

#[tauri::command(async)]
pub fn slides_screens(window: Window) -> Result<Screens, CommandError> {
    let monitors = window
        .available_monitors()
        .map_err(|error| failed(error.to_string()))?;
    Ok(Screens {
        count: monitors.len(),
    })
}

/// the slide show fills the window's screen, or gives it back.
#[tauri::command(async)]
pub fn slides_full_screen(window: Window, on: bool) -> Result<(), CommandError> {
    window
        .set_fullscreen(on)
        .map_err(|error| failed(error.to_string()))
}

/// opens the audience's window for deck `id` on a screen other than this
/// window's, filling it. the presenter view stays in this window.
#[tauri::command(async)]
pub fn slides_open_audience(
    app: AppHandle,
    window: Window,
    id: u64,
) -> Result<String, CommandError> {
    let monitors = window
        .available_monitors()
        .map_err(|error| failed(error.to_string()))?;
    let here = window.current_monitor().ok().flatten();
    let other = monitors
        .into_iter()
        .find(|monitor| {
            here.as_ref()
                .is_none_or(|current| current.position() != monitor.position())
        })
        .ok_or_else(|| {
            failed(
                "There is no second screen. Connect a projector or a display, then try again."
                    .into(),
            )
        })?;
    let label = format!("{AUDIENCE_PREFIX}{id}");
    let scale = other.scale_factor();
    let position = other.position().to_logical::<f64>(scale);
    let for_window = label.clone();
    let app_for_window = app.clone();
    app.run_on_main_thread(move || {
        let built = WebviewWindowBuilder::new(
            &app_for_window,
            &for_window,
            WebviewUrl::App("index.html".into()),
        )
        .title("Slide Show")
        .position(position.x, position.y)
        .fullscreen(true)
        .build();
        if let Err(error) = built {
            eprintln!("could not open the audience window: {error}");
        }
    })
    .map_err(|error| failed(error.to_string()))?;
    Ok(label)
}

/// closes the audience's window when the show ends.
#[tauri::command(async)]
pub fn slides_close_audience(app: AppHandle, id: u64) {
    if let Some(window) = app.get_webview_window(&format!("{AUDIENCE_PREFIX}{id}")) {
        let _ = window.destroy();
    }
}
