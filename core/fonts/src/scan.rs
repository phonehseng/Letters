//! finding font files in folders and reading their facts, with a cache
//! so later starts don't read a thousand files again.
//!
//! a file's cached facts are used while its path, size and modification
//! time are unchanged. files that turned out not to be fonts are cached
//! too (with no faces), so a broken file is not read at every start.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::container::{has_font_extension, read_limited, to_plain_font, MAX_SYSTEM_FONT_BYTES};
use crate::facts::read_facts;
use crate::model::{FaceFacts, FaceRecord, Source};
use crate::sfnt::face_count;

/// a font file as found on disk, before it is read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundFile {
    pub path: PathBuf,
    pub size: u64,
    /// nanoseconds since 1970, 0 when the system can't say
    pub modified: u64,
}

/// what the cache remembers about one file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedFile {
    pub size: u64,
    pub modified: u64,
    /// each face's index in the file and its facts; empty when the file
    /// is not a usable font
    pub faces: Vec<(u32, FaceFacts)>,
}

/// the facts of every file read before, by path.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanCache {
    /// bumped when the facts read change shape, so old caches are ignored
    pub version: u32,
    pub files: HashMap<PathBuf, CachedFile>,
}

/// change this when FaceFacts gains or changes a field.
pub const CACHE_VERSION: u32 = 3;

impl ScanCache {
    /// the cache in `path`, or an empty one when it is missing, damaged
    /// or from an older version of Letters.
    pub fn load(path: &Path) -> ScanCache {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ScanCache>(&bytes).ok())
            .filter(|cache| cache.version == CACHE_VERSION)
            .unwrap_or_else(|| ScanCache {
                version: CACHE_VERSION,
                files: HashMap::new(),
            })
    }

    /// writes the cache through a temporary file, so a crash never leaves
    /// half a cache (a damaged one would only be read again from scratch).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let temporary = path.with_extension("saving");
        std::fs::write(
            &temporary,
            serde_json::to_vec(self).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(&temporary, path)
    }

    /// the cached facts of a file, if it hasn't changed since.
    pub fn fresh(&self, file: &FoundFile) -> Option<&CachedFile> {
        self.files
            .get(&file.path)
            .filter(|cached| cached.size == file.size && cached.modified == file.modified)
    }

    /// forgets files that are gone, so the cache doesn't grow forever.
    pub fn keep_only(&mut self, files: &[FoundFile]) {
        let present: std::collections::HashSet<&PathBuf> =
            files.iter().map(|file| &file.path).collect();
        self.files.retain(|path, _| present.contains(path));
    }
}

/// the folders operating systems install fonts into, for everyone and
/// for this person, on windows, macos and linux.
pub fn system_font_folders() -> Vec<PathBuf> {
    let mut folders = Vec::new();
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from);
    if let Some(windows) = std::env::var_os("WINDIR") {
        folders.push(PathBuf::from(windows).join("Fonts"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        // fonts installed "for me only" on windows 10 and later
        folders.push(
            PathBuf::from(local)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts"),
        );
    }
    for folder in [
        "/System/Library/Fonts",
        "/Library/Fonts",
        "/usr/share/fonts",
        "/usr/local/share/fonts",
    ] {
        folders.push(PathBuf::from(folder));
    }
    if let Some(home) = &home {
        folders.push(home.join("Library").join("Fonts"));
        folders.push(home.join(".fonts"));
        folders.push(home.join(".local").join("share").join("fonts"));
    }
    folders.extend(fontconfig_folders(
        Path::new("/etc/fonts/fonts.conf"),
        home.as_deref(),
    ));
    let mut unique = Vec::new();
    for folder in folders {
        if !unique.contains(&folder) {
            unique.push(folder);
        }
    }
    unique
}

/// the <dir> entries of a fontconfig file. only the simple form is read;
/// linux systems put their fonts in the usual folders anyway.
fn fontconfig_folders(config: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(config) else {
        return Vec::new();
    };
    text.split("<dir")
        .skip(1)
        .filter_map(|part| {
            let inner = part.split_once('>')?.1.split_once("</dir>")?.0.trim();
            match inner.strip_prefix("~/") {
                Some(rest) => home.map(|home| home.join(rest)),
                None if inner.starts_with('/') => Some(PathBuf::from(inner)),
                None => None,
            }
        })
        .collect()
}

/// every font file in the folders, a few levels deep (linux nests them).
/// listing is quick: only names and sizes are read.
pub fn find_font_files(folders: &[PathBuf]) -> Vec<FoundFile> {
    let mut files = Vec::new();
    for folder in folders {
        collect(folder, 0, &mut files);
    }
    files
}

/// stops a folder full of links from going on forever.
const MAX_DEPTH: usize = 6;
const MAX_FILES: usize = 20_000;

fn collect(folder: &Path, depth: usize, files: &mut Vec<FoundFile>) {
    if depth > MAX_DEPTH || files.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            collect(&path, depth + 1, files);
        } else if metadata.is_file() && has_font_extension(&path) && !is_partial(&path) {
            files.push(FoundFile {
                modified: metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                    .map(|since| since.as_nanos() as u64)
                    .unwrap_or(0),
                size: metadata.len(),
                path,
            });
        }
    }
}

