//! the indexing job: one background thread that keeps the index matching
//! the files, and the handle the app searches through.
//!
//! it is built never to get in the way:
//!
//! - incremental: a file is read again only when its size or time
//!   changed, and then only if its content checksum changed too
//! - throttled: after each file it rests for a moment, longer after a
//!   long file, so the pdf engine and the disk stay free for the person
//! - pausable: pausing takes effect between pages, not at the end of a
//!   500 page file, and is remembered across restarts
//! - safe to stop at any moment: checkpoints are written every few
//!   files, and a restart simply carries on with what is not indexed yet
//!
//! passwords are never asked for or stored: a locked file is indexed by
//! its name only and marked as locked.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use letters_search::extract::{extract_pdf, extract_text, extract_word, OutlineEntry};
use letters_search::index::path_key;
use letters_search::{
    fingerprint, DocKind, DocNote, ExtractError, ExtractedDocument, Fingerprint, Index, PdfText,
    SearchRequest, SearchResults,
};
use serde::Serialize;

use super::settings::{LibrarySettings, LibrarySettingsStore};
use super::sources::files_to_index;

/// what the job is doing, for the status line on the search screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobPhase {
    Starting,
    /// listing the files and checking which changed
    Scanning,
    Indexing,
    /// everything is indexed; waiting for changes
    Idle,
    Paused,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatus {
    pub phase: JobPhase,
    /// files read in this pass, and how many need reading
    pub done: usize,
    pub total: usize,
    /// the file being read now
    pub current: Option<String>,
    pub documents: usize,
    pub passages: usize,
    pub bytes_on_disk: u64,
    pub locked: usize,
    pub needs_text_recognition: usize,
    pub unreadable: usize,
    pub last_error: Option<String>,
    /// milliseconds since 1970 when the last full pass ended
    pub last_finished: Option<u64>,
    /// the stored index was damaged and was started again
    pub recovered: bool,
}

impl Default for JobStatus {
    fn default() -> Self {
        JobStatus {
            phase: JobPhase::Starting,
            done: 0,
            total: 0,
            current: None,
            documents: 0,
            passages: 0,
            bytes_on_disk: 0,
            locked: 0,
            needs_text_recognition: 0,
            unreadable: 0,
            last_error: None,
            last_finished: None,
            recovered: false,
        }
    }
}

/// the files Letters already knows about, asked for at each pass.
#[derive(Clone, Debug, Default)]
pub struct SourceLists {
    pub recent_files: Vec<String>,
    pub pinned_folders: Vec<String>,
}

/// how the job paces itself. the app uses the defaults; tests go faster.
#[derive(Clone, Copy, Debug)]
pub struct JobOptions {
    /// the least rest after each file
    pub rest: Duration,
    /// files are looked at again this often even without a change
    pub rescan_every: Duration,
    /// a checkpoint after this many changed documents...
    pub commit_every: usize,
    /// ...or this long, whichever comes first
    pub commit_after: Duration,
    /// word and text files larger than this are indexed by name only
    pub max_file_bytes: u64,
}

impl Default for JobOptions {
    fn default() -> Self {
        JobOptions {
            rest: Duration::from_millis(15),
            rescan_every: Duration::from_secs(10 * 60),
            commit_every: 50,
            commit_after: Duration::from_secs(5),
            max_file_bytes: 200 * 1024 * 1024,
        }
    }
}

#[derive(Default)]
struct Control {
    wake: bool,
    rebuild: bool,
    stop: bool,
}

struct Shared {
    index: Mutex<Index>,
    status: Mutex<JobStatus>,
    control: Mutex<Control>,
    signal: Condvar,
    settings: LibrarySettingsStore,
    options: JobOptions,
}

impl Shared {
    fn control(&self) -> MutexGuard<'_, Control> {
        self.control.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn index(&self) -> MutexGuard<'_, Index> {
        self.index.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn set_status(&self, change: impl FnOnce(&mut JobStatus)) {
        change(&mut self.status.lock().unwrap_or_else(|p| p.into_inner()));
    }

