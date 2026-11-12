//! one open presentation: the engine's model of it, where it came from,
//! and its undo history.
//!
//! every change goes through `apply`, which runs one of the engine's
//! operations and keeps the inverse the engine hands back. undo applies
//! that inverse (and keeps its own inverse for redo), so undoing is exact
//! and an object nobody touched keeps its original xml.

use std::path::{Path, PathBuf};

use letters_slides::edit::{apply, Operation};
use letters_slides::{read_odp, read_pptx, write_odp, write_pptx, Presentation};
use serde::Serialize;

use crate::session::file_name;
use crate::writer::save::write_atomically;

/// how many steps undo goes back. each step holds a copy of what it
/// changed (usually one slide), so a long session stays a few megabytes.
const UNDO_LIMIT: usize = 200;

/// the two formats a deck is read from and saved in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeckFormat {
    Pptx,
    Odp,
}

impl DeckFormat {
    /// the format a file name asks for, by its extension.
    pub fn for_path(path: &Path) -> Option<DeckFormat> {
        let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
        match extension.as_str() {
            "pptx" => Some(DeckFormat::Pptx),
            "odp" => Some(DeckFormat::Odp),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            DeckFormat::Pptx => "pptx",
            DeckFormat::Odp => "odp",
        }
    }
}

/// what one change did, for the editor to redraw just that.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    /// the slides whose drawing changed
    pub changed_slides: Vec<String>,
    /// true when every slide may look different (a theme, the slide list)
    pub all_slides: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
}

pub struct SlideDeck {
    pub path: PathBuf,
    pub format: DeckFormat,
    pub presentation: Presentation,
    undo: Vec<Operation>,
    redo: Vec<Operation>,
    /// how many steps the history is from where it started; with
    /// `saved_position` it says whether there are unsaved changes, so
    /// undoing back to the saved state counts as saved again
    position: i64,
    saved_position: Option<i64>,
}

impl SlideDeck {
    /// reads a .pptx or .odp. the error is the engine's sentence.
    pub fn open(path: &Path) -> Result<SlideDeck, String> {
        let format = DeckFormat::for_path(path).ok_or_else(|| {
            format!(
                "“{}” isn’t a PowerPoint (.pptx) or OpenDocument (.odp) presentation.",
                file_name(path)
            )
        })?;
        let bytes = std::fs::read(path)
            .map_err(|error| format!("“{}” can’t be read. ({error})", file_name(path)))?;
        let presentation = read_bytes(&bytes, format)?;
        Ok(SlideDeck::from_presentation(path, format, presentation))
    }

    pub fn from_presentation(
        path: &Path,
        format: DeckFormat,
        presentation: Presentation,
    ) -> SlideDeck {
        SlideDeck {
            path: path.to_path_buf(),
            format,
            presentation,
            undo: Vec::new(),
            redo: Vec::new(),
            position: 0,
            saved_position: Some(0),
        }
    }

    /// a deck read back from a recovery copy: it belongs to `path` but
    /// has changes that were never saved there.
    pub fn recovered(path: &Path, format: DeckFormat, presentation: Presentation) -> SlideDeck {
        let mut deck = SlideDeck::from_presentation(path, format, presentation);
        deck.saved_position = None;
        deck
    }

    /// runs one edit. on an error nothing has changed.
    pub fn apply(&mut self, operation: &Operation) -> Result<Change, String> {
        let applied =
            apply(&mut self.presentation, operation).map_err(|error| error.to_string())?;
        self.undo.push(applied.inverse);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        // a new edit after undoing makes the undone steps unreachable,
        // and with them a saved state that lay among them
        if self
            .saved_position
            .is_some_and(|saved| saved > self.position)
        {
            self.saved_position = None;
        }
        self.redo.clear();
        self.position += 1;
        Ok(self.change(applied.changed_slides, applied.all_slides))
    }

    /// takes back the last edit. `None` when there is nothing to undo.
    pub fn undo(&mut self) -> Result<Option<Change>, String> {
        let Some(inverse) = self.undo.pop() else {
            return Ok(None);
        };
        let applied = apply(&mut self.presentation, &inverse).map_err(|error| error.to_string())?;
        self.redo.push(applied.inverse);
        self.position -= 1;
        Ok(Some(
            self.change(applied.changed_slides, applied.all_slides),
        ))
    }

    /// makes the last undone edit again. `None` when there is nothing to redo.
    pub fn redo(&mut self) -> Result<Option<Change>, String> {
        let Some(again) = self.redo.pop() else {
            return Ok(None);
        };
        let applied = apply(&mut self.presentation, &again).map_err(|error| error.to_string())?;
        self.undo.push(applied.inverse);
        self.position += 1;
        Ok(Some(
            self.change(applied.changed_slides, applied.all_slides),
        ))
    }

    pub fn dirty(&self) -> bool {
        self.saved_position != Some(self.position)
    }

    /// the history's state, with nothing redrawn.
    pub fn state(&self) -> Change {
        self.change(Vec::new(), false)
    }

    fn change(&self, changed_slides: Vec<String>, all_slides: bool) -> Change {
        Change {
            changed_slides,
            all_slides,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            dirty: self.dirty(),
        }
    }

    /// the deck as a file in `format`, with what odp could only approximate.
    pub fn bytes(&self, format: DeckFormat) -> Result<(Vec<u8>, Vec<String>), String> {
        match format {
            DeckFormat::Pptx => write_pptx(&self.presentation)
                .map(|bytes| (bytes, Vec::new()))
                .map_err(|error| error.to_string()),
            DeckFormat::Odp => write_odp(&self.presentation).map_err(|error| error.to_string()),
        }
    }

