//! edits and the undo history of one workbook.
//!
//! an edit is either one of the engine's operations or a size change
//! (layout.rs). applying an edit returns the edits that undo it; a step
//! is everything one action did ("Paste", "Sort"), so Undo takes back
//! the whole action at once, as excel does.

use letters_sheets::calc::CellKey;
use letters_sheets::edit::{self, Operation};
use letters_sheets::Workbook;
use serde::{Deserialize, Serialize};

use super::layout::{apply_layout, LayoutChange};

/// one change to a workbook.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "edit", rename_all = "camelCase")]
pub enum Edit {
    Book(Operation),
    Layout(LayoutChange),
}

/// what applying some edits changed, for recalculation and redrawing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Touched {
    pub cells: Vec<CellKey>,
    /// rows, columns or sheets moved: the calculation engine is rebuilt
    pub structure: bool,
    /// only sizes or hidden rows changed: nothing to calculate
    pub layout_only: bool,
}

impl Touched {
    fn add(&mut self, other: Touched) {
        self.cells.extend(other.cells);
        self.structure |= other.structure;
        self.layout_only &= other.layout_only;
    }
}

/// filters and freezing hide rows or keep them in place; formulas don't
/// read either, so the engine needn't be rebuilt for them
fn moves_no_cells(operation: &Operation) -> bool {
    matches!(
        operation,
        Operation::Filter { .. } | Operation::ClearFilter { .. } | Operation::Freeze { .. }
    )
}

/// applies one edit; returns its undo and what it touched.
pub fn apply_edit(book: &mut Workbook, edit: &Edit) -> Result<(Vec<Edit>, Touched), String> {
    match edit {
        Edit::Book(operation) => {
            let applied = edit::apply(book, operation).map_err(|error| error.to_string())?;
            let layout_only = moves_no_cells(operation);
            let touched = Touched {
                cells: applied.changed.cells,
                structure: applied.changed.structure && !layout_only,
                layout_only,
            };
            Ok((
                applied.inverse.into_iter().map(Edit::Book).collect(),
                touched,
            ))
        }
        Edit::Layout(change) => {
            let undo = apply_layout(book, change)?;
            let touched = Touched {
                layout_only: true,
                ..Touched::default()
            };
            Ok((vec![Edit::Layout(undo)], touched))
        }
    }
}

/// applies edits in order. if one fails, the ones before it are taken
/// back, so the workbook is never left half changed.
pub fn apply_all(book: &mut Workbook, edits: &[Edit]) -> Result<(Vec<Edit>, Touched), String> {
    let mut undo_lists: Vec<Vec<Edit>> = Vec::with_capacity(edits.len());
    let mut touched = Touched {
        layout_only: true,
        ..Touched::default()
    };
    for edit in edits {
        match apply_edit(book, edit) {
            Ok((undo, this)) => {
                undo_lists.push(undo);
                touched.add(this);
            }
            Err(error) => {
                let undo = joined_in_reverse(undo_lists);
                // the undo of edits that just worked can't fail; if it
                // did, there would be nothing better to do than report
                // the first problem
                let _ = apply_all(book, &undo);
                return Err(error);
            }
        }
    }
    Ok((joined_in_reverse(undo_lists), touched))
}

/// the undo of a list of edits: each edit's undo, last edit first.
fn joined_in_reverse(undo_lists: Vec<Vec<Edit>>) -> Vec<Edit> {
    undo_lists.into_iter().rev().flatten().collect()
}

/// one action as undo sees it.
#[derive(Clone, Debug)]
pub struct Step {
    /// "Typing", "Paste", "Sort": shown as "Undo Paste"
    pub label: String,
    /// applied in order, these take the action back (or redo it)
    pub edits: Vec<Edit>,
}

/// the undo and redo stacks of one workbook.
#[derive(Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

/// how many actions undo remembers; sorts and structure edits keep a
/// whole sheet each, so the history is bounded
const MOST_STEPS: usize = 200;

impl History {
    /// a new action: its undo is remembered and redo is forgotten.
    pub fn record(&mut self, step: Step) {
        self.undo.push(step);
        if self.undo.len() > MOST_STEPS {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn take_undo(&mut self) -> Option<Step> {
        self.undo.pop()
    }

    pub fn take_redo(&mut self) -> Option<Step> {
        self.redo.pop()
    }

    pub fn push_undo(&mut self, step: Step) {
        self.undo.push(step);
    }

    pub fn push_redo(&mut self, step: Step) {
        self.redo.push(step);
    }

    pub fn undo_label(&self) -> Option<String> {
        self.undo.last().map(|step| step.label.clone())
    }

    pub fn redo_label(&self) -> Option<String> {
        self.redo.last().map(|step| step.label.clone())
    }
}
