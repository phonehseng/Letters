//! delimited text (csv, tsv, semicolon files from european excel): the
//! encoding and the delimiter are worked out from the bytes, fields follow
//! rfc 4180 (quotes, doubled quotes, line breaks inside quotes), and
//! values are read the way typing them into a cell would read them.

use serde::{Deserialize, Serialize};

use crate::address::{MAX_COLS, MAX_ROWS};
use crate::error::SheetError;
use crate::model::{Cell, Value, Workbook};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvOptions {
    /// none: detect it
    pub delimiter: Option<char>,
    /// read numbers, dates and TRUE/FALSE; false keeps every field as text
    pub convert_values: bool,
    /// "007" stays text instead of becoming 7 (excel drops the zeros,
    /// which ruins postcodes and ids)
    pub keep_leading_zeros: bool,
}

impl Default for CsvOptions {
    fn default() -> CsvOptions {
        CsvOptions {
            delimiter: None,
            convert_values: true,
            keep_leading_zeros: true,
        }
    }
}

/// the text of a file: utf-8 or utf-16 with a byte order mark, utf-8
/// without one, or else windows-1252 (what excel on windows writes)
pub fn decode(bytes: &[u8]) -> String {
    if let Ok(text) = crate::xml::decode_text(bytes) {
        return text;
    }
    bytes
        .iter()
        .filter_map(|b| crate::calc::functions::windows_1252(*b))
        .collect()
}

/// the delimiter that splits the first lines into the same number of fields
pub fn detect_delimiter(text: &str) -> char {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(20)
        .collect();
    let mut best = (',', 0usize);
    for candidate in [',', ';', '\t', '|'] {
        let counts: Vec<usize> = lines
            .iter()
            .map(|line| split_line(line, candidate).len())
            .collect();
        let Some(first) = counts.first().copied() else {
            continue;
        };
        let consistent = counts.iter().filter(|c| **c == first).count();
        // a delimiter that gives more than one field on most lines wins
        let score = if first > 1 {
            consistent * 100 + first
        } else {
            0
        };
        if score > best.1 {
            best = (candidate, score);
        }
    }
    best.0
}

/// the fields of one line (no line breaks inside quotes)
fn split_line(line: &str, delimiter: char) -> Vec<String> {
    parse_records(line, delimiter)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// every record and field, quotes and line breaks inside them handled
pub fn parse_records(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            '"' if quoted => quoted = false,
            '"' if field.is_empty() => quoted = true,
            c if c == delimiter && !quoted => record.push(std::mem::take(&mut field)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            c => field.push(c),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

fn field_value(field: &str, book: &mut Workbook, options: &CsvOptions) -> Result<Cell, SheetError> {
    if !options.convert_values || field.is_empty() {
        return Ok(Cell::value(if field.is_empty() {
            Value::Blank
        } else {
            Value::text(field)
        }));
    }
    let leading_zero = field.len() > 1
        && field.starts_with('0')
        && field.chars().nth(1).is_some_and(|c| c.is_ascii_digit());
    // a field starting with "=" stays text: opening a csv must never run formulas
    if field.starts_with('=') || (options.keep_leading_zeros && leading_zero) {
        return Ok(Cell::value(Value::text(field)));
    }
    let typed = crate::edit::input::read_input(field, book.date_system)?;
    let style = match &typed.number_format {
        Some(code) => book.styles.derive(0, |s| s.number_format = code.clone()),
        None => 0,
    };
    Ok(Cell {
        value: typed.value,
        style,
        ..Cell::default()
    })
}

pub fn read_csv(bytes: &[u8], options: &CsvOptions) -> Result<Workbook, SheetError> {
    let text = decode(bytes);
    let delimiter = options.delimiter.unwrap_or_else(|| detect_delimiter(&text));
    let mut book = Workbook::new();
    for (row, record) in parse_records(&text, delimiter).into_iter().enumerate() {
        if row as u32 >= MAX_ROWS {
            return Err(SheetError::TooLarge(format!("more than {MAX_ROWS} rows")));
        }
        for (col, field) in record.into_iter().enumerate().take(MAX_COLS as usize) {
            let cell = field_value(&field, &mut book, options)?;
            if !cell.is_empty() {
                book.sheets[0].cells.set(row as u32, col as u32, cell);
            }
        }
    }
    Ok(book)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvWriteOptions {
    pub delimiter: char,
    /// write what cells show (formatted) instead of their raw values
    pub shown_values: bool,
    /// start with a utf-8 byte order mark so excel reads accents right
    pub byte_order_mark: bool,
}

impl Default for CsvWriteOptions {
    fn default() -> CsvWriteOptions {
        CsvWriteOptions {
            delimiter: ',',
            shown_values: true,
            byte_order_mark: true,
        }
    }
}

fn quote(field: &str, delimiter: char) -> String {
    let needs = field.contains(delimiter)
        || field.contains('"')
        || field.contains('\n')
        || field.contains('\r');
    if needs {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// one sheet as delimited text
pub fn write_csv(book: &Workbook, sheet: usize, options: &CsvWriteOptions) -> Vec<u8> {
    let mut out = String::new();
    if options.byte_order_mark {
        out.push('\u{feff}');
    }
    let Some(target) = book.sheets.get(sheet) else {
        return out.into_bytes();
    };
    let Some(used) = target.cells.used_range() else {
        return out.into_bytes();
    };
    for row in 0..=used.end.row {
        let mut fields = Vec::new();
        for col in 0..=used.end.col {
            let text = match target.cells.get(row, col) {
                None => String::new(),
                Some(cell) if options.shown_values => crate::view::shown(book, cell),
                Some(cell) => crate::calc::value::to_text(&cell.value)
                    .unwrap_or_else(|e| e.as_str().to_string()),
            };
            fields.push(quote(&text, options.delimiter));
        }
        out.push_str(&fields.join(&options.delimiter.to_string()));
        out.push_str("\r\n");
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quotes_delimiters_and_values() {
        let text = "name;amount;date\n\"Smith, Jo\";1,5;3/1/2024\n\"say \"\"hi\"\"\";007;=1+1\n";
        assert_eq!(detect_delimiter(text), ';');
        let book = read_csv(text.as_bytes(), &CsvOptions::default()).unwrap();
        assert_eq!(book.value_at("A2"), Value::text("Smith, Jo"));
        assert_eq!(book.value_at("A3"), Value::text("say \"hi\""));
        assert_eq!(book.value_at("B3"), Value::text("007"));
        assert_eq!(book.value_at("C3"), Value::text("=1+1"));
        assert_eq!(book.value_at("C2"), Value::Number(45352.0));
        let tabs = read_csv(b"a\tb\n1\t2\n", &CsvOptions::default()).unwrap();
        assert_eq!(tabs.value_at("B2"), Value::Number(2.0));
    }

    #[test]
    fn encodings_are_detected() {
        assert_eq!(decode(b"caf\xe9"), "café");
        assert_eq!(decode("café".as_bytes()), "café");
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend("é,1".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode(&utf16), "é,1");
    }

    #[test]
    fn writes_quoted_fields() {
        let book = read_csv(b"a,\"b,c\"\n1,\"x\"\"y\"\n", &CsvOptions::default()).unwrap();
        let options = CsvWriteOptions {
            byte_order_mark: false,
            ..CsvWriteOptions::default()
        };
        assert_eq!(
            String::from_utf8(write_csv(&book, 0, &options)).unwrap(),
            "a,\"b,c\"\r\n1,\"x\"\"y\"\r\n"
        );
    }
}
