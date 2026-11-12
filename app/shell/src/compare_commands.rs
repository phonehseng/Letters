//! the Compare screen: two versions of a document side by side, laid over
//! each other, and a report of what changed.
//!
//! a comparison runs as a job on its own thread (shell-core's compare/),
//! which the window polls for progress. both documents are opened in the
//! pdf worker under ids of their own, so the screen draws their pages with
//! the usual tiles; they are closed when the screen goes away, or with
//! the window. nothing here changes either document.

use std::path::{Path, PathBuf};

use letters_compare::{CompareOptions, ComparisonResult, ReportOptions};
use letters_pdf::DocumentId;
use letters_shell_core::compare::{
    copy_for_comparison, encode_png, safe_scale, CompareJob, CompareJobs, JobStatus, OpenedSide,
    ReportFormat, SideSpec,
};
use letters_shell_core::forms::{data_url, write_chosen_bytes};
use letters_shell_core::session::file_name;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State, Window};

use crate::documents::{page_dto, PageDto};
use crate::errors::CommandError;
use crate::state::AppState;

/// the most pixels one overlay picture may have: a us letter page at
/// about 2.8 pixels per point, enough for a sharp page on most screens.
const MAX_PICTURE_PIXELS: f32 = 4_000_000.0;

/// every comparison open in any window.
pub struct CompareState {
    jobs: CompareJobs,
}

impl CompareState {
    pub fn start(app: &AppHandle) -> Self {
        let cache_dir = app.path().app_cache_dir().unwrap_or_default();
        Self {
            jobs: CompareJobs::new(cache_dir.join("comparisons")),
        }
    }
}

/// closes the comparisons of a window that is going away.
pub fn close_window(app: &AppHandle, window: &str) {
    if let Some(state) = app.try_state::<CompareState>() {
        state.jobs.close_window(window);
    }
}

/// one side of a comparison: a file on disk, or a document open in a window.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CompareInput {
    File {
        path: String,
    },
    /// the open document as it is now, unsaved changes included
    Open {
        id: u64,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareSide {
    name: String,
    document_id: u64,
    pages: Vec<PageDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareStarted {
    job_id: u64,
    old: CompareSide,
    new: CompareSide,
}

/// the three pictures of one page pair, ready for `<img>`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairPicturesDto {
    old: String,
    new: String,
    overlay: String,
}

/// starts comparing `old` with `new` in the background.
#[tauri::command(async)]
pub fn compare_start(
    window: Window,
    state: State<AppState>,
    compare: State<CompareState>,
    old: CompareInput,
    new: CompareInput,
    options: CompareOptions,
) -> Result<CompareStarted, CommandError> {
    let worker = state.worker().map_err(CommandError::engine_unavailable)?;
    let (id, folder) = compare.jobs.prepare().map_err(CommandError::failed)?;
    let started = (|| {
        let old = side_spec(&state, old, &folder)?;
        let new = side_spec(&state, new, &folder)?;
        CompareJob::start(worker, id, &old, &new, options, folder.clone())
            .map_err(CommandError::failed)
    })();
    let job = match started {
        Ok(job) => compare.jobs.insert(window.label(), job),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&folder);
            return Err(error);
        }
    };
    Ok(CompareStarted {
        job_id: job.id,
        old: side_dto(&job.old),
        new: side_dto(&job.new),
    })
}

/// how far a comparison has got.
#[tauri::command(async)]
pub fn compare_status(
    compare: State<CompareState>,
    job_id: u64,
) -> Result<JobStatus, CommandError> {
    Ok(compare
        .jobs
        .get(job_id)
        .map_err(CommandError::failed)?
        .status())
}

/// the finished comparison: page pairs, changes, the summary.
#[tauri::command(async)]
pub fn compare_result(
    compare: State<CompareState>,
    job_id: u64,
) -> Result<ComparisonResult, CommandError> {
    let job = compare.jobs.get(job_id).map_err(CommandError::failed)?;
    let result = job
        .result()
        .ok_or_else(|| CommandError::failed("The comparison hasn’t finished yet."))?;
    Ok((*result).clone())
}

