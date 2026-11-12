//! the zip container pptx and odp use, copied from letters-office's
//! package module without its word-processor format detection.
//!
//! reading is defensive: a file might be damaged or built to hurt, so the
//! number of entries and the size each one expands to are capped, and the
//! cap is checked while decompressing, not taken from the zip's own word.

use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::SlidesError;

/// more entries than any real deck has (a 300-slide deck has about 1,500)
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
    pub fn read(bytes: &[u8]) -> Result<Package, SlidesError> {
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|error| SlidesError::NotAPresentation(error.to_string()))?;
        if archive.len() > MAX_ENTRIES {
            return Err(SlidesError::TooLarge(format!(
                "{} files inside",
                archive.len()
            )));
        }
        let mut parts = Vec::with_capacity(archive.len());
        let mut total = 0u64;
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|error| SlidesError::Damaged(error.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let part = read_entry(entry)?;
            total += part.bytes.len() as u64;
            if total > MAX_TOTAL_BYTES {
                return Err(SlidesError::TooLarge("over 1 GB unpacked".into()));
            }
            parts.push(part);
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
}

fn read_entry<R: Read>(entry: zip::read::ZipFile<'_, R>) -> Result<Part, SlidesError> {
    let name = entry
        .name()
        .map_err(|error| SlidesError::Damaged(error.to_string()))?
        .trim_start_matches('/')
        .to_string();
    let mut data = Vec::new();
    entry
        .take(MAX_PART_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|error| SlidesError::Damaged(format!("{name}: {error}")))?;
    if data.len() as u64 > MAX_PART_BYTES {
        return Err(SlidesError::TooLarge(format!("{name} is over 512 MB")));
    }
    Ok(Part { name, bytes: data })
}

/// writes parts into a zip. `stored` names parts that must not be
/// compressed (odp's `mimetype`, which also has to come first).
pub fn write_zip(parts: &[Part], stored: &[&str]) -> Result<Vec<u8>, SlidesError> {
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
    let failed = |error: &dyn std::fmt::Display| SlidesError::WriteFailed(error.to_string());
    for part in parts {
        let options = if stored.contains(&part.name.as_str()) {
            plain
        } else {
            deflated
        };
        writer
            .start_file(part.name.as_str(), options)
            .map_err(|error| failed(&error))?;
        writer
            .write_all(&part.bytes)
            .map_err(|error| failed(&error))?;
    }
    let cursor = writer.finish().map_err(|error| failed(&error))?;
    Ok(cursor.into_inner())
}

/// joins a relationship target onto the folder of the part it is
/// relative to, resolving `..`. "ppt/slides/slide1.xml" + "../media/a.png"
/// gives "ppt/media/a.png"; a target starting with "/" is absolute.
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
    fn targets_resolve_relative_to_their_part() {
        assert_eq!(
            resolve_target("ppt/slides/slide1.xml", "../media/a.png"),
            "ppt/media/a.png"
        );
        assert_eq!(
            resolve_target("", "ppt/presentation.xml"),
            "ppt/presentation.xml"
        );
        assert_eq!(
            resolve_target("ppt/slides/slide1.xml", "/ppt/x.xml"),
            "ppt/x.xml"
        );
        assert_eq!(
            relative_target("ppt/slides/slide1.xml", "ppt/media/a.png"),
            "../media/a.png"
        );
        assert_eq!(
            relative_target("", "ppt/presentation.xml"),
            "ppt/presentation.xml"
        );
    }

    #[test]
    fn zips_round_trip() {
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
        assert_eq!(Package::read(&bytes).unwrap().parts, parts);
    }

    #[test]
    fn non_zip_bytes_are_refused_clearly() {
        assert!(matches!(
            Package::read(b"hello"),
            Err(SlidesError::NotAPresentation(_))
        ));
    }
}
