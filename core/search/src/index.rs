//! the index: adding, replacing and dropping documents, checkpoints and
//! compaction. searching it is in `search.rs`.
//!
//! in memory the index holds the word lists (which passages each word
//! is in) and, for every passage, only where its text sits on disk. the
//! texts themselves stay in the passages file and are read for the few
//! results that are shown.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::model::{DocKind, DocRecord, ExtractedDocument, Fingerprint, Passage};
use crate::normalize::terms;
use crate::postings::PostingList;
use crate::store::{
    folder_size, passages_path, read_snapshot, remove_stale_passage_files, snapshot_path,
    write_snapshot, PassageFile, PassageReader, SnapshotRef, StoredPassage, UnitMeta,
};
use crate::SearchError;

/// words from titles and headings are kept under this mark, apart from
/// words in the text, so "title or heading only" and the heading boost
/// can look them up on their own. it sorts before every letter, so it
/// never shows up when the dictionary is scanned for a prefix.
pub const HEADING_MARK: char = '\u{1}';

/// compaction runs when at least this share of passages belongs to
/// deleted or re-indexed documents (and there are enough to matter).
const COMPACT_DEAD_SHARE: f32 = 0.25;
const COMPACT_MIN_DEAD: usize = 500;

/// how many documents and passages the index holds, and its size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IndexStats {
    pub documents: usize,
    pub passages: usize,
    pub words: usize,
    pub bytes_on_disk: u64,
    /// an estimate of what the index holds in memory
    pub bytes_in_memory: u64,
}

pub struct Index {
    pub(crate) folder: PathBuf,
    pub(crate) generation: u32,
    pub(crate) docs: Vec<DocRecord>,
    /// live documents by their path key
    pub(crate) by_path: HashMap<String, u32>,
    pub(crate) units: Vec<UnitMeta>,
    pub(crate) terms: BTreeMap<String, PostingList>,
    passages: PassageFile,
    dead_units: usize,
    uncommitted: usize,
    /// true when the stored index was damaged and was started afresh
    recovered: bool,
}

/// the key a document's path is kept under (see paths.rs). it stays
/// reachable here because the app's library code imports it from here.
pub use crate::paths::path_key;

impl Index {
    /// opens the index in `folder`, or starts an empty one. a damaged
    /// index is thrown away and started again: it holds nothing that
    /// cannot be rebuilt from the documents.
    pub fn open(folder: &Path) -> Result<Index, SearchError> {
        std::fs::create_dir_all(folder)?;
        match read_snapshot(folder) {
            Ok(Some(snapshot)) => Index::from_snapshot(folder, snapshot),
            Ok(None) => Index::empty(folder, false),
            Err(SearchError::Damaged) => {
                delete_files(folder);
                Index::empty(folder, true)
            }
            Err(error) => Err(error),
        }
    }

    fn empty(folder: &Path, recovered: bool) -> Result<Index, SearchError> {
        remove_stale_passage_files(folder, 0);
        Ok(Index {
            folder: folder.to_path_buf(),
            generation: 0,
            docs: Vec::new(),
            by_path: HashMap::new(),
            units: Vec::new(),
            terms: BTreeMap::new(),
            passages: PassageFile::open(passages_path(folder, 0), 0)?,
            dead_units: 0,
            uncommitted: 0,
            recovered,
        })
    }

    fn from_snapshot(
        folder: &Path,
        snapshot: crate::store::Snapshot,
    ) -> Result<Index, SearchError> {
        remove_stale_passage_files(folder, snapshot.generation);
        let passages = PassageFile::open(
            passages_path(folder, snapshot.generation),
            snapshot.passages_len,
        )?;
        if passages.len() < snapshot.passages_len {
            // the passages file lost data the checkpoint relies on
            delete_files(folder);
            return Index::empty(folder, true);
        }
        let mut by_path = HashMap::new();
        let mut dead_units = 0;
        for (id, doc) in snapshot.docs.iter().enumerate() {
            if doc.live {
                by_path.insert(path_key(&doc.path), id as u32);
            } else {
                dead_units += doc.unit_count as usize;
            }
        }
        Ok(Index {
            folder: folder.to_path_buf(),
            generation: snapshot.generation,
            docs: snapshot.docs,
            by_path,
            units: snapshot.units,
            terms: snapshot.terms.into_iter().collect(),
            passages,
            dead_units,
            uncommitted: 0,
            recovered: false,
        })
    }

