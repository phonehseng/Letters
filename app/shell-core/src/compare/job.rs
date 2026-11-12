//! one comparison of two documents, run on its own thread.
//!
//! a job opens both files twice: once with lopdf, which is how the
//! comparison engine reads text, and once in the pdf worker, so the
//! window can draw their pages with the usual tiles and the engine can
//! compare pictures. both stay open while the compare screen shows, for
//! the overlay pictures and the report, and are closed together by
//! `close`, which also deletes the job's folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use letters_compare::{
    aligned_images, compare, overlay_image, report_html, report_pdf, report_text, CompareError,
    CompareOptions, ComparisonResult, LopdfSource, PageSource, Progress, ReportOptions, RgbaImage,
    Stage, Summary,
};
use letters_pdf::{DocumentId, PageSize, RenderWorker};
use serde::Serialize;

use super::renderer::render_page;

/// a file to compare and the name people know it by.
#[derive(Clone, Debug)]
pub struct SideSpec {
    pub path: PathBuf,
    pub name: String,
}

/// one side of a comparison as the window draws it.
#[derive(Clone, Debug)]
pub struct OpenedSide {
    pub name: String,
    pub document: DocumentId,
    pub page_sizes: Vec<PageSize>,
}

/// how far a job has got, in the shape the window polls for.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum JobStatus {
    Running {
        stage: Option<Stage>,
        done: u32,
        total: u32,
    },
    /// the result itself is fetched once, with `CompareJob::result`
    Finished {
        summary: Summary,
    },
    Failed {
        message: String,
    },
    Cancelled,
}

/// what a job knows while it runs and after.
enum Outcome {
    Running(Option<Progress>),
    Finished(Arc<ComparisonResult>),
    Failed(String),
    Cancelled,
}

/// the three pictures of one page pair for the overlay view, lined up.
pub struct PairPictures {
    pub old: RgbaImage,
    pub new: RgbaImage,
    /// shared content grey, removed red, added green (the engine's colours)
    pub overlay: RgbaImage,
}

/// the formats a report can be saved in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReportFormat {
    Pdf,
    Html,
    Text,
}

pub struct CompareJob {
    pub id: u64,
    pub old: OpenedSide,
    pub new: OpenedSide,
    sources: Arc<(LopdfSource, LopdfSource)>,
    cancel: Arc<AtomicBool>,
    outcome: Arc<Mutex<Outcome>>,
    /// how many progress reports arrived, so tests can check every page reported
    progress_reports: Arc<AtomicU32>,
    thread: Mutex<Option<JoinHandle<()>>>,
    worker: RenderWorker,
    folder: PathBuf,
    closed: AtomicBool,
}

impl CompareJob {
    /// opens both files and starts comparing them in the background.
    ///
    /// a file that can't be read, or is locked with a password, is
    /// refused with the engine's own sentence, and nothing stays open.
    pub fn start(
        worker: &RenderWorker,
        id: u64,
        old: &SideSpec,
        new: &SideSpec,
        options: CompareOptions,
        folder: PathBuf,
    ) -> Result<CompareJob, String> {
        let (old_source, old_side) = open_side(worker, old)?;
        let (new_source, new_side) = match open_side(worker, new) {
            Ok(opened) => opened,
            Err(message) => {
                worker.close(old_side.document);
                return Err(message);
            }
        };
        let job = CompareJob {
            id,
            old: old_side,
            new: new_side,
            sources: Arc::new((old_source, new_source)),
            cancel: Arc::new(AtomicBool::new(false)),
            outcome: Arc::new(Mutex::new(Outcome::Running(None))),
            progress_reports: Arc::new(AtomicU32::new(0)),
            thread: Mutex::new(None),
            worker: worker.clone(),
            folder,
            closed: AtomicBool::new(false),
        };
        let thread = job.spawn(options);
        *job.thread.lock().unwrap() = Some(thread);
        Ok(job)
    }

    /// runs the comparison on its own thread, reporting into `outcome`.
    fn spawn(&self, options: CompareOptions) -> JoinHandle<()> {
        let sources = Arc::clone(&self.sources);
        let cancel = Arc::clone(&self.cancel);
        let outcome = Arc::clone(&self.outcome);
        let reports = Arc::clone(&self.progress_reports);
        std::thread::spawn(move || {
            let mut report_progress = |progress: Progress| {
                reports.fetch_add(1, Ordering::SeqCst);
                *lock(&outcome) = Outcome::Running(Some(progress));
            };
            let finished = compare(
                &sources.0,
                &sources.1,
                &options,
                &mut report_progress,
                &cancel,
            );
            *lock(&outcome) = match finished {
                Ok(result) => Outcome::Finished(Arc::new(result)),
                Err(CompareError::Cancelled) => Outcome::Cancelled,
                Err(error) => Outcome::Failed(error.to_string()),
            };
        })
    }

