//! one open workbook: the model, its calculation engine, its file and
//! its undo history. the ui never holds the workbook; it asks for the
//! cells it shows (look.rs) and sends edits here.
//!
//! after every edit only the formulas that depend on the changed cells
//! are calculated again; edits that move rows, columns or sheets rebuild
//! the engine, because every reference may have shifted.

use std::path::PathBuf;
use std::time::Instant;

use letters_sheets::analysis::{SelectionStats, StatsCache};
use letters_sheets::calc::{CalcReport, CellKey, Engine};
use letters_sheets::{Range, Workbook};
use serde::Serialize;

use super::files::SheetFormat;
use super::history::{apply_all, Edit, History, Step, Touched};
use super::look::{sheet_tabs, SheetTab};

/// what the ui learns after an edit, an undo or a redo.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOutcome {
    /// goes up with every change, so the ui knows its cells are stale
    pub revision: u64,
    pub dirty: bool,
    /// rows, columns or sheets moved, or sizes changed: redraw everything
    pub structure: bool,
    pub sheets: Vec<SheetTab>,
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    /// cells in a circular reference, as "Sheet1!A1"
    pub circular: Vec<String>,
    /// the step taken back or done again, for "Undo Paste" messages
    pub label: Option<String>,
    pub calc_milliseconds: f64,
}

pub struct SheetDocument {
    pub book: Workbook,
    engine: Engine,
    /// none for a new workbook that has never been saved
    pub path: Option<PathBuf>,
    pub format: SheetFormat,
    history: History,
    revision: u64,
    saved_revision: u64,
    circular: Vec<CellKey>,
    /// the status bar's running totals, for one sheet at one revision
    stats: Option<(usize, u64, StatsCache)>,
    /// the revision the last recovery copy holds
    pub recovered_revision: u64,
    /// functions the file uses that letters doesn't have; their cells
    /// keep the values the file stored
    pub unknown_functions: Vec<String>,
}

impl SheetDocument {
    /// reads every formula and calculates the workbook once, so values
    /// from files written by other programs (or none at all, as in the
    /// starter workbooks) are this engine's own from the start.
    pub fn new(mut book: Workbook, path: Option<PathBuf>, format: SheetFormat) -> SheetDocument {
        let mut engine = Engine::new(&book);
        let report = engine.recalculate_all(&mut book);
        let mut unknown_functions: Vec<String> = report
            .not_recalculated
            .iter()
            .flat_map(|cell| cell.functions.iter().cloned())
            .collect();
        unknown_functions.sort();
        unknown_functions.dedup();
        SheetDocument {
            unknown_functions,
            book,
            engine,
            path,
            format,
            history: History::default(),
            revision: 0,
            saved_revision: 0,
            circular: report.circular,
            stats: None,
            recovered_revision: 0,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// marks the workbook as unsaved (a recovered copy, a new workbook).
    pub fn mark_unsaved(&mut self) {
        self.revision += 1;
    }

    pub fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
    }

    /// applies one action and remembers how to undo it.
    pub fn apply(&mut self, edits: &[Edit], label: &str) -> Result<EditOutcome, String> {
        let (undo, touched) = apply_all(&mut self.book, edits)?;
        self.history.record(Step {
            label: label.to_string(),
            edits: undo,
        });
        Ok(self.after_change(touched, None))
    }

    /// takes back the last action; `None` when there is nothing to undo.
    pub fn undo(&mut self) -> Result<Option<EditOutcome>, String> {
        let Some(step) = self.history.take_undo() else {
            return Ok(None);
        };
        let (redo, touched) = apply_all(&mut self.book, &step.edits)?;
        self.history.push_redo(Step {
            label: step.label.clone(),
            edits: redo,
        });
        Ok(Some(self.after_change(touched, Some(step.label))))
    }

    /// does the last undone action again.
    pub fn redo(&mut self) -> Result<Option<EditOutcome>, String> {
        let Some(step) = self.history.take_redo() else {
            return Ok(None);
        };
        let (undo, touched) = apply_all(&mut self.book, &step.edits)?;
        self.history.push_undo(Step {
            label: step.label.clone(),
            edits: undo,
        });
        Ok(Some(self.after_change(touched, Some(step.label))))
    }

    fn after_change(&mut self, touched: Touched, label: Option<String>) -> EditOutcome {
        let started = Instant::now();
        self.revision += 1;
        self.stats = None;
        let structure = touched.structure || touched.layout_only;
        if touched.structure {
            self.engine = Engine::new(&self.book);
            let report = self.engine.recalculate_all(&mut self.book);
            self.circular = report.circular;
        } else if !touched.layout_only {
            let report = self.engine.cells_changed(&mut self.book, &touched.cells);
            self.update_circular(&touched.cells, report);
        }
        EditOutcome {
            revision: self.revision,
            dirty: self.dirty(),
            structure,
            sheets: sheet_tabs(&self.book),
            undo_label: self.history.undo_label(),
            redo_label: self.history.redo_label(),
            circular: self.circular_names(),
            label,
            calc_milliseconds: started.elapsed().as_secs_f64() * 1000.0,
        }
    }

    /// an incremental run only sees the loops among what it calculated:
    /// loops it no longer finds among those cells are gone
    fn update_circular(&mut self, edited: &[CellKey], report: CalcReport) {
        let calculated: std::collections::HashSet<CellKey> =
            report.changed.iter().chain(edited).copied().collect();
        self.circular.retain(|key| !calculated.contains(key));
        for key in report.circular {
            if !self.circular.contains(&key) {
                self.circular.push(key);
            }
        }
    }

    /// "Sheet1!B3" for each cell in a loop, for the circular reference notice.
    pub fn circular_names(&self) -> Vec<String> {
        self.circular
            .iter()
            .filter_map(|(sheet, row, col)| {
                let name = &self.book.sheets.get(*sheet)?.name;
                let cell = letters_sheets::CellRef::new(*row, *col).to_a1();
                Some(format!("{name}!{cell}"))
            })
            .collect()
    }

    pub fn circular_cells(&self) -> &[CellKey] {
        &self.circular
    }

    /// the status bar's sum, average and count for the selection.
    pub fn stats(&mut self, sheet: usize, ranges: &[Range]) -> Option<SelectionStats> {
        let target = self.book.sheets.get(sheet)?;
        let fresh = matches!(&self.stats, Some((s, r, _)) if *s == sheet && *r == self.revision);
        if !fresh {
            self.stats = Some((sheet, self.revision, StatsCache::new(target)));
        }
        let (_, _, cache) = self.stats.as_ref()?;
        Some(cache.stats(target, ranges))
    }

    /// the outcome as if nothing had changed, for the ui's first look.
    pub fn current(&self) -> EditOutcome {
        EditOutcome {
            revision: self.revision,
            dirty: self.dirty(),
            structure: false,
            sheets: sheet_tabs(&self.book),
            undo_label: self.history.undo_label(),
            redo_label: self.history.redo_label(),
            circular: self.circular_names(),
            label: None,
            calc_milliseconds: 0.0,
        }
    }
}
