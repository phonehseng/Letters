//! adding, renaming, moving, duplicating and deleting sheets, with every
//! formula and name that refers to them kept right.

use crate::error::SheetError;
use crate::formula::refs::{remove_sheet, rename_sheet};
use crate::model::{DefinedName, Sheet, SheetCarry, Workbook};

use super::rewrite::rewrite_all;
use super::{sheet_check, snapshot, Applied, Changed, Operation};

/// excel's rules for sheet names
pub fn check_name(book: &Workbook, name: &str, except: Option<usize>) -> Result<(), SheetError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(SheetError::Invalid("A sheet needs a name.".into()));
    }
    if trimmed.chars().count() > 31 {
        return Err(SheetError::Invalid(
            "Sheet names can be at most 31 characters long.".into(),
        ));
    }
    if trimmed.contains(['[', ']', ':', '*', '?', '/', '\\'])
        || trimmed.starts_with('\'')
        || trimmed.ends_with('\'')
    {
        return Err(SheetError::Invalid(
            "Sheet names can’t contain [ ] : * ? / \\ or start or end with '.".into(),
        ));
    }
    if trimmed.eq_ignore_ascii_case("History") {
        return Err(SheetError::Invalid(
            "“History” is a name Excel keeps for itself.".into(),
        ));
    }
    let taken =
        book.sheets.iter().enumerate().any(|(index, sheet)| {
            Some(index) != except && sheet.name.eq_ignore_ascii_case(trimmed)
        });
    if taken {
        return Err(SheetError::Invalid(format!(
            "There is already a sheet called “{trimmed}”."
        )));
    }
    Ok(())
}

/// moves sheet-level names along when sheets are inserted or removed
fn renumber_names(book: &mut Workbook, map: impl Fn(usize) -> Option<usize>) {
    book.defined_names.retain_mut(|name| match name.sheet {
        Some(index) => match map(index) {
            Some(new) => {
                name.sheet = Some(new);
                true
            }
            None => false,
        },
        None => true,
    });
}

pub fn add(book: &mut Workbook, index: usize, name: &str) -> Result<Applied, SheetError> {
    check_name(book, name, None)?;
    let index = index.min(book.sheets.len());
    book.sheets.insert(index, Sheet::new(name.trim()));
    renumber_names(book, |i| Some(if i >= index { i + 1 } else { i }));
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![Operation::RemoveSheet { index }],
    })
}

/// takes a sheet out with no other change (the undo of adding it)
pub fn remove_plain(book: &mut Workbook, index: usize) -> Result<Applied, SheetError> {
    sheet_check(book, index)?;
    let names = book.defined_names.clone();
    let sheet = book.sheets.remove(index);
    renumber_names(book, |i| match i.cmp(&index) {
        std::cmp::Ordering::Less => Some(i),
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(i - 1),
    });
    book.active_sheet = book.active_sheet.min(book.sheets.len().saturating_sub(1));
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![Operation::ReinsertSheet {
            index,
            sheet: Box::new(sheet),
            names,
        }],
    })
}

pub fn reinsert(
    book: &mut Workbook,
    index: usize,
    sheet: &Sheet,
    names: &[DefinedName],
) -> Result<Applied, SheetError> {
    let index = index.min(book.sheets.len());
    book.sheets.insert(index, sheet.clone());
    book.defined_names = names.to_vec();
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![Operation::RemoveSheet { index }],
    })
}

pub fn rename(book: &mut Workbook, sheet: usize, name: &str) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    check_name(book, name, Some(sheet))?;
    let old = book.sheets[sheet].name.clone();
    let new = name.trim().to_string();
    rewrite_all(book, &|expr, _, _| rename_sheet(expr, &old, &new));
    book.sheets[sheet].name = new;
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![Operation::RenameSheet { sheet, name: old }],
    })
}

