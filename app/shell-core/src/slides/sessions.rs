//! every presentation open in any window. the shell owns each deck
//! (unlike the writer, whose ui holds the model): the editor sends the
//! engine's operations, and gets back what changed and the slides'
//! pictures. one lock per deck, so two windows never wait on each other.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use letters_slides::edit::Operation;
use letters_slides::export::{export_pdf, PdfLayout};
use serde::Serialize;

use super::deck::{Change, DeckFormat, SlideDeck};
use super::find::{find, replace_one, Found};
use super::new_objects::{build, next_object_id, picture, NewObject, PointBox};
use super::pictures::{LeftOut, PictureViewer, SlidePicture, SlidePictures};
use super::recovery::DeckRecovery;
use super::view::{deck_view, slide_view, DeckView, SlideView};
use crate::session::file_name;
use crate::writer::recovery::RecoveryInfo;
use crate::writer::save::write_atomically;

/// what the editor gets when a deck opens.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedDeck {
    pub id: u64,
    pub name: String,
    pub path: String,
    pub format: DeckFormat,
    pub deck: DeckView,
    pub state: Change,
    /// a recovery copy a run that ended without saving left behind
    pub recovery: Option<RecoveryInfo>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDeck {
    pub name: String,
    pub path: String,
    pub format: DeckFormat,
    /// what odp could only approximate, in the engine's words
    pub notes: Vec<String>,
    pub state: Change,
}

/// an edit's result: what to redraw, and the new deck summary.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edited {
    pub change: Change,
    pub deck: DeckView,
    /// the object an insert added, so the editor can select it
    pub added: Option<u32>,
}

struct OpenDeck {
    deck: SlideDeck,
    pictures: SlidePictures,
    window: String,
}

pub struct SlidesSessions {
    open: Mutex<HashMap<u64, Arc<Mutex<OpenDeck>>>>,
    next_id: AtomicU64,
    recovery: DeckRecovery,
    pictures_folder: PathBuf,
}

impl SlidesSessions {
    pub fn new(cache_dir: &Path) -> SlidesSessions {
        SlidesSessions {
            open: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            recovery: DeckRecovery::new(cache_dir.join("slides recovery")),
            pictures_folder: cache_dir.join("slide pictures"),
        }
    }

    /// opens a .pptx or .odp; `recovered` opens its recovery copy instead.
    pub fn open(&self, window: &str, path: &Path, recovered: bool) -> Result<OpenedDeck, String> {
        let deck = if recovered {
            self.recovery
                .deck(path)
                .ok_or("The recovered version is no longer there.")?
        } else {
            SlideDeck::open(path)?
        };
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let opened = OpenedDeck {
            id,
            name: file_name(&deck.path),
            path: deck.path.to_string_lossy().into_owned(),
            format: deck.format,
            deck: deck_view(&deck.presentation),
            state: deck.state(),
            recovery: if recovered {
                None
            } else {
                self.recovery.find(path)
            },
        };
        let pictures = SlidePictures::new(
            self.pictures_folder
                .join(format!("{}-{id}", std::process::id())),
        );
        let entry = OpenDeck {
            deck,
            pictures,
            window: window.to_string(),
        };
        self.map().insert(id, Arc::new(Mutex::new(entry)));
        Ok(opened)
    }

    pub fn deck(&self, id: u64) -> Result<DeckView, String> {
        self.with(id, |open| Ok(deck_view(&open.deck.presentation)))
    }

    pub fn slide(&self, id: u64, slide_id: &str) -> Result<SlideView, String> {
        self.with(id, |open| {
            let presentation = &open.deck.presentation;
            let slide = presentation
                .slides
                .iter()
                .find(|slide| slide.id == slide_id)
                .ok_or("This slide is no longer in the presentation.")?;
            Ok(slide_view(presentation, slide))
        })
    }

    /// runs one of the engine's operations.
    pub fn apply(
        &self,
        id: u64,
        operation: &Operation,
        viewer: &dyn PictureViewer,
    ) -> Result<Edited, String> {
        self.with(id, |open| {
            let change = open.deck.apply(operation)?;
            Ok(open.after(change, None, viewer))
        })
    }

    pub fn undo(&self, id: u64, viewer: &dyn PictureViewer) -> Result<Edited, String> {
        self.with(id, |open| {
            let change = open.deck.undo()?.unwrap_or_else(|| open.deck.state());
            Ok(open.after(change, None, viewer))
        })
    }

    pub fn redo(&self, id: u64, viewer: &dyn PictureViewer) -> Result<Edited, String> {
        self.with(id, |open| {
            let change = open.deck.redo()?.unwrap_or_else(|| open.deck.state());
            Ok(open.after(change, None, viewer))
        })
    }

    /// adds a shape, text box or table to a slide.
    pub fn insert(
        &self,
        id: u64,
        slide_id: &str,
        new: &NewObject,
        at: PointBox,
        viewer: &dyn PictureViewer,
    ) -> Result<Edited, String> {
        self.with(id, |open| {
            let object_id = open.next_id_on(slide_id)?;
            let object = build(new, at, object_id);
            open.add(slide_id, object, viewer)
        })
    }

    /// adds a picture file to a slide, in `at`.
    pub fn insert_picture(
        &self,
        id: u64,
        slide_id: &str,
        file: &Path,
        at: PointBox,
        viewer: &dyn PictureViewer,
    ) -> Result<Edited, String> {
        let bytes = std::fs::read(file)
            .map_err(|error| format!("“{}” can’t be read. ({error})", file_name(file)))?;
        self.with(id, |open| {
            let object_id = open.next_id_on(slide_id)?;
            let object = picture(
                &mut open.deck.presentation,
                &file_name(file),
                bytes,
                at,
                object_id,
            );
            open.add(slide_id, object, viewer)
        })
    }

