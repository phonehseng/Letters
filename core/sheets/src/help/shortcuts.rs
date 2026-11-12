//! keyboard shortcuts as data, for the operations letters has, in the
//! excel (windows and mac) and google sheets key sets, so the ui can offer
//! either set and show the keys next to every command.
//!
//! sources: microsoft's "keyboard shortcuts in excel"
//! (support.microsoft.com/office/1798d9d5-842a-42b8-9c99-9b7213f0040f)
//! and google's "keyboard shortcuts for google sheets"
//! (support.google.com/docs/answer/181110, the pc list). every row was
//! compared with both pages on 12 november 2026; `UNCONFIRMED` lists the
//! rows where a page doesn't name one of the keys, with the reason.

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub operation: &'static str,
    pub excel_windows: &'static str,
    pub excel_mac: &'static str,
    pub google_sheets: &'static str,
    pub description: &'static str,
    /// where the families use the same keys for different things
    #[serde(skip_serializing_if = "str::is_empty")]
    pub conflict: &'static str,
    /// false until compared with the cited pages
    pub checked: bool,
}

/// rows a published page doesn't confirm in full, and why:
/// - moveToEdge, selectToEdge: google's list has no ctrl with the arrows
/// - editCell: google's list has no key for editing a cell
/// - lineBreak: google's list names no key for a line break in a cell
/// - fillRight, toggleFilter, replace, print: microsoft's windows tables
///   don't list ctrl r, ctrl shift l, ctrl h or ctrl p (its mac and web
///   tables do)
/// - autoSum, formatCells, flashFill, clearContents: not in google's list
/// - merge: neither page lists a key for merging cells
const UNCONFIRMED: &[&str] = &[
    "moveToEdge",
    "selectToEdge",
    "editCell",
    "lineBreak",
    "fillRight",
    "toggleFilter",
    "replace",
    "print",
    "autoSum",
    "formatCells",
    "flashFill",
    "clearContents",
    "merge",
];

