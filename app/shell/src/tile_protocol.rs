//! serves page tiles and thumbnails to the ui as ordinary images.
//!
//! the ui shows a tile with a plain `<img>` whose address looks like
//! `tile://localhost/3/0/1500/1/2` (document 3, page 0, 1.5 pixels per
//! point, column 1, row 2). images arrive as raw bytes this way, which
//! is much cheaper than passing them through json.

use std::fs;
use std::path::Path;

use letters_shell_core::address::{parse, Wanted};
use tauri::http::{Request, Response, StatusCode};
use tauri::{Manager, UriSchemeContext, UriSchemeResponder, Wry};

use crate::state::AppState;

pub fn handle(
    context: UriSchemeContext<'_, Wry>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = context.app_handle().clone();
    let path = request.uri().path().to_owned();
    // drawing can take a while, so it must not block the thread that
    // keeps the windows responsive
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        responder.respond(answer(&state, &path));
    });
}

fn answer(state: &AppState, path: &str) -> Response<Vec<u8>> {
    match parse(path) {
        Some(Wanted::Tile(key)) => match state.worker().map(|worker| worker.tile(key)) {
            Ok(Ok(tile)) => png(tile.to_vec()),
            Ok(Err(error)) => failure(StatusCode::NOT_FOUND, &error.to_string()),
            Err(reason) => failure(StatusCode::SERVICE_UNAVAILABLE, &reason),
        },
        Some(Wanted::Thumbnail(name)) => read_thumbnail(&state.thumbnail_dir, &name),
        None => failure(StatusCode::BAD_REQUEST, "not a tile address"),
    }
}

fn read_thumbnail(thumbnail_dir: &Path, name: &str) -> Response<Vec<u8>> {
    match fs::read(thumbnail_dir.join(format!("{name}.png"))) {
        Ok(bytes) => png(bytes),
        Err(_) => failure(StatusCode::NOT_FOUND, "no thumbnail yet"),
    }
}

fn png(bytes: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "image/png")
        // our own cache is the only cache: letting the webview keep copies
        // too would quietly undo the memory limit
        .header("Cache-Control", "no-store")
        .body(bytes)
        .expect("a response with static headers is always valid")
}

fn failure(status: StatusCode, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain")
        .body(message.as_bytes().to_vec())
        .expect("a response with static headers is always valid")
}