    pub fn pictures(
        &self,
        id: u64,
        slide_ids: &[String],
        viewer: &dyn PictureViewer,
    ) -> Result<Vec<SlidePicture>, String> {
        self.with(id, |open| {
            open.pictures
                .pictures(&open.deck.presentation, slide_ids, viewer)
        })
    }

    /// the slide's picture without the text being typed into.
    pub fn picture_without(
        &self,
        id: u64,
        slide_id: &str,
        left_out: LeftOut,
        viewer: &dyn PictureViewer,
    ) -> Result<SlidePicture, String> {
        self.with(id, |open| {
            open.pictures
                .without(&mut open.deck.presentation, slide_id, left_out, viewer)
        })
    }

    pub fn find(&self, id: u64, words: &str, match_case: bool) -> Result<Vec<Found>, String> {
        self.with(id, |open| {
            Ok(find(&open.deck.presentation, words, match_case))
        })
    }

    pub fn replace(
        &self,
        id: u64,
        found: &Found,
        words: &str,
        replacement: &str,
        match_case: bool,
        viewer: &dyn PictureViewer,
    ) -> Result<Edited, String> {
        self.with(id, |open| {
            let operation = replace_one(
                &open.deck.presentation,
                found,
                words,
                replacement,
                match_case,
            )?;
            let change = open.deck.apply(&operation)?;
            Ok(open.after(change, None, viewer))
        })
    }

    /// saves to the deck's file or to `to`; the recovery copy goes.
    pub fn save(&self, id: u64, to: Option<&Path>) -> Result<SavedDeck, String> {
        self.with(id, |open| {
            let old_path = open.deck.path.clone();
            let notes = open.deck.save(to)?;
            self.recovery.discard(&old_path);
            Ok(SavedDeck {
                name: file_name(&open.deck.path),
                path: open.deck.path.to_string_lossy().into_owned(),
                format: open.deck.format,
                notes,
                state: open.deck.state(),
            })
        })
    }

    /// writes the recovery copy while there are unsaved changes.
    pub fn autosave(&self, id: u64) -> Result<(), String> {
        self.with(id, |open| {
            if open.deck.dirty() {
                self.recovery.write(&open.deck)?;
            }
            Ok(())
        })
    }

    pub fn discard_recovery(&self, id: u64) -> Result<(), String> {
        self.with(id, |open| {
            self.recovery.discard(&open.deck.path);
            Ok(())
        })
    }

    /// Export as PDF: slides, notes pages or handouts, drawn by the engine.
    pub fn export_pdf(&self, id: u64, layout: PdfLayout, to: &Path) -> Result<Vec<String>, String> {
        self.with(id, |open| {
            let (bytes, report) =
                export_pdf(&open.deck.presentation, layout, open.pictures.fonts())
                    .map_err(|error| error.to_string())?;
            write_atomically(&bytes, to)?;
            Ok(report.approximations)
        })
    }

    pub fn path_of(&self, id: u64) -> Option<PathBuf> {
        self.with(id, |open| Ok(open.deck.path.clone())).ok()
    }

    pub fn close(&self, id: u64, viewer: &dyn PictureViewer) {
        if let Some(entry) = self.map().remove(&id) {
            lock(&entry).pictures.close(viewer);
        }
    }

    /// a closed window's decks go too (their unsaved changes were offered
    /// for saving by the window's close sheet).
    pub fn close_window(&self, window: &str, viewer: &dyn PictureViewer) {
        let ids: Vec<u64> = self
            .map()
            .iter()
            .filter(|(_, entry)| lock(entry).window == window)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.close(id, viewer);
        }
    }

    pub fn open_count(&self) -> usize {
        self.map().len()
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Arc<Mutex<OpenDeck>>>> {
        self.open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn with<T>(
        &self,
        id: u64,
        work: impl FnOnce(&mut OpenDeck) -> Result<T, String>,
    ) -> Result<T, String> {
        let entry = self
            .map()
            .get(&id)
            .cloned()
            .ok_or("This presentation is no longer open.")?;
        let mut open = lock(&entry);
        work(&mut open)
    }
}

fn lock(entry: &Arc<Mutex<OpenDeck>>) -> std::sync::MutexGuard<'_, OpenDeck> {
    entry
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl OpenDeck {
    /// pictures of changed slides are redrawn when next asked for.
    fn after(&mut self, change: Change, added: Option<u32>, viewer: &dyn PictureViewer) -> Edited {
        self.pictures.changed(
            &self.deck.presentation,
            &change.changed_slides,
            change.all_slides,
            viewer,
        );
        Edited {
            change,
            deck: deck_view(&self.deck.presentation),
            added,
        }
    }

    fn next_id_on(&self, slide_id: &str) -> Result<u32, String> {
        self.deck
            .presentation
            .slides
            .iter()
            .find(|slide| slide.id == slide_id)
            .map(next_object_id)
            .ok_or_else(|| "This slide is no longer in the presentation.".to_string())
    }

    fn add(
        &mut self,
        slide_id: &str,
        object: letters_slides::model::Object,
        viewer: &dyn PictureViewer,
    ) -> Result<Edited, String> {
        let operation = Operation::AddObject {
            slide_id: slide_id.to_string(),
            object: Box::new(object),
        };
        let change = self.deck.apply(&operation)?;
        let added = self
            .deck
            .presentation
            .slides
            .iter()
            .find(|slide| slide.id == slide_id)
            .and_then(|slide| slide.objects.last())
            .map(|object| object.id());
        Ok(self.after(change, added, viewer))
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