    /// true when the current pass should stop: paused, stopping or rebuilding.
    fn interrupted(&self) -> bool {
        let control = self.control();
        control.stop || control.rebuild || self.settings.get().paused
    }

    /// waits up to `time`, ending early when anything asks for attention.
    fn rest(&self, time: Duration) {
        let control = self.control();
        let _ = self
            .signal
            .wait_timeout_while(control, time, |c| !c.stop && !c.rebuild && !c.wake);
    }
}

/// the search everyone shares: the index, its settings and its job.
pub struct LibrarySearch {
    shared: Arc<Shared>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

type Sources = Box<dyn Fn() -> SourceLists + Send>;

impl LibrarySearch {
    /// opens the index in `index_folder` and starts the job.
    pub fn start(
        index_folder: &Path,
        settings_file: PathBuf,
        pdf: Box<dyn PdfText + Send>,
        sources: Sources,
        options: JobOptions,
    ) -> Result<LibrarySearch, String> {
        let index = Index::open(index_folder).map_err(|e| e.to_string())?;
        let recovered = index.was_recovered();
        let shared = Arc::new(Shared {
            index: Mutex::new(index),
            status: Mutex::new(JobStatus {
                recovered,
                ..JobStatus::default()
            }),
            control: Mutex::new(Control::default()),
            signal: Condvar::new(),
            settings: LibrarySettingsStore::load(settings_file),
            options,
        });
        let worker = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("search-indexing".into())
            .spawn(move || run(worker, pdf, sources))
            .map_err(|e| e.to_string())?;
        Ok(LibrarySearch {
            shared,
            thread: Mutex::new(Some(thread)),
        })
    }

    pub fn search(&self, request: &SearchRequest) -> Result<SearchResults, String> {
        let mut request = request.clone();
        if request.now == 0 {
            request.now = now_millis();
        }
        self.shared
            .index()
            .search(&request)
            .map_err(|e| e.to_string())
    }

    pub fn status(&self) -> JobStatus {
        self.shared.status.lock().unwrap().clone()
    }

    pub fn settings(&self) -> LibrarySettings {
        self.shared.settings.get()
    }

    /// changes the settings and has the job look at the files again.
    pub fn update_settings(&self, change: impl FnOnce(&mut LibrarySettings)) -> LibrarySettings {
        let settings = self.shared.settings.update(change);
        self.wake();
        settings
    }

    pub fn pause(&self) {
        self.update_settings(|s| s.paused = true);
    }

    pub fn resume(&self) {
        self.update_settings(|s| s.paused = false);
    }

    /// a file was moved or renamed in Letters: its entry follows it at
    /// once, without reading it again (the next pass sees the same content).
    pub fn document_moved(&self, old: &str, new: &str) -> bool {
        self.shared.index().rename_document(old, new)
    }

    /// asks the job to look for changes now.
    pub fn wake(&self) {
        self.shared.control().wake = true;
        self.shared.signal.notify_all();
    }

    /// throws the index away and reads every file again.
    pub fn rebuild(&self) {
        self.shared.control().rebuild = true;
        self.shared.settings.update(|s| s.paused = false);
        self.shared.signal.notify_all();
    }

    /// deletes the index and pauses indexing, so it is not built again
    /// until the person resumes it.
    pub fn delete_index(&self) -> Result<(), String> {
        self.shared.settings.update(|s| s.paused = true);
        self.wake();
        let mut index = self.shared.index();
        index.clear().map_err(|e| e.to_string())?;
        drop(index);
        refresh_counts(&self.shared);
        Ok(())
    }

    /// ends the job after writing a checkpoint. safe to call twice.
    pub fn stop(&self) {
        self.shared.control().stop = true;
        self.shared.signal.notify_all();
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }

