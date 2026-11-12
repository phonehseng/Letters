//! every workbook open in any window.
//!
//! unlike the writer, the shell owns the whole workbook here: the ui only
//! ever holds the cells on screen, so a 200,000-row sheet never crosses
//! into the web view. each workbook has its own lock, so a long save in
//! one window never stops another window from scrolling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use letters_sheets::help::compat::{compatibility, Target};
use letters_sheets::Workbook;
use serde::Serialize;

use super::document::{EditOutcome, SheetDocument};
use super::files::{
    format_for_path, read_workbook, save_losses, save_workbook, with_sheet_extension, SheetFormat,
};
use super::recovery::{SheetRecoveryInfo, SheetRecoveryStore};
use super::tips::TipStore;
use crate::disk_stamp::{changed_on_disk_message, DiskStamp};
use crate::session::file_name;

/// what the ui gets when a workbook opens.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedSheet {
    pub id: u64,
    pub name: String,
    /// none for a new workbook: Save asks where
    pub path: Option<String>,
    pub format: SheetFormat,
    pub active_sheet: usize,
    pub state: EditOutcome,
    /// a recovery copy left by a run that ended without saving
    pub recovery: Option<SheetRecoveryInfo>,
    /// the engine's sentences about what this workbook holds that
    /// letters keeps but can't run (google sheets functions, macros)
    pub notes: Vec<String>,
    /// functions in the file letters doesn't have; their cells keep the
    /// values the file stored
    pub unknown_functions: Vec<String>,
}

/// the file a workbook belongs to after a save.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSheet {
    pub name: String,
    pub path: String,
    pub format: SheetFormat,
    pub state: EditOutcome,
}

struct OpenSheet {
    document: SheetDocument,
    window: String,
}

pub struct SheetSessions {
    open: Mutex<HashMap<u64, Arc<Mutex<OpenSheet>>>>,
    next_id: AtomicU64,
    recovery: SheetRecoveryStore,
    pub tips: TipStore,
}

impl SheetSessions {
    pub fn new(cache_dir: &Path, data_dir: &Path) -> SheetSessions {
        SheetSessions {
            open: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            recovery: SheetRecoveryStore::new(cache_dir.join("sheets recovery")),
            tips: TipStore::load(data_dir.join("sheet-tips.json")),
        }
    }

    /// opens a spreadsheet file. `recovered` opens its recovery copy instead
    /// (which then starts with unsaved changes).
    pub fn open(&self, window: &str, path: &Path, recovered: bool) -> Result<OpenedSheet, String> {
        let (mut book, format) = read_workbook(path)?;
        if recovered {
            book = self
                .recovery
                .workbook(path)
                .ok_or("The recovered version is no longer there.")?;
        }
        let mut document = SheetDocument::new(book, Some(path.to_path_buf()), format);
        if recovered {
            document.mark_unsaved();
        }
        let recovery = (!recovered).then(|| self.recovery.find(path)).flatten();
        Ok(self.keep(window, document, file_name(path), recovery))
    }

    /// a new workbook: empty, or one of the engine's starter workbooks
    /// ("budget", "grades", "lab", "schedule", "invoice").
    pub fn new_workbook(
        &self,
        window: &str,
        template: Option<&str>,
    ) -> Result<OpenedSheet, String> {
        let book = match template {
            None => Workbook::new(),
            Some(name) => letters_sheets::help::presets::template(name)
                .ok_or_else(|| format!("There is no starter workbook called “{name}”."))?,
        };
        let name = template
            .map(title_case)
            .unwrap_or_else(|| "Untitled".into());
        let mut document = SheetDocument::new(book, None, SheetFormat::Xlsx);
        // a starter workbook isn't on disk yet, so closing it asks first
        if template.is_some() {
            document.mark_unsaved();
        }
        Ok(self.keep(window, document, format!("{name}.xlsx"), None))
    }

    fn keep(
        &self,
        window: &str,
        document: SheetDocument,
        name: String,
        recovery: Option<SheetRecoveryInfo>,
    ) -> OpenedSheet {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let opened = OpenedSheet {
            id,
            name,
            path: document
                .path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            format: document.format,
            active_sheet: document
                .book
                .active_sheet
                .min(document.book.sheets.len() - 1),
            state: document.current(),
            recovery,
            notes: compatibility(&document.book, Target::Xlsx)
                .into_iter()
                .map(|note| note.text)
                .collect(),
            unknown_functions: document.unknown_functions.clone(),
        };
        let entry = OpenSheet {
            document,
            window: window.to_string(),
        };
        self.map().insert(id, Arc::new(Mutex::new(entry)));
        opened
    }