/// (operation, excel windows, excel mac, google sheets, description, conflict)
const TABLE: &[(&str, &str, &str, &str, &str, &str)] = &[
    (
        "moveToEdge",
        "Ctrl+Arrow",
        "Cmd+Arrow",
        "Ctrl+Arrow",
        "Jump to the edge of the data in that direction",
        "",
    ),
    (
        "selectToEdge",
        "Ctrl+Shift+Arrow",
        "Cmd+Shift+Arrow",
        "Ctrl+Shift+Arrow",
        "Select to the edge of the data",
        "",
    ),
    (
        "goToStart",
        "Ctrl+Home",
        "Ctrl+Home",
        "Ctrl+Home",
        "Go to cell A1",
        "",
    ),
    (
        "goToEnd",
        "Ctrl+End",
        "Ctrl+End",
        "Ctrl+End",
        "Go to the last used cell",
        "",
    ),
    (
        "selectColumn",
        "Ctrl+Space",
        "Ctrl+Space",
        "Ctrl+Space",
        "Select the whole column",
        "",
    ),
    (
        "selectRow",
        "Shift+Space",
        "Shift+Space",
        "Shift+Space",
        "Select the whole row",
        "",
    ),
    (
        "selectAll",
        "Ctrl+A",
        "Cmd+A",
        "Ctrl+A",
        "Select the data block, then everything",
        "",
    ),
    (
        "editCell",
        "F2",
        "F2",
        "Enter",
        "Edit the active cell",
        "Google Sheets' list has no key to edit a cell; Enter and F2 both start editing there",
    ),
    (
        "cycleReference",
        "F4",
        "Cmd+T or F4",
        "F4",
        "While editing: cycle a reference through $A$1, A$1, $A1, A1",
        "Outside a formula F4 repeats the last action in Excel and redoes in Google Sheets",
    ),
    (
        "fillDown",
        "Ctrl+D",
        "Cmd+D",
        "Ctrl+D",
        "Copy the top cell of the selection down",
        "",
    ),
    (
        "fillRight",
        "Ctrl+R",
        "Cmd+R",
        "Ctrl+R",
        "Copy the left cell of the selection right",
        "",
    ),
    (
        "fillSelection",
        "Ctrl+Enter",
        "Ctrl+Return or Cmd+Return",
        "Ctrl+Enter",
        "Put what was typed into every selected cell",
        "",
    ),
    (
        "lineBreak",
        "Alt+Enter",
        "Ctrl+Option+Return",
        "Ctrl+Enter",
        "A new line inside the cell",
        "Google Sheets' list gives Ctrl+Enter as filling the range and Alt+Enter as opening a link",
    ),
    (
        "insertDate",
        "Ctrl+;",
        "Ctrl+;",
        "Ctrl+;",
        "Type today's date",
        "",
    ),
    (
        "insertTime",
        "Ctrl+Shift+;",
        "Cmd+;",
        "Ctrl+Shift+;",
        "Type the time now",
        "",
    ),
    (
        "toggleFilter",
        "Ctrl+Shift+L",
        "Cmd+Shift+F",
        "Ctrl+Alt+Shift+F",
        "Turn filter buttons on or off",
        "Ctrl+Shift+L aligns left in Google Sheets",
    ),
    (
        "makeTable",
        "Ctrl+T",
        "Cmd+T",
        "Ctrl+Alt+T",
        "Turn the data into a table",
        "Ctrl+T opens a new browser tab, so Sheets uses Ctrl+Alt+T",
    ),
    (
        "autoSum",
        "Alt+=",
        "Cmd+Shift+T",
        "Alt+Shift+=",
        "Insert a SUM of the numbers above or to the left",
        "",
    ),
    (
        "formatCells",
        "Ctrl+1",
        "Cmd+1",
        "Ctrl+/ then Format menu",
        "Open the format dialog",
        "Google Sheets has no format dialog; Ctrl+/ lists shortcuts",
    ),
    (
        "formatCurrency",
        "Ctrl+Shift+$",
        "Ctrl+Shift+$",
        "Ctrl+Shift+4",
        "Currency format",
        "",
    ),
    (
        "formatPercent",
        "Ctrl+Shift+%",
        "Ctrl+Shift+%",
        "Ctrl+Shift+5",
        "Percent format",
        "",
    ),
    (
        "formatDate",
        "Ctrl+Shift+#",
        "Ctrl+Shift+#",
        "Ctrl+Shift+3",
        "Date format",
        "",
    ),
    (
        "formatNumber",
        "Ctrl+Shift+!",
        "Ctrl+Shift+!",
        "Ctrl+Shift+1",
        "Number format with two decimals",
        "",
    ),
    (
        "formatTime",
        "Ctrl+Shift+@",
        "Ctrl+Shift+@",
        "Ctrl+Shift+2",
        "Time format",
        "",
    ),
    (
        "insertCells",
        "Ctrl+Shift+Plus",
        "Ctrl+Shift+=",
        "Ctrl+Alt+=",
        "Insert rows, columns or cells",
        "",
    ),
    (
        "deleteCells",
        "Ctrl+Minus",
        "Cmd+-",
        "Ctrl+Alt+-",
        "Delete rows, columns or cells",
        "",
    ),
    (
        "hideRows",
        "Ctrl+9",
        "Ctrl+9",
        "Ctrl+Alt+9",
        "Hide the selected rows",
        "",
    ),
    (
        "hideColumns",
        "Ctrl+0",
        "Ctrl+0",
        "Ctrl+Alt+0",
        "Hide the selected columns",
        "",
    ),
    (
        "nextSheet",
        "Ctrl+PageDown",
        "Option+Right",
        "Alt+Down",
        "Go to the next sheet",
        "",
    ),
    (
        "previousSheet",
        "Ctrl+PageUp",
        "Option+Left",
        "Alt+Up",
        "Go to the previous sheet",
        "",
    ),
    (
        "showFormulas",
        "Ctrl+`",
        "Ctrl+`",
        "Ctrl+~",
        "Show formulas instead of results",
        "",
    ),
    (
        "recalculate",
        "F9",
        "F9",
        "",
        "Calculate now",
        "Google Sheets always calculates and has no key for it",
    ),
    ("bold", "Ctrl+B", "Cmd+B", "Ctrl+B", "Bold", ""),
    ("italic", "Ctrl+I", "Cmd+I", "Ctrl+I", "Italic", ""),
    ("underline", "Ctrl+U", "Cmd+U", "Ctrl+U", "Underline", ""),
    ("undo", "Ctrl+Z", "Cmd+Z", "Ctrl+Z", "Undo", ""),
    ("redo", "Ctrl+Y", "Cmd+Y", "Ctrl+Y", "Redo", ""),
    ("copy", "Ctrl+C", "Cmd+C", "Ctrl+C", "Copy", ""),
    ("cut", "Ctrl+X", "Cmd+X", "Ctrl+X", "Cut (move)", ""),
    ("paste", "Ctrl+V", "Cmd+V", "Ctrl+V", "Paste", ""),
    (
        "pasteValues",
        "Ctrl+Alt+V then V",
        "Ctrl+Cmd+V then V",
        "Ctrl+Shift+V",
        "Paste only the values",
        "",
    ),
    ("find", "Ctrl+F", "Ctrl+F", "Ctrl+F", "Find", ""),
    (
        "replace",
        "Ctrl+H",
        "Ctrl+H",
        "Ctrl+H",
        "Find and replace",
        "",
    ),
    (
        "flashFill",
        "Ctrl+E",
        "Ctrl+E",
        "Ctrl+Shift+Y",
        "Fill by example",
        "",
    ),
    (
        "insertNote",
        "Shift+F2",
        "Shift+F2",
        "Shift+F2",
        "Add or edit a note",
        "",
    ),
    ("insertLink", "Ctrl+K", "Cmd+K", "Ctrl+K", "Add a link", ""),
    (
        "merge",
        "Alt+H, M, M",
        "",
        "",
        "Merge cells (ribbon keys)",
        "No default key on Mac or in Google Sheets",
    ),
    (
        "save",
        "Ctrl+S",
        "Cmd+S",
        "",
        "Save",
        "Google Sheets saves by itself",
    ),
    ("print", "Ctrl+P", "Cmd+P", "Ctrl+P", "Print", ""),
    (
        "newSheet",
        "Shift+F11",
        "Shift+F11",
        "Shift+F11",
        "Insert a sheet",
        "",
    ),
    (
        "clearContents",
        "Delete",
        "Delete",
        "Delete",
        "Clear the selected cells",
        "",
    ),
];

