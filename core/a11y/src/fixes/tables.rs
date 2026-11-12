//! table header cells and their scope.

use lopdf::{Dictionary, Document, Object};
use serde::{Deserialize, Serialize};

use crate::error::A11yError;
use crate::objects::{dict_mut, ensure_editable, name, resolve, ObjectRef};
use crate::report::FixResult;
use crate::rules::{row_cells, table_rows};

use super::find_element;

/// which rows and columns of a table are headers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableHeaders {
    /// how many rows at the top are header rows (usually 1)
    pub header_rows: u32,
    /// how many columns at the left are header columns (often 0)
    pub header_columns: u32,
}

/// makes the chosen rows and columns header cells (TH) with a scope saying
/// what they head, and every other cell a data cell (TD).
pub fn set_table_headers(
    doc: &mut Document,
    table: ObjectRef,
    headers: TableHeaders,
) -> Result<FixResult, A11yError> {
    ensure_editable(doc)?;
    let (tree, index) = find_element(doc, table)?;
    if tree.elements[index].role != "Table" {
        return Err(A11yError::InvalidInput("That tag isn’t a table.".into()));
    }
    let rows = table_rows(&tree, index);
    if rows.is_empty() {
        return Err(A11yError::InvalidInput(
            "This table has no rows in its tags.".into(),
        ));
    }
    if headers.header_rows as usize > rows.len() {
        return Err(A11yError::InvalidInput(format!(
            "This table has only {} rows, so it can’t have {} header rows.",
            rows.len(),
            headers.header_rows
        )));
    }
    let mut header_count = 0;
    for (row_number, &row) in rows.iter().enumerate() {
        for (column, cell) in row_cells(&tree, row).into_iter().enumerate() {
            let id = tree.elements[cell].id.ok_or(A11yError::NoSuchElement)?;
            let in_header_row = (row_number as u32) < headers.header_rows;
            let in_header_column = (column as u32) < headers.header_columns;
            let scope = match (in_header_row, in_header_column) {
                (true, true) => Some("Both"),
                (true, false) => Some("Column"),
                (false, true) => Some("Row"),
                (false, false) => None,
            };
            if scope.is_some() {
                header_count += 1;
            }
            set_cell(doc, id, scope)?;
        }
    }
    Ok(FixResult::one(format!(
        "Made {header_count} cells of {} header cells, each saying whether it heads a column or a row.",
        tree.path(index)
    )))
}

/// makes one cell a TH with a scope, or a TD.
fn set_cell(
    doc: &mut Document,
    cell: lopdf::ObjectId,
    scope: Option<&str>,
) -> Result<(), A11yError> {
    let attributes = {
        let dict = doc
            .get_dictionary(cell)
            .map_err(|_| A11yError::NoSuchElement)?;
        let list: Vec<Object> = match dict.get(b"A").ok().map(|a| resolve(doc, a)) {
            Some(Object::Array(items)) => items
                .iter()
                .map(|item| resolve(doc, item).clone())
                .collect(),
            Some(Object::Dictionary(single)) => vec![Object::Dictionary(single.clone())],
            _ => Vec::new(),
        };
        with_scope(list, scope)
    };
    let dict = dict_mut(doc, cell)?;
    dict.set("S", name(if scope.is_some() { "TH" } else { "TD" }));
    match attributes.len() {
        0 => {
            dict.remove(b"A");
        }
        1 => dict.set("A", attributes.into_iter().next().unwrap()),
        _ => dict.set("A", Object::Array(attributes)),
    }
    Ok(())
}

/// the attribute list with the table attributes' scope set or removed.
/// other owners' attributes (layout, lists) are kept as they were.
fn with_scope(list: Vec<Object>, scope: Option<&str>) -> Vec<Object> {
    let mut found_table = false;
    let mut result: Vec<Object> = list
        .into_iter()
        .filter_map(|item| {
            let Object::Dictionary(mut dict) = item else {
                // revision numbers between attributes are not needed after this edit
                return None;
            };
            if dict.get(b"O").ok().and_then(|o| o.as_name().ok()) == Some(b"Table") {
                found_table = true;
                match scope {
                    Some(scope) => dict.set("Scope", name(scope)),
                    None => {
                        dict.remove(b"Scope");
                    }
                }
                // a table attribute with nothing but its owner says nothing
                if dict.len() <= 1 {
                    return None;
                }
            }
            Some(Object::Dictionary(dict))
        })
        .collect();
    if let (false, Some(scope)) = (found_table, scope) {
        let mut dict = Dictionary::new();
        dict.set("O", name("Table"));
        dict.set("Scope", name(scope));
        result.push(Object::Dictionary(dict));
    }
    result
}
