//! downloading a family from the online library, only when the person
//! chooses it. files come from the google/fonts repository at the
//! catalogue's pinned commit, so each one's size and git blob hash are
//! known in advance and checked before the file is used.
//!
//! what leaves the computer: one https request per file, for that file's
//! address and nothing else (no account, no key, no identifier). a
//! download can be cancelled and continues where it stopped next time
//! (an unfinished file is kept as "<name>.part").

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;
use sha1::{Digest, Sha1};

use crate::catalogue::{CatalogueFamily, CatalogueFile};
use crate::error::FontError;

/// where the library's files are read from. raw.githubusercontent.com
/// serves a repository's files exactly as they are at a commit.
pub const LIBRARY_ADDRESS: &str = "https://raw.githubusercontent.com/google/fonts";

/// the only host a download contacts, for the notice the ui shows once.
pub const LIBRARY_HOST: &str = "raw.githubusercontent.com";

/// how a download is getting on, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub done: u64,
    pub total: u64,
}

/// reads files from the library (or, in tests, from a local server).
#[derive(Clone)]
pub struct Downloader {
    base: String,
    agent: ureq::Agent,
}

impl Downloader {
    pub fn new(base: &str) -> Downloader {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(600)))
            .user_agent("Letters")
            .build()
            .into();
        Downloader {
            base: base.trim_end_matches('/').to_string(),
            agent,
        }
    }

    pub fn library() -> Downloader {
        Downloader::new(LIBRARY_ADDRESS)
    }

    /// the address of one file of a family at a commit.
    pub fn url(&self, commit: &str, dir: &str, filename: &str) -> String {
        format!(
            "{}/{commit}/{}/{}",
            self.base,
            encode(dir),
            encode(filename)
        )
    }

    /// downloads every file of a family (fonts and licence) into its own
    /// folder inside `library`, checking each against the catalogue.
    /// returns the family's folder.
    pub fn download_family(
        &self,
        commit: &str,
        family: &CatalogueFamily,
        library: &Path,
        progress: &mut dyn FnMut(DownloadProgress),
        cancel: &AtomicBool,
    ) -> Result<PathBuf, FontError> {
        let folder = family_folder(library, family);
        std::fs::create_dir_all(&folder)
            .map_err(|error| FontError::CannotSave(error.to_string()))?;
        let files: Vec<&CatalogueFile> = family.license_file.iter().chain(&family.files).collect();
        let total = files.iter().map(|file| file.size).sum();
        let mut finished = 0u64;
        for file in files {
            let target = folder.join(safe_file_name(&file.filename)?);
            let mut report = |done: u64| {
                progress(DownloadProgress {
                    done: finished + done,
                    total,
                })
            };
            self.fetch_file(commit, &family.dir, file, &target, &mut report, cancel)?;
            finished += file.size;
        }
        progress(DownloadProgress { done: total, total });
        Ok(folder)
    }

    /// downloads one file, continuing a ".part" left by an earlier try,
    /// and keeps it only when its size and hash are right.
    pub fn fetch_file(
        &self,
        commit: &str,
        dir: &str,
        file: &CatalogueFile,
        target: &Path,
        progress: &mut dyn FnMut(u64),
        cancel: &AtomicBool,
    ) -> Result<(), FontError> {
        if target.exists() && matches_catalogue(target, file) {
            progress(file.size);
            return Ok(());
        }
        let partial = partial_path(target);
        let mut have = std::fs::metadata(&partial)
            .map(|meta| meta.len())
            .unwrap_or(0);
        if have > file.size {
            std::fs::remove_file(&partial).ok();
            have = 0;
        }
        if have < file.size {
            self.receive(
                &self.url(commit, dir, &file.filename),
                &partial,
                have,
                file.size,
                progress,
                cancel,
            )?;
        }
        if !matches_catalogue(&partial, file) {
            std::fs::remove_file(&partial).ok();
            return Err(FontError::HashMismatch(file.filename.clone()));
        }
        std::fs::rename(&partial, target).map_err(|error| FontError::CannotSave(error.to_string()))
    }

    fn receive(
        &self,
        url: &str,
        partial: &Path,
        have: u64,
        size: u64,
        progress: &mut dyn FnMut(u64),
        cancel: &AtomicBool,
    ) -> Result<(), FontError> {
        let failed = |error: &dyn std::fmt::Display| FontError::DownloadFailed(error.to_string());
        let mut request = self.agent.get(url);
        if have > 0 {
            request = request.header("Range", format!("bytes={have}-"));
        }
        let mut response = request.call().map_err(|error| failed(&error))?;
        // a server that ignores the range sends the whole file again
        let resumed = have > 0 && response.status().as_u16() == 206;
        let mut output = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(partial)
            .map_err(|error| FontError::CannotSave(error.to_string()))?;
        let mut done = if resumed { have } else { 0 };
        progress(done);
        let mut reader = response.body_mut().with_config().limit(size + 1).reader();
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                output.flush().ok();
                return Err(FontError::Cancelled);
            }
            let count = reader.read(&mut buffer).map_err(|error| failed(&error))?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| FontError::CannotSave(error.to_string()))?;
            done += count as u64;
            if done > size {
                break;
            }
            progress(done);
        }
        output
            .flush()
            .map_err(|error| FontError::CannotSave(error.to_string()))
    }
}

/// where a family's files go: "ofl/inter" becomes "<library>/ofl-inter".
pub fn family_folder(library: &Path, family: &CatalogueFamily) -> PathBuf {
    library.join(family.dir.replace(['/', '\\'], "-"))
}

fn partial_path(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    target.with_file_name(name)
}

/// a catalogue file name, refused if it tries to leave its folder.
fn safe_file_name(name: &str) -> Result<&str, FontError> {
    let bad = name.is_empty() || name.contains(['/', '\\', ':']) || name.starts_with('.');
    if bad {
        Err(FontError::NotInCatalogue(name.to_string()))
    } else {
        Ok(name)
    }
}

/// percent-encodes a path for a url, keeping the slashes.
fn encode(path: &str) -> String {
    path.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// git's hash of a file: sha-1 of "blob <size>\0" and the bytes.
pub fn git_blob_hash(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {size}\0").as_bytes());
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).ok()?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

fn matches_catalogue(path: &Path, file: &CatalogueFile) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.len() == file.size)
        && git_blob_hash(path).is_some_and(|hash| hash.eq_ignore_ascii_case(&file.sha1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_encoded_and_names_checked() {
        let downloader = Downloader::new("https://example.test/repo/");
        assert_eq!(
            downloader.url("abc", "ofl/inter", "Inter[opsz,wght].ttf"),
            "https://example.test/repo/abc/ofl/inter/Inter%5Bopsz%2Cwght%5D.ttf"
        );
        assert!(safe_file_name("../evil.ttf").is_err());
        assert!(safe_file_name("a/b.ttf").is_err());
        assert!(safe_file_name("Inter.ttf").is_ok());
        assert_eq!(
            partial_path(Path::new("/x/A.ttf")),
            PathBuf::from("/x/A.ttf.part")
        );
    }

    #[test]
    fn git_hashes_match_git() {
        // `git hash-object` of a file holding "hello\n"
        let folder = std::env::temp_dir().join(format!("letters-hash-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("hello.txt");
        std::fs::write(&path, b"hello\n").unwrap();
        assert_eq!(
            git_blob_hash(&path).unwrap(),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
        std::fs::remove_dir_all(&folder).ok();
    }
}