    /// the sentence for a save over the workbook's own file that another
    /// program changed since this window read or wrote it; `None` when it
    /// is as this window left it (the ui then asks Replace, Cancel or Save As).
    /// `to` is where the save goes (none: the workbook's own file); a save
    /// elsewhere is never asked about.
    pub fn changed_on_disk(&self, id: u64, to: Option<&Path>) -> Result<Option<String>, String> {
        self.with(id, |document| {
            let Some(path) = &document.path else {
                return Ok(None);
            };
            if to.is_some_and(|to| to != path.as_path()) {
                return Ok(None);
            };
            Ok(document
                .disk_stamp
                .is_some_and(|stamp| stamp.changed_since(path))
                .then(|| changed_on_disk_message(&file_name(path))))
        })
    }

    /// saves to the workbook's file, or to `to` (Save As; the format comes
    /// from the name). csv writes `sheet`, the one on screen.
    /// `replace_changed`: the person chose to write over a change made to
    /// the file elsewhere; otherwise such a save is refused.
    pub fn save(
        &self,
        id: u64,
        to: Option<&Path>,
        sheet: usize,
        replace_changed: bool,
    ) -> Result<SavedSheet, String> {
        self.with(id, |document| {
            let target = match (to, &document.path) {
                (Some(path), _) => with_sheet_extension(path, document.format),
                (None, Some(path)) => path.clone(),
                (None, None) => return Err("Choose where to save this new workbook.".into()),
            };
            let over_own_file = document.path.as_ref() == Some(&target);
            if over_own_file
                && !replace_changed
                && document
                    .disk_stamp
                    .is_some_and(|stamp| stamp.changed_since(&target))
            {
                return Err(changed_on_disk_message(&file_name(&target)));
            }
            let format = format_for_path(&target).unwrap_or(document.format);
            save_workbook(&document.book, &target, format, sheet)?;
            document.disk_stamp = DiskStamp::of(&target);
            if let Some(old) = &document.path {
                self.recovery.discard(old);
            }
            // a csv copy is a copy: the workbook stays the full one, so
            // closing it still asks before its sheets and formulas are lost
            if format == SheetFormat::Csv && document.book.sheets.len() > 1 {
                document.mark_unsaved();
            } else {
                document.mark_saved();
            }
            document.path = Some(target.clone());
            document.format = format;
            Ok(SavedSheet {
                name: file_name(&target),
                path: target.to_string_lossy().into_owned(),
                format,
                state: document.current(),
            })
        })
    }

    /// what saving as `path`'s format would leave out, or `None`.
    pub fn save_losses(&self, id: u64, path: &Path) -> Result<Option<String>, String> {
        self.with(id, |document| {
            let format = format_for_path(path).unwrap_or(document.format);
            Ok(save_losses(&document.book, format))
        })
    }

    /// writes a recovery copy when there are changes the last copy lacks.
    pub fn autosave(&self, id: u64) -> Result<(), String> {
        self.with(id, |document| {
            let Some(path) = document.path.clone() else {
                return Ok(());
            };
            if !document.dirty() || document.recovered_revision == document.revision() {
                return Ok(());
            }
            self.recovery
                .write(&path, &document.book, document.format)?;
            document.recovered_revision = document.revision();
            Ok(())
        })
    }

    pub fn discard_recovery(&self, id: u64) -> Result<(), String> {
        self.with(id, |document| {
            if let Some(path) = &document.path {
                self.recovery.discard(path);
            }
            Ok(())
        })
    }

    pub fn close(&self, id: u64) {
        self.map().remove(&id);
    }

    /// a closed window's workbooks go too (unsaved changes were offered
    /// for saving by the window's close sheet).
    pub fn close_window(&self, window: &str) {
        self.map().retain(|_, entry| {
            entry
                .lock()
                .map(|open| open.window != window)
                .unwrap_or(false)
        });
    }

    pub fn open_count(&self) -> usize {
        self.map().len()
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Arc<Mutex<OpenSheet>>>> {
        self.open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// runs `work` on one open workbook.
    pub fn with<T>(
        &self,
        id: u64,
        work: impl FnOnce(&mut SheetDocument) -> Result<T, String>,
    ) -> Result<T, String> {
        let entry = self
            .map()
            .get(&id)
            .cloned()
            .ok_or("This workbook is no longer open.")?;
        let mut open = entry
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        work(&mut open.document)
    }

    pub fn path_of(&self, id: u64) -> Option<PathBuf> {
        self.with(id, |document| Ok(document.path.clone()))
            .ok()
            .flatten()
    }
}

/// "budget" becomes "Budget", the name a starter workbook first has.
fn title_case(name: &str) -> String {
    let mut letters = name.chars();
    match letters.next() {
        Some(first) => first.to_uppercase().chain(letters).collect(),
        None => String::new(),
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