/// stops a comparison before its next page.
#[tauri::command(async)]
pub fn compare_cancel(compare: State<CompareState>, job_id: u64) {
    if let Ok(job) = compare.jobs.get(job_id) {
        job.cancel();
    }
}

/// closes a comparison: both documents leave the worker.
#[tauri::command(async)]
pub fn compare_close(compare: State<CompareState>, job_id: u64) {
    compare.jobs.close(job_id);
}

/// the old page, the new page and the overlay of one page pair, lined up.
#[tauri::command(async)]
pub fn compare_pair_pictures(
    compare: State<CompareState>,
    job_id: u64,
    pair: usize,
    scale: f32,
) -> Result<PairPicturesDto, CommandError> {
    let job = compare.jobs.get(job_id).map_err(CommandError::failed)?;
    let scale = picture_scale(&job, pair, scale);
    let pictures = job
        .pair_pictures(pair, scale)
        .map_err(CommandError::failed)?;
    let as_url = |image| encode_png(image).map(|png| data_url("image/png", &png));
    Ok(PairPicturesDto {
        old: as_url(&pictures.old).map_err(CommandError::failed)?,
        new: as_url(&pictures.new).map_err(CommandError::failed)?,
        overlay: as_url(&pictures.overlay).map_err(CommandError::failed)?,
    })
}

/// writes the report where the save panel said. returns the path written.
#[tauri::command(async)]
pub fn compare_save_report(
    compare: State<CompareState>,
    job_id: u64,
    path: String,
    format: ReportFormat,
    options: ReportOptions,
) -> Result<String, CommandError> {
    let job = compare.jobs.get(job_id).map_err(CommandError::failed)?;
    let bytes = job.report(format, &options).map_err(CommandError::failed)?;
    let path = PathBuf::from(path);
    write_chosen_bytes(&path, &bytes).map_err(CommandError::failed)?;
    Ok(path.to_string_lossy().into_owned())
}

/// what to compare from one side: a file is read where it is; an open
/// document is copied as it is now into the job's folder.
fn side_spec(
    state: &AppState,
    input: CompareInput,
    folder: &Path,
) -> Result<SideSpec, CommandError> {
    match input {
        CompareInput::File { path } => {
            let path = PathBuf::from(path);
            Ok(SideSpec {
                name: file_name(&path),
                path,
            })
        }
        CompareInput::Open { id } => {
            let worker = state.worker().map_err(CommandError::engine_unavailable)?;
            let (spec, _) =
                state
                    .documents
                    .with_session(worker, DocumentId(id), |session, _show| {
                        let copy = copy_for_comparison(session.working_copy(), folder)
                            .map_err(CommandError::failed)?;
                        let spec = SideSpec {
                            path: copy,
                            name: file_name(session.original_path()),
                        };
                        Ok((spec, None))
                    })?;
            Ok(spec)
        }
    }
}

fn side_dto(side: &OpenedSide) -> CompareSide {
    CompareSide {
        name: side.name.clone(),
        document_id: side.document.0,
        pages: side.page_sizes.iter().map(page_dto).collect(),
    }
}

/// the asked-for scale, made smaller for a page so big its picture would
/// take too much memory.
fn picture_scale(job: &CompareJob, pair: usize, scale: f32) -> f32 {
    let result = job.result();
    let largest = result
        .as_ref()
        .and_then(|result| result.page_pairs.get(pair))
        .map(|pair| {
            let sizes = [pair.left_size, pair.right_size];
            sizes
                .iter()
                .flatten()
                .fold((1.0f32, 1.0f32), |(w, h), size| {
                    (w.max(size.width), h.max(size.height))
                })
        })
        .unwrap_or((612.0, 792.0));
    let size = letters_pdf::PageSize {
        width_pt: largest.0,
        height_pt: largest.1,
    };
    safe_scale(size, scale, MAX_PICTURE_PIXELS)
}
