//! the fonts that may lend glyphs when new text needs letters a document's
//! subset font lacks: every font of the app's font library (system, added,
//! bundled and downloaded), handed to letters-edit for one edit at a time.
//!
//! making the pool reads nothing from disk, but each donor keeps its font
//! bytes once read, so the same pool is kept until the library's index
//! changes (a scan finishing, a font added) instead of being made per edit.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use letters_edit::FontDonors;
use letters_fonts::matching::donors::DonorPool;
use letters_fonts::FontIndex;

/// the donors made from the library's latest index.
pub struct DonorCache {
    /// where letters-fonts remembers fitted fonts between edits
    cache_folder: Option<PathBuf>,
    kept: Mutex<Option<(Arc<FontIndex>, FontDonors)>>,
}

impl DonorCache {
    pub fn new(cache_folder: Option<PathBuf>) -> DonorCache {
        DonorCache {
            cache_folder,
            kept: Mutex::new(None),
        }
    }

    /// the donors for `index`, made again only when it is a new index.
    pub fn donors_for(&self, index: Arc<FontIndex>) -> FontDonors {
        let mut kept = self
            .kept
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((kept_index, donors)) = kept.as_ref() {
            if Arc::ptr_eq(kept_index, &index) {
                return donors.clone();
            }
        }
        let donors = FontDonors::new(DonorPool::from_index(&index), self.cache_folder.clone());
        *kept = Some((index, donors.clone()));
        donors
    }

    /// whether the kept donors were made from `index`.
    pub fn is_current(&self, index: &Arc<FontIndex>) -> bool {
        let kept = self
            .kept
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        kept.as_ref()
            .is_some_and(|(kept_index, _)| Arc::ptr_eq(kept_index, index))
    }
}
