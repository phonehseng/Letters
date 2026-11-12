//! the indexing job from the outside, with a fake pdf engine that reads
//! text files: incremental passes, restarts, pausing, resting, locked
//! files, exclusions, delete and rebuild.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// a pdf engine that reads ".pdf" files as plain text, one page per
/// line, so tests need no pdfium. files with "locked" in their name
/// need a password.
struct TextAsPdf {
    pages: Vec<String>,
    opened: Arc<AtomicUsize>,
    page_delay: Duration,
}

impl PdfText for TextAsPdf {
    fn open(&mut self, path: &Path) -> Result<u32, ExtractError> {
        if path.to_string_lossy().contains("locked") {
            return Err(ExtractError::Locked);
        }
        self.opened.fetch_add(1, Ordering::SeqCst);
        let text =
            std::fs::read_to_string(path).map_err(|e| ExtractError::Unreadable(e.to_string()))?;
        self.pages = text.lines().map(str::to_string).collect();
        Ok(self.pages.len() as u32)
    }
    fn page_text(&mut self, page: u32) -> Result<String, ExtractError> {
        thread::sleep(self.page_delay);
        Ok(self.pages[page as usize].clone())
    }
    fn outline(&mut self) -> Vec<OutlineEntry> {
        Vec::new()
    }
    fn close(&mut self) {
        self.pages.clear();
    }
}

struct Fixture {
    root: tempfile::TempDir,
    opened: Arc<AtomicUsize>,
}