    /// for tests: waits until a pass has finished (or the job paused).
    pub fn wait_until_settled(&self, timeout: Duration) -> bool {
        let started = Instant::now();
        while started.elapsed() < timeout {
            let phase = self.status().phase;
            if phase == JobPhase::Idle || phase == JobPhase::Paused {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }
}

impl Drop for LibrarySearch {
    fn drop(&mut self) {
        self.stop();
    }
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// the job's thread.
fn run(shared: Arc<Shared>, mut pdf: Box<dyn PdfText + Send>, sources: Sources) {
    refresh_counts(&shared);
    loop {
        if take_rebuild(&shared) {
            if let Err(error) = shared.index().clear() {
                shared.set_status(|s| s.last_error = Some(error.to_string()));
            }
            refresh_counts(&shared);
        }
        if shared.control().stop {
            break;
        }
        if shared.settings.get().paused {
            shared.set_status(|s| {
                s.phase = JobPhase::Paused;
                s.current = None;
            });
            shared.rest(Duration::from_secs(3600));
            shared.control().wake = false;
            continue;
        }
        let finished = run_pass(&shared, pdf.as_mut(), &sources());
        commit(&shared);
        refresh_counts(&shared);
        if finished {
            shared.set_status(|s| {
                s.phase = JobPhase::Idle;
                s.current = None;
                s.last_finished = Some(now_millis());
            });
            shared.rest(shared.options.rescan_every);
            shared.control().wake = false;
        }
    }
    commit(&shared);
}

fn take_rebuild(shared: &Shared) -> bool {
    std::mem::take(&mut shared.control().rebuild)
}

fn commit(shared: &Shared) {
    if let Err(error) = shared.index().commit() {
        shared.set_status(|s| s.last_error = Some(error.to_string()));
    }
}

/// one look at every file. false when it was interrupted part way.
fn run_pass(shared: &Shared, pdf: &mut (dyn PdfText + Send), sources: &SourceLists) -> bool {
    // this pass answers any wake-up so far; left set, it would cut every rest short
    shared.control().wake = false;
    shared.set_status(|s| {
        s.phase = JobPhase::Scanning;
        s.done = 0;
        s.total = 0;
    });
    let settings = shared.settings.get();
    let files = files_to_index(&settings, &sources.recent_files, &sources.pinned_folders);
    let todo = changed_files(shared, &files);
    shared.set_status(|s| {
        s.phase = JobPhase::Indexing;
        s.total = todo.len();
    });
    let mut last_commit = Instant::now();
    for (done, (path, print)) in todo.into_iter().enumerate() {
        if shared.interrupted() {
            return false;
        }
        let shown = path.to_string_lossy().into_owned();
        shared.set_status(|s| s.current = Some(shown.clone()));
        let started = Instant::now();
        let Some(extracted) = read_file(shared, pdf, &path, print.size) else {
            return false;
        };
        let kind = DocKind::of_path(&path).unwrap_or(DocKind::Text);
        {
            let mut index = shared.index();
            if let Err(error) = index.add_document(&shown, kind, print, extracted) {
                shared.set_status(|s| s.last_error = Some(error.to_string()));
            }
            let due = index.uncommitted_changes() >= shared.options.commit_every
                || last_commit.elapsed() >= shared.options.commit_after;
            if due {
                if let Err(error) = index.commit() {
                    shared.set_status(|s| s.last_error = Some(error.to_string()));
                }
                last_commit = Instant::now();
            }
        }
        shared.set_status(|s| s.done = done + 1);
        // rest longer after a long file: about a fifth of the time it took
        let rest = shared.options.rest + (started.elapsed() / 5).min(Duration::from_millis(400));
        shared.rest(rest);
    }
    true
}

/// drops files that are gone and returns those that are new or changed.
fn changed_files(shared: &Shared, files: &[PathBuf]) -> Vec<(PathBuf, Fingerprint)> {
    let wanted: HashSet<String> = files
        .iter()
        .map(|f| path_key(&f.to_string_lossy()))
        .collect();
    let mut index = shared.index();
    index.retain(|doc| wanted.contains(&path_key(&doc.path)));
    let mut todo = Vec::new();
    for file in files {
        let path = file.to_string_lossy();
        let Some((size, modified)) = size_and_time(file) else {
            continue;
        };
        let stored = index.document(&path).map(|doc| doc.fingerprint);
        if stored.is_some_and(|s| s.size == size && s.modified == modified) {
            continue;
        }
        let Some(print) = fingerprint(file) else {
            continue;
        };
        // same bytes, new time: remember the time and do not read it again
        if stored.is_some_and(|s| s.size == print.size && s.hash == print.hash) {
            index.refresh_fingerprint(&path, print);
            continue;
        }
        todo.push((file.clone(), print));
    }
    todo
}

fn size_and_time(path: &Path) -> Option<(u64, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    Some((metadata.len(), modified))
}

/// reads one file. none when the job was interrupted while reading it.
fn read_file(
    shared: &Shared,
    pdf: &mut (dyn PdfText + Send),
    path: &Path,
    size: u64,
) -> Option<ExtractedDocument> {
    let result = match DocKind::of_path(path) {
        Some(DocKind::Pdf) => {
            let mut guarded = Guarded { inner: pdf, shared };
            extract_pdf(path, &mut guarded)
        }
        _ if size > shared.options.max_file_bytes => Err(ExtractError::Unreadable(
            "It is too large to search.".into(),
        )),
        Some(DocKind::Word) => std::fs::read(path)
            .map_err(|e| ExtractError::Unreadable(e.to_string()))
            .and_then(|bytes| extract_word(path, &bytes)),
        _ => std::fs::read(path)
            .map(|bytes| extract_text(path, &bytes))
            .map_err(|e| ExtractError::Unreadable(e.to_string())),
    };
    match result {
        Ok(document) => Some(document),
        Err(ExtractError::Stopped) => None,
        Err(ExtractError::Locked) => Some(name_only(path, DocNote::Locked)),
        Err(ExtractError::Unreadable(reason)) => {
            Some(name_only(path, DocNote::Unreadable { reason }))
        }
    }
}

/// a file that could not be read is still found by its name, and the
/// note says why its text is missing.
fn name_only(path: &Path, note: DocNote) -> ExtractedDocument {
    ExtractedDocument {
        title: letters_search::model::title_from_path(path),
        pages: 0,
        passages: Vec::new(),
        notes: vec![note],
    }
}

/// the app's pdf engine, with pausing between pages.
struct Guarded<'a> {
    inner: &'a mut (dyn PdfText + Send),
    shared: &'a Shared,
}

impl PdfText for Guarded<'_> {
    fn open(&mut self, path: &Path) -> Result<u32, ExtractError> {
        self.inner.open(path)
    }
    fn page_text(&mut self, page: u32) -> Result<String, ExtractError> {
        self.inner.page_text(page)
    }
    fn outline(&mut self) -> Vec<OutlineEntry> {
        self.inner.outline()
    }
    fn close(&mut self) {
        self.inner.close()
    }
    fn should_stop(&mut self) -> bool {
        self.shared.interrupted()
    }
}

fn refresh_counts(shared: &Shared) {
    let index = shared.index();
    let stats = index.stats();
    let (mut locked, mut scans, mut unreadable) = (0, 0, 0);
    for doc in index.documents() {
        for note in &doc.notes {
            match note {
                DocNote::Locked => locked += 1,
                DocNote::NeedsTextRecognition { .. } => scans += 1,
                DocNote::Unreadable { .. } => unreadable += 1,
            }
        }
    }
    drop(index);
    shared.set_status(|s| {
        s.documents = stats.documents;
        s.passages = stats.passages;
        s.bytes_on_disk = stats.bytes_on_disk;
        s.locked = locked;
        s.needs_text_recognition = scans;
        s.unreadable = unreadable;
    });
}

#[cfg(test)]
#[path = "job_tests.rs"]
mod tests;