pub fn move_sheet(book: &mut Workbook, from: usize, to: usize) -> Result<Applied, SheetError> {
    sheet_check(book, from)?;
    let to = to.min(book.sheets.len() - 1);
    let sheet = book.sheets.remove(from);
    book.sheets.insert(to, sheet);
    let position = |i: usize| -> usize {
        if i == from {
            to
        } else if from < to && i > from && i <= to {
            i - 1
        } else if to < from && i >= to && i < from {
            i + 1
        } else {
            i
        }
    };
    renumber_names(book, |i| Some(position(i)));
    book.active_sheet = position(book.active_sheet);
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![Operation::MoveSheet { from: to, to: from }],
    })
}

/// "Budget" -> "Budget (2)", as excel names a copy
fn copy_name(book: &Workbook, name: &str) -> String {
    let stem = name.rsplit_once(" (").map(|(stem, _)| stem).unwrap_or(name);
    (2..)
        .map(|n| format!("{stem} ({n})"))
        .find(|candidate| book.sheet_index(candidate).is_none() && candidate.chars().count() <= 31)
        .unwrap_or_else(|| name.to_string())
}

pub fn duplicate(book: &mut Workbook, sheet: usize) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    let mut copy = book.sheets[sheet].clone();
    copy.name = copy_name(book, &copy.name);
    // the copy is a new sheet in the file: it gets its own parts and ids
    copy.carry = SheetCarry::default();
    copy.view.selected = false;
    let next_id = book
        .sheets
        .iter()
        .flat_map(|s| s.tables.iter().map(|t| t.id))
        .max()
        .unwrap_or(0)
        + 1;
    for (offset, table) in copy.tables.iter_mut().enumerate() {
        table.id = next_id + offset as u32;
        let stem = table.name.clone();
        table.name = (2..)
            .map(|n| format!("{stem}{n}"))
            .find(|n| book.find_table(n).is_none())
            .unwrap_or(stem);
    }
    let index = sheet + 1;
    let names_before = book.defined_names.clone();
    book.sheets.insert(index, copy);
    renumber_names(book, |i| Some(if i >= index { i + 1 } else { i }));
    let local: Vec<DefinedName> = book
        .defined_names
        .iter()
        .filter(|name| name.sheet == Some(sheet))
        .map(|name| DefinedName {
            sheet: Some(index),
            ..name.clone()
        })
        .collect();
    book.defined_names.extend(local);
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![
            Operation::RemoveSheet { index },
            Operation::RestoreNames {
                names: names_before,
            },
        ],
    })
}

pub fn delete(book: &mut Workbook, sheet: usize) -> Result<Applied, SheetError> {
    sheet_check(book, sheet)?;
    if book.sheets.len() == 1 {
        return Err(SheetError::Invalid(
            "A workbook needs at least one sheet.".into(),
        ));
    }
    let name = book.sheets[sheet].name.clone();
    let names = book.defined_names.clone();
    let removed = book.sheets[sheet].clone();
    let changes = rewrite_all(book, &|expr, _, _| remove_sheet(expr, &name));
    remove_plain(book, sheet)?;
    let changes: Vec<_> = changes
        .into_iter()
        .filter_map(|mut change| {
            // places on later sheets moved down by one
            use super::rewrite::FormulaPlace::*;
            match &mut change.place {
                Cell { sheet: s, .. }
                | Conditional { sheet: s, .. }
                | Validation { sheet: s, .. }
                | TableColumn { sheet: s, .. } => {
                    if *s == sheet {
                        return None;
                    }
                    if *s > sheet {
                        *s -= 1;
                    }
                }
                Name { .. } => return None,
            }
            Some(change)
        })
        .collect();
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![
            Operation::RestoreFormulas { changes },
            Operation::ReinsertSheet {
                index: sheet,
                sheet: Box::new(removed),
                names,
            },
        ],
    })
}

/// puts a sheet back as it was (undo of structural edits)
pub fn restore(
    book: &mut Workbook,
    index: usize,
    sheet: &Sheet,
    names: &[DefinedName],
) -> Result<Applied, SheetError> {
    sheet_check(book, index)?;
    let undo = snapshot(book, index);
    book.sheets[index] = sheet.clone();
    book.defined_names = names.to_vec();
    Ok(Applied {
        changed: Changed::structure(),
        inverse: vec![undo],
    })
}