pub fn shortcuts() -> Vec<Shortcut> {
    TABLE
        .iter()
        .map(
            |(operation, excel_windows, excel_mac, google_sheets, description, conflict)| {
                Shortcut {
                    operation,
                    excel_windows,
                    excel_mac,
                    google_sheets,
                    description,
                    conflict,
                    checked: !UNCONFIRMED.contains(operation),
                }
            },
        )
        .collect()
}

/// the keys for an operation in a key set ("excelWindows", "excelMac", "googleSheets")
pub fn keys_for(operation: &str, set: &str) -> Option<&'static str> {
    let row = TABLE.iter().find(|row| row.0 == operation)?;
    let keys = match set {
        "excelMac" => row.2,
        "googleSheets" => row.3,
        _ => row.1,
    };
    (!keys.is_empty()).then_some(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_appears_once() {
        let all = shortcuts();
        let mut names: Vec<&str> = all.iter().map(|s| s.operation).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), all.len());
        assert_eq!(keys_for("autoSum", "excelWindows"), Some("Alt+="));
        assert_eq!(keys_for("recalculate", "googleSheets"), None);
    }

    #[test]
    fn rows_the_published_pages_confirm_are_marked_checked() {
        let all = shortcuts();
        let row = |name: &str| all.iter().find(|s| s.operation == name).unwrap();
        assert!(row("fillDown").checked);
        assert!(!row("merge").checked);
        assert!(UNCONFIRMED.iter().all(|name| all.iter().any(|s| s.operation == *name)));
        assert_eq!(keys_for("nextSheet", "googleSheets"), Some("Alt+Down"));
        assert_eq!(keys_for("insertTime", "excelMac"), Some("Cmd+;"));
    }
}
