//! pasting text from other apps: excel, google sheets and libreoffice
//! put a copied range on the clipboard as tab-separated text (and an html
//! table, which the ui reads into the same rows of fields). each field
//! is put in as if it were typed, so "12%" becomes a percentage and
//! "=A1*2" a formula, which is what excel does with pasted text.

use letters_sheets::address::{MAX_COLS, MAX_ROWS};
use letters_sheets::edit::Operation;

use super::history::Edit;

/// the edits that put `rows` of fields at (`row`, `col`). an empty field
/// clears its cell, as pasting does in excel.
pub fn paste_edits(
    sheet: usize,
    row: u32,
    col: u32,
    rows: &[Vec<String>],
) -> Result<Vec<Edit>, String> {
    let height = rows.len() as u64;
    let width = rows.iter().map(Vec::len).max().unwrap_or(0) as u64;
    if u64::from(row) + height > u64::from(MAX_ROWS) || u64::from(col) + width > u64::from(MAX_COLS)
    {
        return Err("The pasted cells don’t fit on the sheet from here. Select a cell further up or to the left.".into());
    }
    let mut edits = Vec::new();
    for (down, fields) in rows.iter().enumerate() {
        for (across, field) in fields.iter().enumerate() {
            edits.push(Edit::Book(Operation::SetCell {
                sheet,
                row: row + down as u32,
                col: col + across as u32,
                input: field.clone(),
            }));
        }
    }
    Ok(edits)
}

/// splits tab-separated text into rows of fields, reading excel's quotes
/// (a field with a tab, a line break or a quote is quoted, quotes doubled).
pub fn split_tab_separated(text: &str) -> Vec<Vec<String>> {
    let text = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text);
    let mut rows = vec![Vec::new()];
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' if quoted => quoted = false,
            '"' if field.is_empty() => quoted = true,
            '\t' if !quoted => end_field(&mut rows, &mut field),
            '\r' if !quoted && chars.peek() == Some(&'\n') => {}
            '\n' if !quoted => {
                end_field(&mut rows, &mut field);
                rows.push(Vec::new());
            }
            other => field.push(other),
        }
    }
    end_field(&mut rows, &mut field);
    rows
}

fn end_field(rows: &mut [Vec<String>], field: &mut String) {
    if let Some(row) = rows.last_mut() {
        row.push(std::mem::take(field));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excel_text_with_quoted_line_breaks_splits_into_cells() {
        let text = "Name\tNote\r\nAda\t\"two\nlines\"\r\nBob\t\"say \"\"hi\"\"\"\r\n";
        let rows = split_tab_separated(text);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1], vec!["Ada", "two\nlines"]);
        assert_eq!(rows[2], vec!["Bob", "say \"hi\""]);
    }

    #[test]
    fn each_field_is_typed_into_its_cell() {
        let rows = vec![vec!["1".to_string(), "=A1*2".to_string()]];
        let edits = paste_edits(0, 4, 2, &rows).unwrap();
        assert_eq!(edits.len(), 2);
        let Edit::Book(Operation::SetCell {
            row, col, input, ..
        }) = &edits[1]
        else {
            panic!("a typed cell")
        };
        assert_eq!((*row, *col, input.as_str()), (4, 3, "=A1*2"));
    }

    #[test]
    fn a_paste_that_runs_off_the_sheet_is_refused() {
        let rows = vec![vec!["a".to_string(); 3]];
        assert!(paste_edits(0, 0, MAX_COLS - 2, &rows).is_err());
    }
}