    /// saves to the deck's own file, or to `to` (Save As), in the format
    /// the name asks for. afterwards the deck belongs to the saved file.
    /// returns the engine's notes on what odp approximated.
    pub fn save(&mut self, to: Option<&Path>) -> Result<Vec<String>, String> {
        let target = to.map_or_else(
            || self.path.clone(),
            |path| with_deck_extension(path, self.format),
        );
        let format = DeckFormat::for_path(&target).unwrap_or(self.format);
        let (bytes, notes) = self.bytes(format)?;
        write_atomically(&bytes, &target)?;
        self.path = target;
        self.format = format;
        self.saved_position = Some(self.position);
        Ok(notes)
    }
}

/// reads a deck's bytes in a format.
pub fn read_bytes(bytes: &[u8], format: DeckFormat) -> Result<Presentation, String> {
    let read = match format {
        DeckFormat::Pptx => read_pptx(bytes),
        DeckFormat::Odp => read_odp(bytes),
    };
    read.map_err(|error| error.to_string())
}

/// "Talk" saved from a pptx becomes "Talk.pptx"; "Talk.odp" stays.
pub fn with_deck_extension(path: &Path, current: DeckFormat) -> PathBuf {
    if DeckFormat::for_path(path).is_some() {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(current.extension());
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slides::test_decks::{deck_path, scratch_folder};

    fn first_text_object(deck: &SlideDeck) -> (String, u32) {
        let slide = &deck.presentation.slides[0];
        let object = slide.objects.iter().find(|o| o.text().is_some()).unwrap();
        (slide.id.clone(), object.id())
    }

    fn retype(deck: &mut SlideDeck, words: &str) -> Change {
        let (slide_id, object_id) = first_text_object(deck);
        let operation = Operation::SetText {
            slide_id,
            object_id,
            paragraphs: vec![words.to_string()],
        };
        deck.apply(&operation).unwrap()
    }

    #[test]
    fn an_edit_is_undone_and_redone_exactly() {
        let mut deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        let before = serde_json::to_string(&deck.presentation.slides).unwrap();
        let change = retype(&mut deck, "New title");
        assert!(change.dirty && change.can_undo && !change.can_redo);
        assert_eq!(
            change.changed_slides,
            vec![deck.presentation.slides[0].id.clone()]
        );
        let edited = serde_json::to_string(&deck.presentation.slides).unwrap();
        let undone = deck.undo().unwrap().unwrap();
        assert!(!undone.dirty && undone.can_redo);
        assert_eq!(
            serde_json::to_string(&deck.presentation.slides).unwrap(),
            before
        );
        deck.redo().unwrap().unwrap();
        assert_eq!(
            serde_json::to_string(&deck.presentation.slides).unwrap(),
            edited
        );
    }

    #[test]
    fn nothing_to_undo_is_not_an_error() {
        let mut deck = SlideDeck::open(&deck_path("pp_shapes.pptx")).unwrap();
        assert!(deck.undo().unwrap().is_none());
        assert!(deck.redo().unwrap().is_none());
    }

    #[test]
    fn saving_marks_the_deck_clean_and_undo_past_it_dirty() {
        let folder = scratch_folder("deck-save");
        let mut deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        retype(&mut deck, "Saved words");
        deck.save(Some(&folder.join("talk"))).unwrap();
        assert_eq!(deck.path, folder.join("talk.pptx"));
        assert!(!deck.dirty());
        deck.undo().unwrap();
        assert!(deck.dirty());
        let reopened = SlideDeck::open(&folder.join("talk.pptx")).unwrap();
        let (slide, object) = first_text_object(&reopened);
        let text = reopened.presentation.slides[0]
            .objects
            .iter()
            .find(|o| o.id() == object)
            .and_then(|o| o.text())
            .unwrap()
            .plain_text();
        assert_eq!(text, "Saved words");
        assert_eq!(slide, reopened.presentation.slides[0].id);
    }

    #[test]
    fn a_deck_saved_without_edits_keeps_every_byte() {
        let folder = scratch_folder("deck-bytes");
        let original = deck_path("pp_groups_connectors.pptx");
        let mut deck = SlideDeck::open(&original).unwrap();
        deck.save(Some(&folder.join("copy.pptx"))).unwrap();
        let parts = |path: &Path| {
            let reread = read_pptx(&std::fs::read(path).unwrap()).unwrap();
            reread.carry.parts
        };
        assert_eq!(parts(&original), parts(&folder.join("copy.pptx")));
    }

    #[test]
    fn save_as_odp_switches_the_format() {
        let folder = scratch_folder("deck-odp");
        let mut deck = SlideDeck::open(&deck_path("pp_title_bullets.pptx")).unwrap();
        deck.save(Some(&folder.join("talk.odp"))).unwrap();
        assert_eq!(deck.format, DeckFormat::Odp);
        let reopened = SlideDeck::open(&folder.join("talk.odp")).unwrap();
        assert_eq!(
            reopened.presentation.slides.len(),
            deck.presentation.slides.len()
        );
    }

    #[test]
    fn files_that_are_not_presentations_get_a_sentence() {
        let error = SlideDeck::open(Path::new("C:/talks/notes.txt"))
            .err()
            .unwrap();
        assert!(error.contains(".pptx"));
        let folder = scratch_folder("deck-broken");
        let broken = folder.join("broken.pptx");
        std::fs::write(&broken, b"not a zip").unwrap();
        assert!(SlideDeck::open(&broken).is_err());
    }
}
