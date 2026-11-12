//! the zip container both docx and odt use, and telling formats apart.
//!
//! reading is defensive: a file might be damaged or built to hurt, so the
//! number of entries and the size each one expands to are capped, and the
//! cap is checked while decompressing, not taken from the zip's own word.

use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::OfficeError;
use crate::xml;

const MAX_ENTRIES: usize = 20_000;
const MAX_PART_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;

/// one file inside the package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    /// path inside the zip, without a leading slash
    pub name: String,
    pub bytes: Vec<u8>,
}

/// every part of a package, in the order the zip listed them.
#[derive(Clone, Debug, Default)]
pub struct Package {
    pub parts: Vec<Part>,
}

impl Package {
    /// reads every entry of a zip into memory.
    pub fn read(bytes: &[u8]) -> Result<Package, OfficeError> {
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| OfficeError::NotAPackage(error.to_string()))?;
        if archive.len() > MAX_ENTRIES {
            return Err(OfficeError::TooLarge(format!(
                "{} files inside",
                archive.len()
            )));
        }
        let mut parts = Vec::with_capacity(archive.len());
        let mut total = 0u64;
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|error| OfficeError::Damaged(error.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let name = entry
                .name()
                .map_err(|error| OfficeError::Damaged(error.to_string()))?
                .trim_start_matches('/')
                .to_string();
            let mut data = Vec::new();
            entry
                .take(MAX_PART_BYTES + 1)
                .read_to_end(&mut data)
                .map_err(|error| OfficeError::Damaged(format!("{name}: {error}")))?;
            if data.len() as u64 > MAX_PART_BYTES {
                return Err(OfficeError::TooLarge(format!("{name} is over 512 MB")));
            }
            total += data.len() as u64;
            if total > MAX_TOTAL_BYTES {
                return Err(OfficeError::TooLarge("over 1 GB unpacked".into()));
            }
            parts.push(Part { name, bytes: data });
        }
        Ok(Package { parts })
    }

    /// finds a part by name. package part names ignore case.
    pub fn get(&self, name: &str) -> Option<&Part> {
        let name = name.trim_start_matches('/');
        self.parts
            .iter()
            .find(|part| part.name.eq_ignore_ascii_case(name))
    }

    pub fn bytes(&self, name: &str) -> Option<&[u8]> {
        self.get(name).map(|part| part.bytes.as_slice())
    }

    /// a part's text, decoded.
    pub fn text(&self, name: &str) -> Result<Option<String>, OfficeError> {
        match self.bytes(name) {
            None => Ok(None),
            Some(bytes) => xml::decode_text(bytes)
                .map(Some)
                .map_err(|detail| OfficeError::bad_xml(name, detail)),
        }
    }

    /// a part parsed as xml, or `None` when the part is missing.
    pub fn xml(&self, name: &str) -> Result<Option<xml::XmlDocument>, OfficeError> {
        match self.text(name)? {
            None => Ok(None),
            Some(text) => xml::parse(&text)
                .map(Some)
                .map_err(|detail| OfficeError::bad_xml(name, detail)),
        }
    }
}

/// writes parts into a zip. `stored` names parts that must not be
/// compressed (odt's `mimetype`, which also has to come first).
pub fn write_zip(parts: &[Part], stored: &[&str]) -> Result<Vec<u8>, OfficeError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    // a fixed date keeps output identical from run to run, which keeps
    // tests and diffs honest.
    // a fixed system (dos, as word writes) keeps it identical from one
    // computer to the next: left alone the zip crate records the system
    // it runs on in every entry
    let deflated = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .system(zip::System::Dos);
    let plain = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default())
        .system(zip::System::Dos);
    for part in parts {
        let options = if stored.contains(&part.name.as_str()) {
            plain
        } else {
            deflated
        };
        writer
            .start_file(part.name.as_str(), options)
            .map_err(|error| OfficeError::WriteFailed(error.to_string()))?;
        writer
            .write_all(&part.bytes)
            .map_err(|error| OfficeError::WriteFailed(error.to_string()))?;
    }
    let cursor = writer
        .finish()
        .map_err(|error| OfficeError::WriteFailed(error.to_string()))?;
    Ok(cursor.into_inner())
}

/// the kinds of document this crate opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Docx,
    Odt,
}

/// the start of every ole compound file: old .doc files, and encrypted
/// docx files (office wraps those in one).
const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// works out what a file is from its contents, not its name.
pub fn detect(bytes: &[u8]) -> Result<FileKind, OfficeError> {
    detect_ole(bytes)?;
    let package = Package::read(bytes)?;
    detect_package(&package)
}

/// refuses ole compound files with the right explanation: an encrypted
/// docx or an old binary .doc.
pub(crate) fn detect_ole(bytes: &[u8]) -> Result<(), OfficeError> {
    if !bytes.starts_with(&OLE_MAGIC) {
        return Ok(());
    }
    Err(if contains(bytes, &utf16("EncryptionInfo")) {
        OfficeError::Encrypted
    } else {
        OfficeError::LegacyWordFormat
    })
}

