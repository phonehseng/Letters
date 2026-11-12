//! what Letters remembers about reading and writing each document, kept
//! in one small json file in the app's data folder. the documents
//! themselves are never changed by any of this.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use letters_search::index::path_key;
use serde::{Deserialize, Serialize};

/// documents beyond this many are forgotten, least recently read first,
/// so the file stays small however many documents pass through.
const MAX_DOCUMENTS: usize = 2000;
/// the most days of writing history kept (about three years)
const MAX_DAYS: usize = 1100;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Bookmark {
    pub id: String,
    pub name: String,
    /// counted from 0
    pub page: u32,
    /// for word documents, the block it points at
    pub block: Option<u32>,
    pub created: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DocumentReading {
    /// where the reader stopped: the page (from 0) and how far down it (0 to 1)
    pub last_page: Option<u32>,
    pub last_fraction: f32,
    /// word documents: the block at the top of the screen
    pub last_block: Option<u32>,
    /// milliseconds since 1970
    pub updated: u64,
    pub bookmarks: Vec<Bookmark>,
    /// how long this reader spends on a page of this document, measured
    /// while they read; none until there is enough to go on
    pub seconds_per_page: Option<f32>,
    /// the writer's goal for the whole document, in words
    pub word_goal: Option<u32>,
    /// the writer's goal for one sitting, in words
    pub session_goal: Option<u32>,
}

/// the writer's daily word counts, for the streak.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WritingLog {
    /// "2026-11-12" -> words added that day (all documents together)
    pub days: BTreeMap<String, u32>,
    /// a day counts towards the streak at this many words
    pub daily_goal: u32,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct ReadingFile {
    documents: HashMap<String, DocumentReading>,
    writing: WritingLog,
}

pub struct ReadingStore {
    file: PathBuf,
    data: Mutex<ReadingFile>,
}

pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl ReadingStore {
    pub fn load(file: PathBuf) -> ReadingStore {
        let data = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        ReadingStore {
            file,
            data: Mutex::new(data),
        }
    }

    /// what is remembered about a document (nothing yet is an empty record).
    pub fn document(&self, path: &str) -> DocumentReading {
        let data = self.data.lock().unwrap();
        data.documents
            .get(&path_key(path))
            .cloned()
            .unwrap_or_default()
    }

    fn change_document<T>(&self, path: &str, change: impl FnOnce(&mut DocumentReading) -> T) -> T {
        let mut data = self.data.lock().unwrap();
        let record = data.documents.entry(path_key(path)).or_default();
        let result = change(record);
        record.updated = now_millis();
        forget_oldest(&mut data.documents);
        self.write(&data);
        result
    }

    fn write(&self, data: &ReadingFile) {
        if let Some(folder) = self.file.parent() {
            let _ = fs::create_dir_all(folder);
        }
        if let Ok(bytes) = serde_json::to_vec(data) {
            // written beside and renamed, so a crash never leaves half a file
            let temporary = self.file.with_extension("json.tmp");
            if fs::write(&temporary, bytes).is_ok() {
                let _ = fs::rename(&temporary, &self.file);
            }
        }
    }

    /// a document was moved or renamed: its position, bookmarks, speed and
    /// goals follow it.
    pub fn move_document(&self, old: &str, new: &str) {
        let mut data = self.data.lock().unwrap();
        if let Some(record) = data.documents.remove(&path_key(old)) {
            data.documents.insert(path_key(new), record);
            self.write(&data);
        }
    }

    pub fn set_position(&self, path: &str, page: u32, fraction: f32, block: Option<u32>) {
        self.change_document(path, |record| {
            record.last_page = Some(page);
            record.last_fraction = fraction.clamp(0.0, 1.0);
            record.last_block = block;
        });
    }

    /// "Start from the Beginning": forgets the last position.
    pub fn clear_position(&self, path: &str) {
        self.change_document(path, |record| {
            record.last_page = None;
            record.last_fraction = 0.0;
            record.last_block = None;
        });
    }

    pub fn add_bookmark(&self, path: &str, name: &str, page: u32, block: Option<u32>) -> Bookmark {
        self.change_document(path, |record| {
            let created = now_millis();
            let bookmark = Bookmark {
                id: format!("b{created}-{}", record.bookmarks.len()),
                name: bookmark_name(name, page),
                page,
                block,
                created,
            };
            record.bookmarks.push(bookmark.clone());
            record
                .bookmarks
                .sort_by_key(|b| (b.page, b.block, b.created));
            bookmark
        })
    }

    pub fn rename_bookmark(&self, path: &str, id: &str, name: &str) -> bool {
        self.change_document(path, |record| {
            let Some(bookmark) = record.bookmarks.iter_mut().find(|b| b.id == id) else {
                return false;
            };
            bookmark.name = bookmark_name(name, bookmark.page);
            true
        })
    }

    pub fn remove_bookmark(&self, path: &str, id: &str) -> bool {
        self.change_document(path, |record| {
            let before = record.bookmarks.len();
            record.bookmarks.retain(|b| b.id != id);
            record.bookmarks.len() != before
        })
    }

    /// folds one measurement of time on a page into the reader's speed.
    /// glances (under 3 seconds) and breaks (over 20 minutes) say nothing
    /// about reading speed and are ignored.
    pub fn record_page_time(&self, path: &str, seconds: f32) -> Option<f32> {
        if !(3.0..=1200.0).contains(&seconds) {
            return self.document(path).seconds_per_page;
        }
        self.change_document(path, |record| {
            let speed = match record.seconds_per_page {
                // recent pages count more, so the estimate follows the reader
                Some(previous) => previous * 0.8 + seconds * 0.2,
                None => seconds,
            };
            record.seconds_per_page = Some(speed);
            Some(speed)
        })
    }

    pub fn set_goals(&self, path: &str, word_goal: Option<u32>, session_goal: Option<u32>) {
        self.change_document(path, |record| {
            record.word_goal = word_goal.filter(|g| *g > 0);
            record.session_goal = session_goal.filter(|g| *g > 0);
        });
    }

    pub fn writing_log(&self) -> WritingLog {
        self.data.lock().unwrap().writing.clone()
    }

    /// adds words written on `day` ("2026-11-12"). words removed are not
    /// subtracted: deleting a paragraph does not undo a day's work.
    pub fn record_words(&self, day: &str, words: u32) -> WritingLog {
        let mut data = self.data.lock().unwrap();
        *data.writing.days.entry(day.to_string()).or_default() += words;
        while data.writing.days.len() > MAX_DAYS {
            let oldest = data.writing.days.keys().next().cloned();
            if let Some(oldest) = oldest {
                data.writing.days.remove(&oldest);
            }
        }
        self.write(&data);
        data.writing.clone()
    }

    pub fn set_daily_goal(&self, words: u32) -> WritingLog {
        let mut data = self.data.lock().unwrap();
        data.writing.daily_goal = words;
        self.write(&data);
        data.writing.clone()
    }
}

/// a bookmark always has a name: the one typed, or its page.
fn bookmark_name(name: &str, page: u32) -> String {
    let name = name.trim();
    if name.is_empty() {
        format!("Page {}", page + 1)
    } else {
        name.chars().take(120).collect()
    }
}

fn forget_oldest(documents: &mut HashMap<String, DocumentReading>) {
    if documents.len() <= MAX_DOCUMENTS {
        return;
    }
    let mut by_age: Vec<(u64, String)> = documents
        .iter()
        .map(|(key, record)| (record.updated, key.clone()))
        .collect();
    by_age.sort();
    for (_, key) in by_age.into_iter().take(documents.len() - MAX_DOCUMENTS) {
        documents.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, ReadingStore) {
        let folder = tempfile::tempdir().unwrap();
        let store = ReadingStore::load(folder.path().join("reading.json"));
        (folder, store)
    }

    #[test]
    fn the_last_position_is_kept_between_runs() {
        let (folder, store) = store();
        store.set_position("C:/Books/Novel.pdf", 41, 0.25, None);
        let again = ReadingStore::load(folder.path().join("reading.json"));
        let record = again.document("C:/Books/Novel.pdf");
        assert_eq!(record.last_page, Some(41));
        assert_eq!(record.last_fraction, 0.25);
        again.clear_position("C:/Books/Novel.pdf");
        assert_eq!(again.document("C:/Books/Novel.pdf").last_page, None);
    }

    #[test]
    fn bookmarks_are_named_ordered_renamed_and_removed() {
        let (_folder, store) = store();
        let later = store.add_bookmark("a.pdf", "Results table", 30, None);
        let earlier = store.add_bookmark("a.pdf", "  ", 4, None);
        assert_eq!(earlier.name, "Page 5");
        let names: Vec<String> = store
            .document("a.pdf")
            .bookmarks
            .iter()
            .map(|b| b.name.clone())
            .collect();
        assert_eq!(names, vec!["Page 5", "Results table"]);
        assert!(store.rename_bookmark("a.pdf", &later.id, "Table 2"));
        assert!(store.remove_bookmark("a.pdf", &earlier.id));
        assert_eq!(store.document("a.pdf").bookmarks[0].name, "Table 2");
        assert!(!store.remove_bookmark("a.pdf", "missing"));
    }

    #[test]
    fn reading_speed_follows_the_reader_and_ignores_glances() {
        let (_folder, store) = store();
        assert_eq!(store.record_page_time("a.pdf", 60.0), Some(60.0));
        assert_eq!(store.record_page_time("a.pdf", 1.0), Some(60.0));
        assert_eq!(store.record_page_time("a.pdf", 5000.0), Some(60.0));
        let speed = store.record_page_time("a.pdf", 30.0).unwrap();
        assert!((speed - 54.0).abs() < 0.01);
    }

    #[test]
    fn words_written_add_up_by_day() {
        let (_folder, store) = store();
        store.record_words("2026-11-11", 300);
        store.record_words("2026-11-12", 200);
        let log = store.record_words("2026-11-12", 50);
        assert_eq!(log.days["2026-11-12"], 250);
        assert_eq!(store.set_daily_goal(500).daily_goal, 500);
        store.set_goals("essay.docx", Some(3000), Some(0));
        let record = store.document("essay.docx");
        assert_eq!((record.word_goal, record.session_goal), (Some(3000), None));
    }

    #[test]
    fn old_documents_are_forgotten_first() {
        let mut documents = HashMap::new();
        for i in 0..(MAX_DOCUMENTS + 3) {
            documents.insert(
                format!("d{i}"),
                DocumentReading {
                    updated: i as u64,
                    ..DocumentReading::default()
                },
            );
        }
        forget_oldest(&mut documents);
        assert_eq!(documents.len(), MAX_DOCUMENTS);
        assert!(!documents.contains_key("d0") && documents.contains_key("d3"));
    }
}
