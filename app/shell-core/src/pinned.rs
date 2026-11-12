//! folders the person pinned to the home screen, and the pdfs in them.
//!
//! only the list of folder paths is saved. the files are read from disk
//! each time, so a pdf saved into a pinned folder shows up without
//! anything having to watch the folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::Serialize;

/// a folder can hold thousands of files. the home screen shows the
/// newest, and this many is far more than anyone scrolls through.
pub const MAX_FILES_PER_FOLDER: usize = 200;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PinnedFolder {
    pub path: String,
    pub name: String,
    /// the pdfs directly inside the folder, newest first
    pub files: Vec<FolderFile>,
    /// how many pdfs the folder holds; more than `files` when the list was
    /// cut at `MAX_FILES_PER_FOLDER`, so the home screen can say so
    pub total: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderFile {
    pub path: String,
    pub name: String,
    /// seconds since 1970, so the ui can show "2 days ago"
    pub modified_at: u64,
}

pub struct PinnedFolders {
    store: PathBuf,
    paths: Vec<String>,
}

impl PinnedFolders {
    /// reads the saved list. a missing or unreadable file just means
    /// no pinned folders: losing them is never worth an error.
    pub fn load(store: PathBuf) -> Self {
        let paths = fs::read_to_string(&store)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Self { store, paths }
    }

    /// the pinned folder paths, in the order they were pinned.
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// adds a folder to the end of the list and saves it. pinning a
    /// folder that is already pinned changes nothing.
    pub fn pin(&mut self, path: &str) {
        if self.is_pinned(path) {
            return;
        }
        self.paths.push(path.to_owned());
        self.save();
    }

    pub fn unpin(&mut self, path: &str) {
        self.paths
            .retain(|pinned| !same_folder(Path::new(pinned), Path::new(path)));
        self.save();
    }

    fn is_pinned(&self, path: &str) -> bool {
        self.paths
            .iter()
            .any(|pinned| same_folder(Path::new(pinned), Path::new(path)))
    }

    fn save(&self) {
        if let Some(folder) = self.store.parent() {
            let _ = fs::create_dir_all(folder);
        }
        if let Ok(text) = serde_json::to_string_pretty(&self.paths) {
            let _ = fs::write(&self.store, text);
        }
    }
}

/// reads what is in each folder right now. a folder that has gone
/// missing is still listed, with no files, so it can be unpinned.
pub fn describe_folders(paths: &[String]) -> Vec<PinnedFolder> {
    paths
        .iter()
        .map(|path| {
            let (files, total) = pdfs_and_count(Path::new(path));
            PinnedFolder {
                path: path.clone(),
                name: last_name(Path::new(path)),
                files,
                total,
            }
        })
        .collect()
}

/// the pdfs directly inside a folder (not in folders within it),
/// newest first, at most `MAX_FILES_PER_FOLDER`.
pub fn pdfs_in(folder: &Path) -> Vec<FolderFile> {
    pdfs_and_count(folder).0
}

/// `pdfs_in`, with how many pdfs there are in all.
pub fn pdfs_and_count(folder: &Path) -> (Vec<FolderFile>, usize) {
    let Ok(entries) = fs::read_dir(folder) else {
        return (Vec::new(), 0);
    };
    let mut files: Vec<FolderFile> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_pdf_name(path))
        .filter_map(|path| describe_file(&path))
        .collect();
    // newest first; files changed in the same second fall back to name
    // order so the list doesn't shuffle between visits
    files.sort_by(|a, b| {
        b.modified_at
            .cmp(&a.modified_at)
            .then_with(|| a.name.cmp(&b.name))
    });
    let total = files.len();
    files.truncate(MAX_FILES_PER_FOLDER);
    (files, total)
}

/// true for names ending in .pdf in any mix of capitals.
fn is_pdf_name(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
}

/// a file's entry, or none for a folder (even one named like a pdf)
/// or a file that vanished while the folder was being read.
fn describe_file(path: &Path) -> Option<FolderFile> {
    // fs::metadata follows shortcuts (symbolic links) to the real file
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    Some(FolderFile {
        path: path.to_string_lossy().into_owned(),
        name: last_name(path),
        modified_at,
    })
}

