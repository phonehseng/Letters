//! rewriting every formula in a workbook when cells move: cell formulas,
//! defined names, conditional format rules, validation rules and table
//! column formulas. each change is remembered so undo can put the old
//! text back exactly.

use serde::{Deserialize, Serialize};

use crate::address::CellRef;
use crate::formula::{self, Expr};
use crate::model::Workbook;

use super::{Applied, Changed};

/// where a formula lives
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FormulaPlace {
    Cell {
        sheet: usize,
        row: u32,
        col: u32,
    },
    Name {
        index: usize,
    },
    Conditional {
        sheet: usize,
        format: usize,
        rule: usize,
        formula: usize,
    },
    Validation {
        sheet: usize,
        index: usize,
        second: bool,
    },
    TableColumn {
        sheet: usize,
        table: usize,
        column: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaChange {
    pub place: FormulaPlace,
    pub old: String,
}

/// a change to apply to each parsed formula: given the tree, the name
/// of the sheet the formula is on and, for a cell formula, its cell, the
/// new tree or none
pub type Rewrite<'a> = dyn Fn(&Expr, &str, Option<CellRef>) -> Option<Expr> + 'a;

fn rewritten(
    text: &str,
    sheet_name: &str,
    at: Option<CellRef>,
    change: &Rewrite,
) -> Option<String> {
    let expr = formula::parse(text).ok()?;
    change(&expr, sheet_name, at).map(|new| formula::to_string(&new))
}

/// applies `change` to every formula, returning what it changed
pub fn rewrite_all(book: &mut Workbook, change: &Rewrite) -> Vec<FormulaChange> {
    let mut changes = Vec::new();
    for sheet_index in 0..book.sheets.len() {
        let name = book.sheets[sheet_index].name.clone();
        rewrite_sheet(book, sheet_index, &name, change, &mut changes);
    }
    for index in 0..book.defined_names.len() {
        let defined = &book.defined_names[index];
        let home = defined
            .sheet
            .and_then(|s| book.sheets.get(s))
            .map(|s| s.name.clone())
            .unwrap_or_default();
        if let Some(new) = rewritten(&defined.formula, &home, None, change) {
            let old = std::mem::replace(&mut book.defined_names[index].formula, new);
            changes.push(FormulaChange {
                place: FormulaPlace::Name { index },
                old,
            });
        }
    }
    changes
}

fn rewrite_sheet(
    book: &mut Workbook,
    sheet_index: usize,
    name: &str,
    change: &Rewrite,
    changes: &mut Vec<FormulaChange>,
) {
    let sheet = &mut book.sheets[sheet_index];
    for (row, col, cell) in sheet.cells.iter_mut() {
        let Some(formula) = cell.formula.as_mut() else {
            continue;
        };
        if let Some(new) = rewritten(&formula.text, name, Some(CellRef::new(row, col)), change) {
            let old = std::mem::replace(&mut formula.text, new);
            changes.push(FormulaChange {
                place: FormulaPlace::Cell {
                    sheet: sheet_index,
                    row,
                    col,
                },
                old,
            });
        }
    }
    for (f, format) in sheet.conditional_formats.iter_mut().enumerate() {
        for (r, rule) in format.rules.iter_mut().enumerate() {
            for (i, text) in rule.formulas.iter_mut().enumerate() {
                if let Some(new) = rewritten(text, name, None, change) {
                    let old = std::mem::replace(text, new);
                    changes.push(FormulaChange {
                        place: FormulaPlace::Conditional {
                            sheet: sheet_index,
                            format: f,
                            rule: r,
                            formula: i,
                        },
                        old,
                    });
                }
            }
        }
    }
    for (index, rule) in sheet.validations.iter_mut().enumerate() {
        for (second, slot) in [(false, &mut rule.formula1), (true, &mut rule.formula2)] {
            let Some(text) = slot.as_mut() else { continue };
            if let Some(new) = rewritten(text, name, None, change) {
                let old = std::mem::replace(text, new);
                changes.push(FormulaChange {
                    place: FormulaPlace::Validation {
                        sheet: sheet_index,
                        index,
                        second,
                    },
                    old,
                });
            }
        }
    }
    for (t, table) in sheet.tables.iter_mut().enumerate() {
        for (c, column) in table.columns.iter_mut().enumerate() {
            let Some(text) = column.calculated_formula.as_mut() else {
                continue;
            };
            if let Some(new) = rewritten(text, name, None, change) {
                let old = std::mem::replace(text, new);
                changes.push(FormulaChange {
                    place: FormulaPlace::TableColumn {
                        sheet: sheet_index,
                        table: t,
                        column: c,
                    },
                    old,
                });
            }
        }
    }
}

/// the slot a place names, if it still exists
fn slot<'a>(book: &'a mut Workbook, place: &FormulaPlace) -> Option<&'a mut String> {
    match *place {
        FormulaPlace::Cell { sheet, row, col } => book
            .sheets
            .get_mut(sheet)?
            .cells
            .get_mut(row, col)?
            .formula
            .as_mut()
            .map(|f| &mut f.text),
        FormulaPlace::Name { index } => book.defined_names.get_mut(index).map(|n| &mut n.formula),
        FormulaPlace::Conditional {
            sheet,
            format,
            rule,
            formula,
        } => book
            .sheets
            .get_mut(sheet)?
            .conditional_formats
            .get_mut(format)?
            .rules
            .get_mut(rule)?
            .formulas
            .get_mut(formula),
        FormulaPlace::Validation {
            sheet,
            index,
            second,
        } => {
            let rule = book.sheets.get_mut(sheet)?.validations.get_mut(index)?;
            if second {
                rule.formula2.as_mut()
            } else {
                rule.formula1.as_mut()
            }
        }
        FormulaPlace::TableColumn {
            sheet,
            table,
            column,
        } => book
            .sheets
            .get_mut(sheet)?
            .tables
            .get_mut(table)?
            .columns
            .get_mut(column)?
            .calculated_formula
            .as_mut(),
    }
}

/// puts formulas back; returns the inverse (the texts it replaced)
pub fn restore(book: &mut Workbook, changes: &[FormulaChange]) -> Applied {
    let mut inverse = Vec::new();
    let mut cells = Vec::new();
    for change in changes.iter().rev() {
        if let Some(text) = slot(book, &change.place) {
            let current = std::mem::replace(text, change.old.clone());
            inverse.push(FormulaChange {
                place: change.place.clone(),
                old: current,
            });
        }
        if let FormulaPlace::Cell { sheet, row, col } = change.place {
            cells.push((sheet, row, col));
        }
    }
    Applied {
        changed: Changed {
            cells,
            structure: false,
        },
        inverse: vec![super::Operation::RestoreFormulas { changes: inverse }],
    }
}
