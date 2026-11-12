//! the index's files on disk.
//!
//! two files live in the index folder:
//!
//! - `passages-N.bin`: the text of every passage, appended as documents
//!   are indexed. results read their snippets from here, so passage text
//!   never has to sit in memory.
//! - `index.bin`: everything else (the documents, where each passage's
//!   text is, and the word lists), written whole at each checkpoint to a
//!   temporary file that is then renamed over the old one. a rename is
//!   all or nothing, so a crash leaves either the old checkpoint or the
//!   new one, never half of each. passages appended after the last
//!   checkpoint are cut off on the next start and their documents are
//!   simply indexed again.
//!
//! N is a generation number that changes when the index is compacted,
//! so the old passages file stays valid until the new checkpoint names
//! the new one.

use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::model::DocRecord;
use crate::postings::{read_varint, write_varint, PostingList};
use crate::SearchError;

const MAGIC: &[u8; 8] = b"LTRSIDX1";
const SNAPSHOT_NAME: &str = "index.bin";

/// where one passage is, and enough about it to rank it without reading it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitMeta {
    pub doc: u32,
    pub offset: u64,
    pub length: u32,
    /// how many words it holds, for ranking short and long passages fairly
    pub words: u32,
}

/// one passage as read back from the passages file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoredPassage {
    pub number: u32,
    pub page: u32,
    pub heading: String,
    pub text: String,
}

/// everything a checkpoint holds.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub generation: u32,
    /// how far the passages file is committed
    pub passages_len: u64,
    pub docs: Vec<DocRecord>,
    pub units: Vec<UnitMeta>,
    pub terms: Vec<(String, PostingList)>,
}

pub fn passages_path(folder: &Path, generation: u32) -> PathBuf {
    folder.join(format!("passages-{generation}.bin"))
}

pub fn snapshot_path(folder: &Path) -> PathBuf {
    folder.join(SNAPSHOT_NAME)
}

/// the append-only file of passage texts.
pub struct PassageFile {
    path: PathBuf,
    writer: File,
    len: u64,
}

impl PassageFile {
    /// opens the file, cutting off anything after `committed_len`.
    pub fn open(path: PathBuf, committed_len: u64) -> Result<PassageFile, SearchError> {
        let writer = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        let actual = writer.metadata()?.len();
        let len = committed_len.min(actual);
        if actual != len {
            writer.set_len(len)?;
        }
        Ok(PassageFile { path, writer, len })
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// appends one passage and returns where it went.
    pub fn append(&mut self, passage: &StoredPassage) -> Result<(u64, u32), SearchError> {
        let mut record = Vec::with_capacity(passage.text.len() + passage.heading.len() + 16);
        write_varint(&mut record, u64::from(passage.number));
        write_varint(&mut record, u64::from(passage.page));
        write_string(&mut record, &passage.heading);
        write_string(&mut record, &passage.text);
        self.writer.seek(SeekFrom::Start(self.len))?;
        self.writer.write_all(&record)?;
        let offset = self.len;
        self.len += record.len() as u64;
        Ok((offset, record.len() as u32))
    }

    pub fn flush(&mut self) -> Result<(), SearchError> {
        self.writer.flush()?;
        Ok(())
    }
}

/// a reader for passages, opened once per search.
pub struct PassageReader {
    file: File,
}

impl PassageReader {
    pub fn open(path: &Path) -> Result<PassageReader, SearchError> {
        Ok(PassageReader {
            file: File::open(path)?,
        })
    }

