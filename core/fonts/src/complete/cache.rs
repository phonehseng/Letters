//! remembering fitted donors and completed fonts, so the next edit with
//! the same document font is instant. everything is keyed by a hash of
//! the document font's outlines (and of what else the result depends
//! on) and kept in a folder the caller chooses.
//!
//! files are written to a temporary name and renamed, so a crash never
//! leaves half a file; anything unreadable is ignored and made again.

use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};

use super::CompletedFont;
use crate::matching::fit::FittedDonor;

/// bump when what is stored changes, so old entries are not misread.
const CACHE_VERSION: &str = "completion-1";

/// the cache in one folder.
#[derive(Clone, Debug)]
pub struct CompletionCache {
    folder: PathBuf,
}

/// a cache key from the things a result depends on: 16 hex characters.
pub fn cache_key(parts: &[&str]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(CACHE_VERSION.as_bytes());
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.finalize()[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

impl CompletionCache {
    pub fn new(folder: &Path) -> CompletionCache {
        CompletionCache {
            folder: folder.to_path_buf(),
        }
    }

    fn path(&self, key: &str, ending: &str) -> PathBuf {
        self.folder.join(format!("{key}.{ending}"))
    }

    fn write(&self, path: &Path, bytes: &[u8]) {
        if std::fs::create_dir_all(&self.folder).is_err() {
            return;
        }
        let temporary = path.with_extension("saving");
        if std::fs::write(&temporary, bytes).is_ok() {
            std::fs::rename(&temporary, path).ok();
        }
    }

    pub fn load_completed(&self, key: &str) -> Option<CompletedFont> {
        let json = std::fs::read(self.path(key, "completed.json")).ok()?;
        let mut completed: CompletedFont = serde_json::from_slice(&json).ok()?;
        let font = std::fs::read(self.path(key, "completed.ttf")).ok()?;
        // even our own files are checked before use
        ttf_parser::Face::parse(&font, 0).ok()?;
        completed.font = font;
        completed.from_cache = true;
        Some(completed)
    }

    pub fn store_completed(&self, key: &str, completed: &CompletedFont) {
        if let Ok(json) = serde_json::to_vec(completed) {
            // the font first: a json without its font is never read
            self.write(&self.path(key, "completed.ttf"), &completed.font);
            self.write(&self.path(key, "completed.json"), &json);
        }
    }

    pub fn load_fit(&self, key: &str) -> Option<FittedDonor> {
        let json = std::fs::read(self.path(key, "fit.json")).ok()?;
        serde_json::from_slice(&json).ok()
    }

    pub fn store_fit(&self, key: &str, fit: &FittedDonor) {
        if let Ok(json) = serde_json::to_vec(fit) {
            self.write(&self.path(key, "fit.json"), &json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_depend_on_every_part_and_their_order() {
        let a = cache_key(&["outline", "jkq"]);
        assert_eq!(a.len(), 16);
        assert_eq!(a, cache_key(&["outline", "jkq"]));
        assert_ne!(a, cache_key(&["outline", "jkqx"]));
        assert_ne!(a, cache_key(&["jkq", "outline"]));
        // parts can't run into each other
        assert_ne!(cache_key(&["ab", "c"]), cache_key(&["a", "bc"]));
    }

    #[test]
    fn broken_entries_are_ignored() {
        let folder =
            std::env::temp_dir().join(format!("letters-fonts-cache-{}", std::process::id()));
        let cache = CompletionCache::new(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("abc.completed.json"), b"{not json").unwrap();
        assert!(cache.load_completed("abc").is_none());
        std::fs::write(folder.join("abc.fit.json"), b"[]").unwrap();
        assert!(cache.load_fit("abc").is_none());
        std::fs::remove_dir_all(&folder).ok();
    }
}