    pub fn status(&self) -> JobStatus {
        match &*lock(&self.outcome) {
            Outcome::Running(progress) => JobStatus::Running {
                stage: progress.map(|p| p.stage),
                done: progress.map_or(0, |p| p.done),
                total: progress.map_or(0, |p| p.total),
            },
            Outcome::Finished(result) => JobStatus::Finished {
                summary: result.summary.clone(),
            },
            Outcome::Failed(message) => JobStatus::Failed {
                message: message.clone(),
            },
            Outcome::Cancelled => JobStatus::Cancelled,
        }
    }

    /// the finished result, or none while it runs (or when it failed).
    pub fn result(&self) -> Option<Arc<ComparisonResult>> {
        match &*lock(&self.outcome) {
            Outcome::Finished(result) => Some(Arc::clone(result)),
            _ => None,
        }
    }

    /// how many times the engine said how far it had got.
    pub fn progress_reports(&self) -> u32 {
        self.progress_reports.load(Ordering::SeqCst)
    }

    /// asks the comparison to stop before its next page.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    /// waits until the comparison thread has ended (finished, failed or stopped).
    pub fn wait(&self) {
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }

    /// the old page, the new page and the overlay of pair `index`, at `scale`.
    pub fn pair_pictures(&self, index: usize, scale: f32) -> Result<PairPictures, String> {
        let result = self.result().ok_or("The comparison hasn’t finished yet.")?;
        let pair = result
            .page_pairs
            .get(index)
            .ok_or("That page pair isn’t part of this comparison.")?;
        let (old, new) = (&self.sources.0, &self.sources.1);
        let aligned = aligned_images(old, new, pair, scale).map_err(|error| error.to_string())?;
        let overlay = overlay_image(old, new, pair, scale).map_err(|error| error.to_string())?;
        Ok(PairPictures {
            old: aligned.left,
            new: aligned.right,
            overlay,
        })
    }

    /// the report in the chosen format, as bytes ready to write.
    pub fn report(&self, format: ReportFormat, options: &ReportOptions) -> Result<Vec<u8>, String> {
        let result = self.result().ok_or("The comparison hasn’t finished yet.")?;
        match format {
            ReportFormat::Pdf => report_pdf(&result, &self.sources.0, &self.sources.1, options)
                .map_err(|error| error.to_string()),
            ReportFormat::Html => Ok(report_html(&result, options).into_bytes()),
            ReportFormat::Text => Ok(report_text(&result, options).into_bytes()),
        }
    }

    /// stops the comparison, closes both documents in the worker and
    /// deletes the job's folder. safe to call more than once.
    pub fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.cancel();
        self.wait();
        self.worker.close(self.old.document);
        self.worker.close(self.new.document);
        let _ = fs::remove_dir_all(&self.folder);
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }
}

impl Drop for CompareJob {
    fn drop(&mut self) {
        self.close();
    }
}

/// reads a file with lopdf first, so a locked file gets the engine's own
/// message, then opens it in the worker for drawing.
fn open_side(worker: &RenderWorker, spec: &SideSpec) -> Result<(LopdfSource, OpenedSide), String> {
    let source = LopdfSource::open(&spec.path).map_err(|error| error.to_string())?;
    let info = worker
        .open(&spec.path, None)
        .map_err(|error| error.to_string())?;
    let (drawer, document, sizes) = (worker.clone(), info.id, info.page_sizes.clone());
    let source = source.with_renderer(move |page, scale| {
        let size = *sizes.get(page as usize).ok_or("no such page")?;
        render_page(&drawer, document, size, page, scale)
    });
    let side = OpenedSide {
        name: spec.name.clone(),
        document: info.id,
        page_sizes: info.page_sizes,
    };
    // the engine and lopdf must agree on the pages, or changes land on the wrong ones
    if source.page_count() as usize != side.page_sizes.len() {
        worker.close(side.document);
        return Err(format!(
            "“{}” can’t be compared: its list of pages is damaged.",
            spec.name
        ));
    }
    Ok((source, side))
}

/// a poisoned lock still holds a usable answer: the thread that panicked
/// can only have been writing a whole new outcome.
fn lock(outcome: &Mutex<Outcome>) -> MutexGuard<'_, Outcome> {
    outcome
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