impl Fixture {
    fn new() -> Fixture {
        Fixture {
            root: tempfile::tempdir().unwrap(),
            opened: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn library(&self) -> PathBuf {
        self.root.path().join("library")
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.library().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    fn start(&self, options: JobOptions, page_delay: Duration) -> LibrarySearch {
        let settings_file = self.root.path().join("library-search.json");
        let store = LibrarySettingsStore::load(settings_file.clone());
        let library = self.library().to_string_lossy().into_owned();
        store.update(|s| {
            super::super::settings::add_folder(&mut s.library_folders, &library);
        });
        let pdf = TextAsPdf {
            pages: Vec::new(),
            opened: Arc::clone(&self.opened),
            page_delay,
        };
        LibrarySearch::start(
            &self.root.path().join("index"),
            settings_file,
            Box::new(pdf),
            Box::new(SourceLists::default),
            options,
        )
        .unwrap()
    }
}

fn fast() -> JobOptions {
    JobOptions {
        rest: Duration::ZERO,
        rescan_every: Duration::from_secs(3600),
        commit_every: 3,
        ..JobOptions::default()
    }
}

fn find(search: &LibrarySearch, query: &str) -> Vec<String> {
    let results = search
        .search(&SearchRequest {
            query: query.into(),
            ..SearchRequest::default()
        })
        .unwrap();
    results.documents.into_iter().map(|d| d.title).collect()
}

fn settle(search: &LibrarySearch) {
    assert!(search.wait_until_settled(Duration::from_secs(20)));
}

/// waits for the next pass, started by `wake`, to finish.
fn rescan(search: &LibrarySearch) {
    let before = search.status().last_finished;
    search.wake();
    let started = Instant::now();
    while search.status().last_finished == before && started.elapsed() < Duration::from_secs(20) {
        thread::sleep(Duration::from_millis(10));
    }
    settle(search);
}

#[test]
fn indexes_a_folder_and_then_only_what_changed() {
    let fixture = Fixture::new();
    fixture.write("a.pdf", "first page about osmosis\nsecond page");
    fixture.write("b.pdf", "nothing to see");
    let notes = fixture.write("notes/c.md", "# Plan\n\nwrite the essay");
    let search = fixture.start(fast(), Duration::ZERO);
    settle(&search);
    assert_eq!(find(&search, "osmosis"), vec!["a"]);
    assert_eq!(find(&search, "essay"), vec!["c"]);
    let status = search.status();
    assert_eq!(status.documents, 3);
    assert_eq!(fixture.opened.load(Ordering::SeqCst), 2);

    // one file changes, one is deleted: only the changed one is read
    std::thread::sleep(Duration::from_millis(20));
    fixture.write("b.pdf", "now it mentions diffusion");
    std::fs::remove_file(notes).unwrap();
    rescan(&search);
    assert_eq!(fixture.opened.load(Ordering::SeqCst), 3);
    assert_eq!(find(&search, "diffusion"), vec!["b"]);
    assert!(find(&search, "essay").is_empty());
    assert_eq!(search.status().documents, 2);
}

#[test]
fn a_restart_carries_on_without_reading_files_again() {
    let fixture = Fixture::new();
    for i in 0..5 {
        fixture.write(&format!("f{i}.pdf"), &format!("file number {i}"));
    }
    {
        let search = fixture.start(fast(), Duration::ZERO);
        settle(&search);
        search.stop();
    }
    assert_eq!(fixture.opened.load(Ordering::SeqCst), 5);
    fixture.write("f5.pdf", "a new arrival");
    let search = fixture.start(fast(), Duration::ZERO);
    settle(&search);
    // only the new file was read; the others came from the checkpoint
    assert_eq!(fixture.opened.load(Ordering::SeqCst), 6);
    assert_eq!(find(&search, "arrival"), vec!["f5"]);
    assert_eq!(search.status().documents, 6);
}

#[test]
fn pausing_stops_between_pages_and_resuming_finishes() {
    let fixture = Fixture::new();
    let long: String = (0..200).map(|i| format!("page {i} text\n")).collect();
    fixture.write("long.pdf", &long);
    fixture.write("short.pdf", "short words");
    let search = fixture.start(fast(), Duration::from_millis(5));
    // let it get into the long file, then pause
    let started = Instant::now();
    while search.status().phase != JobPhase::Indexing && started.elapsed() < Duration::from_secs(5)
    {
        thread::sleep(Duration::from_millis(5));
    }
    thread::sleep(Duration::from_millis(50));
    search.pause();
    settle(&search);
    assert_eq!(search.status().phase, JobPhase::Paused);
    // 200 pages at 5 ms would take a second: pausing did not wait for it
    assert!(
        started.elapsed() < Duration::from_millis(900),
        "{:?}",
        started.elapsed()
    );
    assert!(search.settings().paused);
    search.resume();
    let started = Instant::now();
    while search.status().documents < 2 && started.elapsed() < Duration::from_secs(20) {
        thread::sleep(Duration::from_millis(10));
    }
    settle(&search);
    assert_eq!(find(&search, "short"), vec!["short"]);
    assert_eq!(find(&search, "\"page 199\""), vec!["long"]);
}

#[test]
fn the_job_rests_between_files() {
    let fixture = Fixture::new();
    for i in 0..4 {
        fixture.write(&format!("r{i}.pdf"), "resting words");
    }
    let started = Instant::now();
    let search = fixture.start(
        JobOptions {
            rest: Duration::from_millis(60),
            ..fast()
        },
        Duration::ZERO,
    );
    settle(&search);
    assert!(
        started.elapsed() >= Duration::from_millis(220),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn locked_files_are_noted_and_found_by_name() {
    let fixture = Fixture::new();
    fixture.write("locked bank statement.pdf", "secret");
    fixture.write("open.pdf", "public");
    let search = fixture.start(fast(), Duration::ZERO);
    settle(&search);
    assert_eq!(search.status().locked, 1);
    assert!(find(&search, "secret").is_empty());
    assert_eq!(
        find(&search, "bank statement"),
        vec!["locked bank statement"]
    );
}

#[test]
fn excluded_folders_drop_out_and_the_index_can_be_deleted_and_rebuilt() {
    let fixture = Fixture::new();
    fixture.write("keep/k.pdf", "keep this");
    let private = fixture.write("private/p.pdf", "private diary");
    let search = fixture.start(fast(), Duration::ZERO);
    settle(&search);
    assert_eq!(find(&search, "diary"), vec!["p"]);
    let folder = private.parent().unwrap().to_string_lossy().into_owned();
    search.update_settings(|s| {
        super::super::settings::add_folder(&mut s.excluded_folders, &folder);
    });
    rescan(&search);
    assert!(find(&search, "diary").is_empty());

    search.delete_index().unwrap();
    let started = Instant::now();
    while search.status().phase != JobPhase::Paused && started.elapsed() < Duration::from_secs(20) {
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(search.status().phase, JobPhase::Paused);
    assert_eq!(search.status().documents, 0);
    assert!(find(&search, "keep").is_empty());
    search.rebuild();
    let started = Instant::now();
    while search.status().documents < 1 && started.elapsed() < Duration::from_secs(20) {
        thread::sleep(Duration::from_millis(10));
    }
    settle(&search);
    assert_eq!(find(&search, "keep"), vec!["k"]);
}