pub(crate) fn detect_package(package: &Package) -> Result<FileKind, OfficeError> {
    if let Some(mimetype) = package.bytes("mimetype") {
        let mimetype = String::from_utf8_lossy(mimetype).trim().to_string();
        return odf_kind(&mimetype);
    }
    if let Some(types) = package.xml("[Content_Types].xml")? {
        return ooxml_kind(&types.root);
    }
    if package.get("content.xml").is_some() {
        // an odt without its mimetype entry; still worth a try
        return Ok(FileKind::Odt);
    }
    Err(OfficeError::NotAPackage(
        "it has neither a content type list nor a mimetype".into(),
    ))
}

fn odf_kind(mimetype: &str) -> Result<FileKind, OfficeError> {
    match mimetype {
        "application/vnd.oasis.opendocument.text"
        | "application/vnd.oasis.opendocument.text-template" => Ok(FileKind::Odt),
        "application/vnd.oasis.opendocument.spreadsheet"
        | "application/vnd.oasis.opendocument.spreadsheet-template" => {
            Err(OfficeError::NotSupportedYet("spreadsheets (.ods)".into()))
        }
        "application/vnd.oasis.opendocument.presentation"
        | "application/vnd.oasis.opendocument.presentation-template" => {
            Err(OfficeError::NotSupportedYet("presentations (.odp)".into()))
        }
        "application/vnd.oasis.opendocument.graphics" => {
            Err(OfficeError::NotSupportedYet("drawings (.odg)".into()))
        }
        other => Err(OfficeError::NotSupportedYet(format!(
            "this kind of OpenDocument file ({other})"
        ))),
    }
}

fn ooxml_kind(types: &xml::Element) -> Result<FileKind, OfficeError> {
    let content_types: Vec<&str> = types
        .children_named("ct:Override")
        .filter_map(|entry| entry.attr("ContentType"))
        .collect();
    let has = |needle: &str| content_types.iter().any(|kind| kind.contains(needle));
    if has("wordprocessingml.document.main")
        || has("wordprocessingml.template.main")
        || has("ms-word.document.macroEnabled.main")
        || has("ms-word.template.macroEnabledTemplate.main")
    {
        Ok(FileKind::Docx)
    } else if has("spreadsheetml") || has("ms-excel") {
        Err(OfficeError::NotSupportedYet("spreadsheets (.xlsx)".into()))
    } else if has("presentationml") || has("ms-powerpoint") {
        Err(OfficeError::NotSupportedYet("presentations (.pptx)".into()))
    } else {
        Err(OfficeError::NotSupportedYet(
            "this kind of Office file".into(),
        ))
    }
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// joins a relationship target onto the folder of the part it is
/// relative to, resolving `..`. "word/document.xml" + "media/a.png"
/// gives "word/media/a.png"; a target starting with "/" is absolute.
pub fn resolve_target(source_part: &str, target: &str) -> String {
    if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_string();
    }
    let mut segments: Vec<&str> = source_part.split('/').collect();
    segments.pop();
    for piece in target.split('/') {
        match piece {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// the opposite of `resolve_target`: how `part` is written relative to
/// the folder of `source_part`.
pub fn relative_target(source_part: &str, part: &str) -> String {
    let base: Vec<&str> = {
        let mut segments: Vec<&str> = source_part.split('/').collect();
        segments.pop();
        segments
    };
    let target: Vec<&str> = part.split('/').collect();
    let shared = base.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut pieces: Vec<String> = vec!["..".to_string(); base.len() - shared];
    pieces.extend(target[shared..].iter().map(|piece| piece.to_string()));
    pieces.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_resolve_relative_to_their_part() {
        assert_eq!(
            resolve_target("word/document.xml", "media/a.png"),
            "word/media/a.png"
        );
        assert_eq!(
            resolve_target("word/document.xml", "../customXml/item1.xml"),
            "customXml/item1.xml"
        );
        assert_eq!(
            resolve_target("_rels/.rels", "/word/document.xml"),
            "word/document.xml"
        );
        assert_eq!(resolve_target("", "word/document.xml"), "word/document.xml");
        assert_eq!(
            relative_target("word/document.xml", "word/media/a.png"),
            "media/a.png"
        );
        assert_eq!(
            relative_target("word/document.xml", "customXml/item1.xml"),
            "../customXml/item1.xml"
        );
    }

    #[test]
    fn non_zip_bytes_are_refused_clearly() {
        assert!(matches!(detect(b"hello"), Err(OfficeError::NotAPackage(_))));
        assert!(matches!(
            detect(&OLE_MAGIC),
            Err(OfficeError::LegacyWordFormat)
        ));
    }

    #[test]
    fn zip_round_trip() {
        let parts = vec![
            Part {
                name: "mimetype".into(),
                bytes: b"x".to_vec(),
            },
            Part {
                name: "a/b.xml".into(),
                bytes: b"<a/>".to_vec(),
            },
        ];
        let bytes = write_zip(&parts, &["mimetype"]).unwrap();
        let package = Package::read(&bytes).unwrap();
        assert_eq!(package.parts, parts);
    }
}