/// the last part of a path, or the whole path for a drive like "C:\".
fn last_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// paths compare by their parts, so "C:\notes" and "C:\notes\" are the
/// same folder.
fn same_folder(a: &Path, b: &Path) -> bool {
    a.components().eq(b.components())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    fn seconds(time: SystemTime) -> u64 {
        time.duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    fn temp_dir(test_name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("letters-pinned-tests")
            .join(format!("{test_name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// makes a file whose last change was `age_seconds` ago.
    fn make_file(path: &Path, age_seconds: u64) -> SystemTime {
        let file = File::create(path).unwrap();
        let modified = SystemTime::now() - Duration::from_secs(age_seconds);
        file.set_modified(modified).unwrap();
        modified
    }

    fn names(files: &[FolderFile]) -> Vec<&str> {
        files.iter().map(|file| file.name.as_str()).collect()
    }

    fn text(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn lists_pdfs_newest_first() {
        let dir = temp_dir("newest-first");
        make_file(&dir.join("old.pdf"), 3000);
        let newest = make_file(&dir.join("new.pdf"), 10);
        make_file(&dir.join("middle.pdf"), 600);

        let files = pdfs_in(&dir);
        assert_eq!(names(&files), ["new.pdf", "middle.pdf", "old.pdf"]);
        assert_eq!(files[0].modified_at, seconds(newest));
        assert_eq!(files[0].path, text(&dir.join("new.pdf")));
    }

    #[test]
    fn ignores_other_files_and_matches_pdf_in_any_case() {
        let dir = temp_dir("only-pdfs");
        make_file(&dir.join("essay.PDF"), 10);
        make_file(&dir.join("notes.txt"), 10);
        make_file(&dir.join("pdf"), 10);
        make_file(&dir.join("scan.pdf.zip"), 10);
        fs::create_dir(dir.join("folder.pdf")).unwrap();

        assert_eq!(names(&pdfs_in(&dir)), ["essay.PDF"]);
    }

    #[test]
    fn does_not_look_inside_folders_within_the_folder() {
        let dir = temp_dir("not-recursive");
        make_file(&dir.join("top.pdf"), 10);
        fs::create_dir(dir.join("inner")).unwrap();
        make_file(&dir.join("inner").join("deep.pdf"), 10);

        assert_eq!(names(&pdfs_in(&dir)), ["top.pdf"]);
    }

    #[test]
    fn shows_at_most_the_newest_two_hundred() {
        let dir = temp_dir("capped");
        for number in 0..MAX_FILES_PER_FOLDER + 5 {
            make_file(&dir.join(format!("{number:03}.pdf")), number as u64 * 60);
        }
        let (files, total) = pdfs_and_count(&dir);
        assert_eq!(files.len(), MAX_FILES_PER_FOLDER);
        assert_eq!(files[0].name, "000.pdf");
        // the folder says how many there are, so the cut isn't silent
        assert_eq!(total, MAX_FILES_PER_FOLDER + 5);
    }

    #[test]
    fn a_missing_folder_is_listed_with_no_files() {
        let dir = temp_dir("missing");
        let gone = text(&dir.join("gone"));

        let folders = describe_folders(std::slice::from_ref(&gone));
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].path, gone);
        assert_eq!(folders[0].name, "gone");
        assert!(folders[0].files.is_empty());
    }

    #[test]
    fn pinned_folders_survive_a_restart() {
        let dir = temp_dir("restart");
        let store = dir.join("data").join("pinned-folders.json");
        let mut pinned = PinnedFolders::load(store.clone());
        pinned.pin("C:\\notes");
        pinned.pin("C:\\lectures");

        let reloaded = PinnedFolders::load(store);
        assert_eq!(reloaded.paths(), ["C:\\notes", "C:\\lectures"]);
    }

    #[test]
    fn pinning_twice_keeps_one_entry() {
        let dir = temp_dir("duplicates");
        let mut pinned = PinnedFolders::load(dir.join("pinned-folders.json"));
        pinned.pin("/home/sam/notes");
        pinned.pin("/home/sam/notes");
        pinned.pin("/home/sam/notes/");
        assert_eq!(pinned.paths(), ["/home/sam/notes"]);
    }

    #[test]
    fn unpinning_forgets_the_folder_and_saves() {
        let dir = temp_dir("unpin");
        let store = dir.join("pinned-folders.json");
        let mut pinned = PinnedFolders::load(store.clone());
        pinned.pin("/home/sam/notes");
        pinned.pin("/home/sam/lectures");
        pinned.unpin("/home/sam/notes");

        assert_eq!(pinned.paths(), ["/home/sam/lectures"]);
        assert_eq!(PinnedFolders::load(store).paths(), ["/home/sam/lectures"]);
    }

    #[test]
    fn a_damaged_list_is_treated_as_empty() {
        let dir = temp_dir("damaged");
        let store = dir.join("pinned-folders.json");
        fs::write(&store, "not json").unwrap();
        assert!(PinnedFolders::load(store).paths().is_empty());
    }

    #[test]
    fn describes_each_folder_with_its_name_and_files() {
        let dir = temp_dir("describe");
        let notes = dir.join("Biology notes");
        fs::create_dir(&notes).unwrap();
        make_file(&notes.join("cells.pdf"), 10);

        let folders = describe_folders(&[text(&notes)]);
        assert_eq!(folders[0].name, "Biology notes");
        assert_eq!(names(&folders[0].files), ["cells.pdf"]);
    }
}