    pub fn was_recovered(&self) -> bool {
        self.recovered
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// the stored record of a live document.
    pub fn document(&self, path: &str) -> Option<&DocRecord> {
        let id = *self.by_path.get(&path_key(path))?;
        self.docs.get(id as usize)
    }

    /// every live document, in the order they were indexed.
    pub fn documents(&self) -> impl Iterator<Item = &DocRecord> {
        self.docs.iter().filter(|doc| doc.live)
    }

    /// true when the file is new to the index or changed since it was indexed.
    pub fn needs_indexing(&self, path: &str, fingerprint: &Fingerprint) -> bool {
        self.document(path)
            .is_none_or(|doc| doc.fingerprint != *fingerprint)
    }

    /// records a new fingerprint for a file whose content did not change
    /// (it was touched, or copied with a new time), so it is not read again.
    pub fn refresh_fingerprint(&mut self, path: &str, fingerprint: Fingerprint) -> bool {
        let Some(id) = self.by_path.get(&path_key(path)).copied() else {
            return false;
        };
        self.docs[id as usize].fingerprint = fingerprint;
        self.uncommitted += 1;
        true
    }

    /// a file was moved or renamed: its entry follows it without the file
    /// being read again. false when the old path was not indexed.
    pub fn rename_document(&mut self, old: &str, new: &str) -> bool {
        let Some(id) = self.by_path.remove(&path_key(old)) else {
            return false;
        };
        // a document already at the new path is replaced by the moved one
        self.remove_document(new);
        let doc = &mut self.docs[id as usize];
        doc.path = new.to_string();
        if let Some(stem) = Path::new(new).file_stem() {
            let old_title = crate::model::title_from_path(Path::new(old));
            if doc.title == old_title {
                doc.title = stem.to_string_lossy().into_owned();
            }
        }
        self.by_path.insert(path_key(new), id);
        self.uncommitted += 1;
        true
    }

    /// adds a document, replacing an older version of the same file.
    pub fn add_document(
        &mut self,
        path: &str,
        kind: DocKind,
        fingerprint: Fingerprint,
        extracted: ExtractedDocument,
    ) -> Result<(), SearchError> {
        self.remove_document(path);
        let doc_id = self.docs.len() as u32;
        let first_unit = self.units.len() as u32;
        let title_terms: Vec<String> = terms(&extracted.title);
        let mut passages = extracted.passages;
        if passages.is_empty() {
            // a locked or empty file is still found by its name
            passages.push(Passage::default());
        }
        for passage in &passages {
            self.add_passage(doc_id, passage, &title_terms)?;
        }
        self.passages.flush()?;
        self.docs.push(DocRecord {
            path: path.to_string(),
            kind,
            title: extracted.title,
            fingerprint,
            pages: extracted.pages,
            first_unit,
            unit_count: passages.len() as u32,
            notes: extracted.notes,
            live: true,
        });
        self.by_path.insert(path_key(path), doc_id);
        self.uncommitted += 1;
        Ok(())
    }

    fn add_passage(
        &mut self,
        doc: u32,
        passage: &Passage,
        title_terms: &[String],
    ) -> Result<(), SearchError> {
        let unit = self.units.len() as u32;
        let stored = StoredPassage {
            number: passage.number,
            page: passage.page,
            heading: passage.heading.clone(),
            text: passage.text.clone(),
        };
        let (offset, length) = self.passages.append(&stored)?;
        let mut counts: HashMap<String, u32> = HashMap::new();
        let words = terms(&passage.text);
        for word in &words {
            *counts.entry(word.clone()).or_default() += 1;
        }
        for word in title_terms.iter().chain(terms(&passage.heading).iter()) {
            *counts.entry(format!("{HEADING_MARK}{word}")).or_default() += 1;
        }
        for (term, frequency) in counts {
            self.terms.entry(term).or_default().push(unit, frequency);
        }
        self.units.push(UnitMeta {
            doc,
            offset,
            length,
            words: words.len() as u32,
        });
        Ok(())
    }

    /// drops a document from the results at once. its passages stay on
    /// disk until the next compaction. true when it was in the index.
    pub fn remove_document(&mut self, path: &str) -> bool {
        let Some(id) = self.by_path.remove(&path_key(path)) else {
            return false;
        };
        let doc = &mut self.docs[id as usize];
        doc.live = false;
        self.dead_units += doc.unit_count as usize;
        self.uncommitted += 1;
        true
    }

    /// drops every document `keep` says no to (deleted files, folders
    /// taken out of the library) and returns their paths.
    pub fn retain(&mut self, mut keep: impl FnMut(&DocRecord) -> bool) -> Vec<String> {
        let gone: Vec<String> = self
            .documents()
            .filter(|doc| !keep(doc))
            .map(|doc| doc.path.clone())
            .collect();
        for path in &gone {
            self.remove_document(path);
        }
        gone
    }

    /// changes since the last checkpoint.
    pub fn uncommitted_changes(&self) -> usize {
        self.uncommitted
    }

    /// writes a checkpoint, compacting first when many passages are dead.
    pub fn commit(&mut self) -> Result<(), SearchError> {
        if self.should_compact() {
            return self.compact();
        }
        self.write_checkpoint()
    }

    fn write_checkpoint(&mut self) -> Result<(), SearchError> {
        self.passages.flush()?;
        let snapshot = SnapshotRef {
            generation: self.generation,
            passages_len: self.passages.len(),
            docs: &self.docs,
            units: &self.units,
            terms: self.terms.iter().collect(),
        };
        write_snapshot(&self.folder, &snapshot)?;
        self.uncommitted = 0;
        Ok(())
    }

    fn should_compact(&self) -> bool {
        self.dead_units >= COMPACT_MIN_DEAD
            && self.dead_units as f32 >= self.units.len() as f32 * COMPACT_DEAD_SHARE
    }

    /// rewrites the index without the passages of dropped documents.
    pub fn compact(&mut self) -> Result<(), SearchError> {
        let generation = self.generation + 1;
        let new_path = passages_path(&self.folder, generation);
        let _ = std::fs::remove_file(&new_path);
        let mut new_passages = PassageFile::open(new_path, 0)?;
        let mut reader = PassageReader::open(self.passages.path())?;
        let mut unit_map: Vec<Option<u32>> = vec![None; self.units.len()];
        let mut new_units = Vec::new();
        let mut new_docs = Vec::new();
        for doc in self.docs.iter().filter(|doc| doc.live) {
            let new_doc = new_docs.len() as u32;
            let first_unit = new_units.len() as u32;
            for old in doc.first_unit..doc.first_unit + doc.unit_count {
                let meta = self.units[old as usize];
                let passage = reader.read(&meta)?;
                let (offset, length) = new_passages.append(&passage)?;
                unit_map[old as usize] = Some(new_units.len() as u32);
                new_units.push(UnitMeta {
                    doc: new_doc,
                    offset,
                    length,
                    words: meta.words,
                });
            }
            let mut record = doc.clone();
            record.first_unit = first_unit;
            new_docs.push(record);
        }
        new_passages.flush()?;
        // the old file must be closed before windows lets it be deleted
        drop(reader);
        let mut new_terms = BTreeMap::new();
        for (term, list) in &self.terms {
            let kept = list.filtered(|unit| unit_map[unit as usize]);
            if !kept.is_empty() {
                new_terms.insert(term.clone(), kept);
            }
        }
        self.generation = generation;
        self.passages = new_passages;
        self.units = new_units;
        self.terms = new_terms;
        self.by_path = new_docs
            .iter()
            .enumerate()
            .map(|(id, doc)| (path_key(&doc.path), id as u32))
            .collect();
        self.docs = new_docs;
        self.dead_units = 0;
        self.write_checkpoint()?;
        remove_stale_passage_files(&self.folder, self.generation);
        Ok(())
    }

    pub fn stats(&self) -> IndexStats {
        let live_documents = self.by_path.len();
        let live_passages = self.units.len() - self.dead_units;
        let postings: usize = self.terms.values().map(PostingList::byte_size).sum();
        let keys: usize = self.terms.keys().map(String::len).sum();
        // each dictionary entry also costs its tree node, string and list headers
        let per_entry = 80;
        let memory = postings
            + keys
            + self.terms.len() * per_entry
            + self.units.len() * std::mem::size_of::<UnitMeta>()
            + self.docs.len() * 200;
        IndexStats {
            documents: live_documents,
            passages: live_passages,
            words: self.terms.len(),
            bytes_on_disk: folder_size(&self.folder),
            bytes_in_memory: memory as u64,
        }
    }

    pub(crate) fn passages_file(&self) -> &Path {
        self.passages.path()
    }

    /// forgets everything and starts an empty index in the same folder.
    pub fn clear(&mut self) -> Result<(), SearchError> {
        let folder = self.folder.clone();
        // the passages file must be closed before it can be deleted on windows
        self.passages = PassageFile::open(folder.join("closing.tmp"), 0)?;
        delete_files(&folder);
        *self = Index::empty(&folder, false)?;
        Ok(())
    }
}

/// deletes the index's files from `folder`, leaving the folder.
pub fn delete_files(folder: &Path) {
    let _ = std::fs::remove_file(snapshot_path(folder));
    let _ = std::fs::remove_file(folder.join("index.bin.tmp"));
    let _ = std::fs::remove_file(folder.join("closing.tmp"));
    remove_stale_passage_files(folder, u32::MAX);
}
