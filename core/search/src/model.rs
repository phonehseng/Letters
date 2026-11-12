//! the shapes the index works with: documents, their passages and
//! what is known about each file.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// the kinds of file the index reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocKind {
    Pdf,
    /// .docx, .odt and .rtf, read through letters-office
    Word,
    /// .txt and .md
    Text,
}

impl DocKind {
    /// the kind of a file from its extension, or none for files the
    /// index does not read.
    pub fn of_path(path: &Path) -> Option<DocKind> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        match extension.as_str() {
            "pdf" => Some(DocKind::Pdf),
            "docx" | "odt" | "rtf" => Some(DocKind::Word),
            "txt" | "md" | "markdown" | "text" => Some(DocKind::Text),
            _ => None,
        }
    }
}

/// what tells a changed file from an unchanged one without reading it
/// all: its size, when it was last changed, and a checksum of its first
/// and last bytes (see `fingerprint.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub size: u64,
    /// milliseconds since 1970
    pub modified: u64,
    pub hash: u32,
}

/// something the person should know about one file in the index.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DocNote {
    /// pages with no text layer: a scan. text recognition would make them searchable
    NeedsTextRecognition { pages: u32 },
    /// the file needs a password, so only its name is searchable
    Locked,
    /// the file could not be read at all
    Unreadable { reason: String },
}

/// one page of a pdf, or one paragraph (or table) of a word document:
/// the thing a search result points at.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Passage {
    /// the page (counted from 0) for a pdf, the block index for a word
    /// document (what the writer scrolls to), the paragraph for plain text
    pub number: u32,
    /// the page it is on, counted from 0. estimated for word documents
    pub page: u32,
    /// the headings it sits under, outermost first ("Part I › Methods")
    pub heading: String,
    pub text: String,
}

/// what an extractor read from one file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtractedDocument {
    /// shown as the result's title and searched as one
    pub title: String,
    /// pages for a pdf, estimated pages for the rest
    pub pages: u32,
    pub passages: Vec<Passage>,
    pub notes: Vec<DocNote>,
}

/// one document as the index remembers it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocRecord {
    pub path: String,
    pub kind: DocKind,
    pub title: String,
    pub fingerprint: Fingerprint,
    pub pages: u32,
    pub first_unit: u32,
    pub unit_count: u32,
    #[serde(default)]
    pub notes: Vec<DocNote>,
    /// false once the file was deleted or re-indexed; its passages wait
    /// for the next compaction to be dropped
    pub live: bool,
}

/// the headings of a passage written as one line, outermost first.
pub fn heading_path(headings: &[String]) -> String {
    headings.join(" \u{203A} ")
}

/// the file name without its extension, the title every document has.
pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_come_from_the_extension() {
        assert_eq!(DocKind::of_path(Path::new("a/B.PDF")), Some(DocKind::Pdf));
        assert_eq!(
            DocKind::of_path(Path::new("essay.docx")),
            Some(DocKind::Word)
        );
        assert_eq!(DocKind::of_path(Path::new("notes.md")), Some(DocKind::Text));
        assert_eq!(DocKind::of_path(Path::new("photo.jpg")), None);
        assert_eq!(DocKind::of_path(Path::new("README")), None);
    }

    #[test]
    fn titles_and_heading_paths() {
        assert_eq!(
            title_from_path(Path::new("C:/x/Lab Report.pdf")),
            "Lab Report"
        );
        assert_eq!(
            heading_path(&["Part I".into(), "Methods".into()]),
            "Part I \u{203A} Methods"
        );
    }
}