    pub fn read(&mut self, unit: &UnitMeta) -> Result<StoredPassage, SearchError> {
        let mut record = vec![0u8; unit.length as usize];
        self.file.seek(SeekFrom::Start(unit.offset))?;
        self.file.read_exact(&mut record)?;
        decode_passage(&record).ok_or(SearchError::Damaged)
    }
}

fn decode_passage(record: &[u8]) -> Option<StoredPassage> {
    let mut at = 0;
    let number = read_varint(record, &mut at)? as u32;
    let page = read_varint(record, &mut at)? as u32;
    let heading = read_string(record, &mut at)?;
    let text = read_string(record, &mut at)?;
    Some(StoredPassage {
        number,
        page,
        heading,
        text,
    })
}

fn write_string(out: &mut Vec<u8>, text: &str) {
    write_varint(out, text.len() as u64);
    out.extend_from_slice(text.as_bytes());
}

fn read_string(bytes: &[u8], at: &mut usize) -> Option<String> {
    let length = read_varint(bytes, at)? as usize;
    let end = at.checked_add(length).filter(|end| *end <= bytes.len())?;
    let text = std::str::from_utf8(&bytes[*at..end]).ok()?.to_string();
    *at = end;
    Some(text)
}

/// what a checkpoint writes, borrowed from the live index so the word
/// lists are never copied just to be saved.
pub struct SnapshotRef<'a> {
    pub generation: u32,
    pub passages_len: u64,
    pub docs: &'a [DocRecord],
    pub units: &'a [UnitMeta],
    pub terms: Vec<(&'a String, &'a PostingList)>,
}

/// writes a checkpoint: to a temporary file first, then renamed into place.
pub fn write_snapshot(folder: &Path, snapshot: &SnapshotRef) -> Result<u64, SearchError> {
    fs::create_dir_all(folder)?;
    let temporary = folder.join("index.bin.tmp");
    {
        let mut out = BufWriter::new(File::create(&temporary)?);
        let mut bytes = Vec::with_capacity(1 << 20);
        bytes.extend_from_slice(MAGIC);
        write_varint(&mut bytes, u64::from(snapshot.generation));
        write_varint(&mut bytes, snapshot.passages_len);
        let catalog = serde_json::to_vec(snapshot.docs).map_err(|_| SearchError::Damaged)?;
        write_varint(&mut bytes, catalog.len() as u64);
        bytes.extend_from_slice(&catalog);
        encode_units(snapshot.units, &mut bytes);
        out.write_all(&bytes)?;
        bytes.clear();
        write_varint(&mut bytes, snapshot.terms.len() as u64);
        for (term, list) in &snapshot.terms {
            write_string(&mut bytes, term);
            list.encode(&mut bytes);
            // written in pieces so the whole dictionary is never copied twice
            if bytes.len() > 1 << 20 {
                out.write_all(&bytes)?;
                bytes.clear();
            }
        }
        out.write_all(&bytes)?;
        out.flush()?;
        out.get_ref().sync_all()?;
    }
    let size = fs::metadata(&temporary)?.len();
    fs::rename(&temporary, snapshot_path(folder))?;
    Ok(size)
}

fn encode_units(units: &[UnitMeta], out: &mut Vec<u8>) {
    write_varint(out, units.len() as u64);
    for unit in units {
        write_varint(out, u64::from(unit.doc));
        write_varint(out, unit.offset);
        write_varint(out, u64::from(unit.length));
        write_varint(out, u64::from(unit.words));
    }
}

fn decode_units(bytes: &[u8], at: &mut usize) -> Option<Vec<UnitMeta>> {
    let count = read_varint(bytes, at)? as usize;
    // a damaged count must not make us reserve gigabytes
    let mut units = Vec::with_capacity(count.min(bytes.len()));
    for _ in 0..count {
        units.push(UnitMeta {
            doc: read_varint(bytes, at)? as u32,
            offset: read_varint(bytes, at)?,
            length: read_varint(bytes, at)? as u32,
            words: read_varint(bytes, at)? as u32,
        });
    }
    Some(units)
}

/// reads the checkpoint. none when there is none yet; an error when it is damaged.
pub fn read_snapshot(folder: &Path) -> Result<Option<Snapshot>, SearchError> {
    let path = snapshot_path(folder);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    decode_snapshot(&bytes)
        .map(Some)
        .ok_or(SearchError::Damaged)
}