/// a download that hasn't finished (see download.rs).
fn is_partial(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".part")
}

/// reads every face of one file; an empty list when it is not a font.
pub fn read_file_faces(path: &Path, limit: u64) -> Vec<(u32, FaceFacts)> {
    let name = crate::container::display_name(path);
    let Ok(plain) = read_limited(path, limit).and_then(|bytes| to_plain_font(bytes, &name)) else {
        return Vec::new();
    };
    (0..face_count(&plain))
        .filter_map(|index| read_facts(&plain, index).map(|facts| (index, facts)))
        .collect()
}

/// how far a scan has got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub done: usize,
    pub total: usize,
    pub finished: bool,
}

/// reads the facts of each file (from the cache where it is fresh) and
/// hands them on in batches, so the library fills in while the scan goes.
/// `on_batch` returns false to stop early (the app is closing).
pub fn scan(
    files: &[FoundFile],
    source: Source,
    cache: &mut ScanCache,
    mut on_batch: impl FnMut(Vec<FaceRecord>, ScanProgress) -> bool,
) {
    const BATCH: usize = 40;
    let total = files.len();
    let mut batch = Vec::new();
    for (done, file) in files.iter().enumerate() {
        let faces = match cache.fresh(file) {
            Some(cached) => cached.faces.clone(),
            None => {
                let faces = read_file_faces(&file.path, MAX_SYSTEM_FONT_BYTES);
                cache.files.insert(
                    file.path.clone(),
                    CachedFile {
                        size: file.size,
                        modified: file.modified,
                        faces: faces.clone(),
                    },
                );
                faces
            }
        };
        let license = license_beside(&file.path);
        batch.extend(faces.into_iter().map(|(index, facts)| FaceRecord {
            key: FaceRecord::make_key(source, &file.path, index),
            source,
            path: file.path.clone(),
            index,
            facts,
            license: license.clone(),
        }));
        let progress = ScanProgress {
            done: done + 1,
            total,
            finished: done + 1 == total,
        };
        let batch_full = (done + 1) % BATCH == 0 || progress.finished;
        if batch_full && !on_batch(std::mem::take(&mut batch), progress) {
            return;
        }
    }
    if total == 0 {
        on_batch(
            Vec::new(),
            ScanProgress {
                done: 0,
                total: 0,
                finished: true,
            },
        );
    }
}

/// the licence named by a licence file in the font's folder, for fonts
/// Letters shipped or downloaded (system fonts don't come with one).
pub fn license_beside(font: &Path) -> Option<String> {
    let folder = font.parent()?;
    LICENSE_FILES
        .iter()
        .find(|(file, _)| folder.join(file).is_file())
        .map(|(_, name)| name.to_string())
}

/// the licence files open font projects ship, and the licence each names.
pub const LICENSE_FILES: [(&str, &str); 3] = [
    ("OFL.txt", "SIL Open Font License 1.1"),
    ("LICENSE.txt", "Apache License 2.0"),
    ("UFL.txt", "Ubuntu Font Licence 1.0"),
];

/// the licence's full name from the online library's short one.
pub fn license_name(short: &str) -> &'static str {
    match short.to_ascii_uppercase().as_str() {
        "OFL" => LICENSE_FILES[0].1,
        "APACHE2" | "APACHE" => LICENSE_FILES[1].1,
        "UFL" => LICENSE_FILES[2].1,
        _ => "Open license",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fontconfig_dirs_are_read() {
        let folder =
            std::env::temp_dir().join(format!("letters-fontconfig-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let config = folder.join("fonts.conf");
        std::fs::write(
            &config,
            "<fontconfig><dir>/opt/fonts</dir><dir prefix=\"xdg\">fonts</dir><dir>~/my fonts</dir></fontconfig>",
        )
        .unwrap();
        let found = fontconfig_folders(&config, Some(Path::new("/home/sam")));
        assert_eq!(
            found,
            vec![
                PathBuf::from("/opt/fonts"),
                PathBuf::from("/home/sam/my fonts")
            ]
        );
        std::fs::remove_dir_all(&folder).ok();
    }

    #[test]
    fn licence_names() {
        assert_eq!(license_name("OFL"), "SIL Open Font License 1.1");
        assert_eq!(license_name("APACHE2"), "Apache License 2.0");
        assert_eq!(license_name("UFL"), "Ubuntu Font Licence 1.0");
    }
}
