//! recovery copies of presentations, as the writer keeps them for word
//! documents: while a deck has unsaved changes the editor asks for a copy
//! every few seconds, written to the app's cache in the deck's own format.
//! the next time the file is opened after a crash, the copy is offered.
//! a copy is deleted when the deck is saved or closed with "Don't Save".

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::deck::{read_bytes, DeckFormat, SlideDeck};
use crate::recents::thumbnail_name;
use crate::writer::recovery::RecoveryInfo;
use crate::writer::save::write_atomically;

/// what is written beside the copy, so a copy is only offered for the
/// file it belongs to (names are hashes, which could in theory collide)
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CopyLabel {
    original_path: String,
    saved_at: u64,
    format: String,
}

pub struct DeckRecovery {
    folder: PathBuf,
}

impl DeckRecovery {
    pub fn new(folder: PathBuf) -> DeckRecovery {
        DeckRecovery { folder }
    }

    fn files_for(&self, original: &Path) -> (PathBuf, PathBuf) {
        let name = thumbnail_name(original);
        (
            self.folder.join(format!("{name}.deck")),
            self.folder.join(format!("{name}.json")),
        )
    }

    /// writes the copy: the deck first, then its label, each in one step,
    /// so a crash while writing never leaves a label for half a deck.
    pub fn write(&self, deck: &SlideDeck) -> Result<(), String> {
        fs::create_dir_all(&self.folder).map_err(|error| error.to_string())?;
        let (copy, label) = self.files_for(&deck.path);
        let (bytes, _) = deck.bytes(deck.format)?;
        write_atomically(&bytes, &copy)?;
        let label_text = serde_json::to_vec(&CopyLabel {
            original_path: deck.path.to_string_lossy().into_owned(),
            saved_at: seconds_since_1970(),
            format: deck.format.extension().to_string(),
        })
        .map_err(|error| error.to_string())?;
        write_atomically(&label_text, &label)
    }

    pub fn find(&self, original: &Path) -> Option<RecoveryInfo> {
        self.label(original).map(|label| RecoveryInfo {
            saved_at: label.saved_at,
        })
    }

    /// the recovered deck, still belonging to `original`.
    pub fn deck(&self, original: &Path) -> Option<SlideDeck> {
        let label = self.label(original)?;
        let format = if label.format == "odp" {
            DeckFormat::Odp
        } else {
            DeckFormat::Pptx
        };
        let bytes = fs::read(self.files_for(original).0).ok()?;
        let presentation = read_bytes(&bytes, format).ok()?;
        Some(SlideDeck::recovered(original, format, presentation))
    }

    pub fn discard(&self, original: &Path) {
        let (copy, label) = self.files_for(original);
        let _ = fs::remove_file(copy);
        let _ = fs::remove_file(label);
    }

    fn label(&self, original: &Path) -> Option<CopyLabel> {
        let text = fs::read(self.files_for(original).1).ok()?;
        let label: CopyLabel = serde_json::from_slice(&text).ok()?;
        (Path::new(&label.original_path) == original).then_some(label)
    }
}

fn seconds_since_1970() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::test_decks::{deck_path, scratch_folder};
    use letters_slides::edit::Operation;

    #[test]
    fn an_edited_deck_is_recovered_after_a_restart() {
        let store = DeckRecovery::new(scratch_folder("recovery-restart"));
        let mut deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        deck.apply(&Operation::DeleteSlide { index: 1 }).unwrap();
        store.write(&deck).unwrap();
        let next_run = DeckRecovery::new(store.folder.clone());
        assert!(next_run.find(&deck.path).unwrap().saved_at > 0);
        let recovered = next_run.deck(&deck.path).unwrap();
        assert_eq!(recovered.presentation.slides.len(), 1);
        assert!(recovered.dirty(), "a recovered deck still needs saving");
        assert_eq!(recovered.path, deck.path);
    }

    #[test]
    fn discarding_removes_the_copy_and_other_files_have_none() {
        let store = DeckRecovery::new(scratch_folder("recovery-discard"));
        let deck = SlideDeck::open(&deck_path("pp_shapes.pptx")).unwrap();
        store.write(&deck).unwrap();
        assert!(store.find(Path::new("C:/talks/other.pptx")).is_none());
        store.discard(&deck.path);
        assert!(store.find(&deck.path).is_none());
    }
}