fn decode_snapshot(bytes: &[u8]) -> Option<Snapshot> {
    if bytes.len() < MAGIC.len() || &bytes[..MAGIC.len()] != MAGIC {
        return None;
    }
    let mut at = MAGIC.len();
    let generation = read_varint(bytes, &mut at)? as u32;
    let passages_len = read_varint(bytes, &mut at)?;
    let catalog_len = read_varint(bytes, &mut at)? as usize;
    let catalog_end = at
        .checked_add(catalog_len)
        .filter(|end| *end <= bytes.len())?;
    let docs: Vec<DocRecord> = serde_json::from_slice(&bytes[at..catalog_end]).ok()?;
    at = catalog_end;
    let units = decode_units(bytes, &mut at)?;
    let term_count = read_varint(bytes, &mut at)? as usize;
    let mut terms = Vec::with_capacity(term_count.min(bytes.len()));
    for _ in 0..term_count {
        let term = read_string(bytes, &mut at)?;
        let list = PostingList::decode(bytes, &mut at)?;
        terms.push((term, list));
    }
    Some(Snapshot {
        generation,
        passages_len,
        docs,
        units,
        terms,
    })
}

/// removes passages files of other generations, left behind by a
/// compaction that was interrupted or finished.
pub fn remove_stale_passage_files(folder: &Path, keep: u32) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let keep_name = format!("passages-{keep}.bin");
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("passages-") && name.ends_with(".bin") && name != keep_name {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// bytes the index uses on disk.
pub fn folder_size(folder: &Path) -> u64 {
    fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                // the directory listing's sizes can be stale for a file that
                // is still open on windows, so each file is asked directly
                .filter_map(|entry| fs::metadata(entry.path()).ok())
                .filter(|metadata| metadata.is_file())
                .map(|metadata| metadata.len())
                .sum()
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DocKind, Fingerprint};

    fn passage(text: &str) -> StoredPassage {
        StoredPassage {
            number: 3,
            page: 2,
            heading: "Methods".into(),
            text: text.into(),
        }
    }

    #[test]
    fn passages_round_trip_and_uncommitted_ones_are_cut_off() {
        let folder = tempfile::tempdir().unwrap();
        let path = passages_path(folder.path(), 0);
        let mut file = PassageFile::open(path.clone(), 0).unwrap();
        let (offset, length) = file.append(&passage("first")).unwrap();
        let committed = file.len();
        file.append(&passage("second, never committed")).unwrap();
        file.flush().unwrap();
        drop(file);
        let file = PassageFile::open(path.clone(), committed).unwrap();
        assert_eq!(file.len(), committed);
        let mut reader = PassageReader::open(&path).unwrap();
        let unit = UnitMeta {
            doc: 0,
            offset,
            length,
            words: 1,
        };
        assert_eq!(reader.read(&unit).unwrap(), passage("first"));
    }

    #[test]
    fn snapshots_round_trip_and_damage_is_reported() {
        let folder = tempfile::tempdir().unwrap();
        assert!(read_snapshot(folder.path()).unwrap().is_none());
        let mut list = PostingList::default();
        list.push(0, 2);
        let term = String::from("cell");
        let docs = vec![DocRecord {
            path: "a.pdf".into(),
            kind: DocKind::Pdf,
            title: "a".into(),
            fingerprint: Fingerprint::default(),
            pages: 1,
            first_unit: 0,
            unit_count: 1,
            notes: vec![],
            live: true,
        }];
        let units = vec![UnitMeta {
            doc: 0,
            offset: 0,
            length: 10,
            words: 2,
        }];
        let snapshot = SnapshotRef {
            generation: 4,
            passages_len: 99,
            docs: &docs,
            units: &units,
            terms: vec![(&term, &list)],
        };
        write_snapshot(folder.path(), &snapshot).unwrap();
        let back = read_snapshot(folder.path()).unwrap().unwrap();
        assert_eq!(back.generation, 4);
        assert_eq!(back.passages_len, 99);
        assert_eq!(back.docs, docs);
        assert_eq!(back.units, units);
        assert_eq!(back.terms, vec![(term.clone(), list.clone())]);
        fs::write(snapshot_path(folder.path()), b"LTRSIDX1garbage").unwrap();
        assert!(read_snapshot(folder.path()).is_err());
    }
}
