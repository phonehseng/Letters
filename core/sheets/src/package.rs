//! the zip container both xlsx and ods use, and telling formats apart.
//!
//! reading is defensive: a file might be damaged or built to hurt, so the
//! number of entries and the size each one expands to are capped, and the
//! cap is checked while decompressing, not taken from the zip's own word.

use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::SheetError;

const MAX_ENTRIES: usize = 50_000;
const MAX_PART_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Package {
    pub parts: Vec<Part>,
}

impl Package {
    pub fn read(bytes: &[u8]) -> Result<Package, SheetError> {
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| SheetError::NotASpreadsheet(error.to_string()))?;
        if archive.len() > MAX_ENTRIES {
            return Err(SheetError::TooLarge(format!(
                "{} files inside",
                archive.len()
            )));
        }
        let mut parts = Vec::with_capacity(archive.len());
        let mut total = 0u64;
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|error| SheetError::Damaged(error.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let name = entry
                .name()
                .map_err(|error| SheetError::Damaged(error.to_string()))?
                .trim_start_matches('/')
                .to_string();
            let mut data = Vec::new();
            entry
                .take(MAX_PART_BYTES + 1)
                .read_to_end(&mut data)
                .map_err(|error| SheetError::Damaged(format!("{name}: {error}")))?;
            if data.len() as u64 > MAX_PART_BYTES {
                return Err(SheetError::TooLarge(format!("{name} is over 1 GB")));
            }
            total += data.len() as u64;
            if total > MAX_TOTAL_BYTES {
                return Err(SheetError::TooLarge("over 2 GB unpacked".into()));
            }
            parts.push(Part { name, bytes: data });
        }
        Ok(Package { parts })
    }

    /// a part by name; package part names ignore case.
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
    pub fn text(&self, name: &str) -> Result<Option<String>, SheetError> {
        match self.bytes(name) {
            None => Ok(None),
            Some(bytes) => crate::xml::decode_text(bytes)
                .map(Some)
                .map_err(|detail| SheetError::bad_xml(name, detail)),
        }
    }
}

/// writes parts into a zip. `stored` parts are not compressed (ods's
/// `mimetype`, which also has to come first).
pub fn write_zip(parts: &[Part], stored: &[&str]) -> Result<Vec<u8>, SheetError> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    // a fixed date keeps output identical from run to run
    let deflated = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .large_file(false);
    let plain = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default());
    for part in parts {
        let options = if stored.contains(&part.name.as_str()) {
            plain
        } else if part.bytes.len() as u64 > u32::MAX as u64 / 2 {
            deflated.large_file(true)
        } else {
            deflated
        };
        writer
            .start_file(part.name.as_str(), options)
            .map_err(|error| SheetError::WriteFailed(error.to_string()))?;
        writer
            .write_all(&part.bytes)
            .map_err(|error| SheetError::WriteFailed(error.to_string()))?;
    }
    let cursor = writer
        .finish()
        .map_err(|error| SheetError::WriteFailed(error.to_string()))?;
    Ok(cursor.into_inner())
}

/// the kinds of spreadsheet file this crate reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Xlsx,
    Ods,
    Csv,
}

const OLE_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// works out what a file is from its contents, not its name. anything
/// that is not a zip is taken to be delimited text.
pub fn detect(bytes: &[u8]) -> Result<FileKind, SheetError> {
    if bytes.starts_with(&OLE_MAGIC) {
        let marker: Vec<u8> = "EncryptionInfo"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        return Err(
            if bytes.windows(marker.len()).any(|w| w == marker.as_slice()) {
                SheetError::Encrypted
            } else {
                SheetError::LegacyFormat
            },
        );
    }
    if !bytes.starts_with(b"PK") {
        return Ok(FileKind::Csv);
    }
    let package = Package::read(bytes)?;
    if let Some(mimetype) = package.bytes("mimetype") {
        let mimetype = String::from_utf8_lossy(mimetype);
        return if mimetype
            .trim()
            .starts_with("application/vnd.oasis.opendocument.spreadsheet")
        {
            Ok(FileKind::Ods)
        } else {
            Err(SheetError::NotASpreadsheet(format!(
                "it is {}",
                mimetype.trim()
            )))
        };
    }
    if let Some(types) = package.text("[Content_Types].xml")? {
        if types.contains("sheet.binary") {
            return Err(SheetError::LegacyFormat);
        }
        if types.contains("spreadsheetml") || types.contains("ms-excel") {
            return Ok(FileKind::Xlsx);
        }
        return Err(SheetError::NotASpreadsheet(
            "it is another kind of Office file".into(),
        ));
    }
    if package.get("content.xml").is_some() {
        return Ok(FileKind::Ods);
    }
    Err(SheetError::NotASpreadsheet(
        "it has neither a content type list nor a mimetype".into(),
    ))
}

/// joins a relationship target onto the folder of its source part.
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

/// how `part` is written relative to the folder of `source_part`.
pub fn relative_target(source_part: &str, part: &str) -> String {
    let mut base: Vec<&str> = source_part.split('/').collect();
    base.pop();
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
    fn targets_resolve() {
        assert_eq!(
            resolve_target("xl/workbook.xml", "worksheets/sheet1.xml"),
            "xl/worksheets/sheet1.xml"
        );
        assert_eq!(
            resolve_target("xl/worksheets/sheet1.xml", "../drawings/d.xml"),
            "xl/drawings/d.xml"
        );
        assert_eq!(
            relative_target("xl/worksheets/sheet1.xml", "xl/comments1.xml"),
            "../comments1.xml"
        );
    }

    #[test]
    fn detects_kinds() {
        assert_eq!(detect(b"a,b\n1,2").unwrap(), FileKind::Csv);
        assert_eq!(detect(&OLE_MAGIC), Err(SheetError::LegacyFormat));
        assert!(detect(b"PK\x03\x04broken").is_err());
    }
}
