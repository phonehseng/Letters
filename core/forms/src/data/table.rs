//! csv and json form data, and many filled copies merged into one csv
//! table with a row per file.

use lopdf::Document;
use serde::{Deserialize, Serialize};

use super::{answers, SkippedField};
use super::{Answer, MERGED_FILE_COLUMN};
use crate::error::{FormError, FormResult};
use crate::read::FieldInfo;

pub(super) fn csv_cell(answer: &Answer) -> String {
    match answer {
        Answer::One(value) => value.clone(),
        Answer::Many(values) => values.join("; "),
        Answer::Empty => String::new(),
    }
}

pub(super) fn csv_row(cells: &[String]) -> String {
    cells
        .iter()
        .map(|cell| {
            if cell.contains([',', '"', '\n', '\r']) || cell.starts_with(' ') || cell.ends_with(' ')
            {
                format!("\"{}\"", cell.replace('"', "\"\""))
            } else {
                cell.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn json_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_owned())
}

pub(super) fn write_json(answers: &[(FieldInfo, Answer)]) -> String {
    let entries: Vec<String> = answers
        .iter()
        .map(|(info, answer)| {
            let value = match answer {
                Answer::One(value) => json_string(value),
                Answer::Many(values) => format!(
                    "[{}]",
                    values
                        .iter()
                        .map(|v| json_string(v))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                Answer::Empty => "null".to_owned(),
            };
            format!("  {}: {value}", json_string(&info.name))
        })
        .collect();
    format!("{{\n{}\n}}\n", entries.join(",\n"))
}

/// reads rfc 4180 csv into rows of cells.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cell.push(c);
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() => quoted = true,
            ',' => row.push(std::mem::take(&mut cell)),
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            other => cell.push(other),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
}

/// a csv with a header row and one row of answers (the first, if more).
pub(super) fn read_csv_single(text: &str) -> FormResult<Vec<(String, Answer)>> {
    let rows = parse_csv(text);
    let [header, values, ..] = rows.as_slice() else {
        return Err(FormError::BadFormData(
            "a CSV file needs a row of field names and a row of answers".into(),
        ));
    };
    // a "File" column from a merged table isn't a field
    Ok(header
        .iter()
        .zip(values.iter().chain(std::iter::repeat(&String::new())))
        .filter(|(name, _)| !name.is_empty() && name.as_str() != MERGED_FILE_COLUMN)
        .map(|(name, value)| (name.clone(), Answer::One(value.clone())))
        .collect())
}

pub(super) fn read_json(data: &[u8]) -> FormResult<Vec<(String, Answer)>> {
    let value: serde_json::Value =
        serde_json::from_slice(data).map_err(|error| FormError::BadFormData(error.to_string()))?;
    let serde_json::Value::Object(map) = value else {
        return Err(FormError::BadFormData(
            "it must be one object of field names and answers".into(),
        ));
    };
    let text = |value: &serde_json::Value| match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(true) => "true".to_owned(),
        serde_json::Value::Bool(false) => "Off".to_owned(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    };
    Ok(map
        .iter()
        .map(|(name, value)| {
            let answer = match value {
                serde_json::Value::Array(items) => Answer::Many(items.iter().map(text).collect()),
                serde_json::Value::Null => Answer::Empty,
                other => Answer::One(text(other)),
            };
            (name.clone(), answer)
        })
        .collect())
}

/// one filled copy of a form, for merging.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilledCopy {
    /// shown in the File column, usually the file name
    pub label: String,
    pub bytes: Vec<u8>,
}

/// the answers of many filled pdfs as one table.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MergedData {
    /// "File" and then every field name seen, in first-seen order
    pub columns: Vec<String>,
    /// one row per file that could be read
    pub rows: Vec<Vec<String>>,
    /// the table as csv
    pub csv: String,
    /// files that couldn't be read, with why
    pub skipped: Vec<SkippedField>,
}

/// gathers the answers of many filled pdfs (of one form, or several) into
/// one csv table with a row per file.
pub fn merge_form_data(copies: &[FilledCopy]) -> MergedData {
    let mut columns = vec![MERGED_FILE_COLUMN.to_owned()];
    let mut records: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut skipped = Vec::new();
    for copy in copies {
        let read = Document::load_mem(&copy.bytes)
            .map_err(FormError::from)
            .and_then(|doc| answers(&doc));
        match read {
            Ok(answers) => {
                let mut record = Vec::new();
                for (info, answer) in answers {
                    if !columns.contains(&info.name) {
                        columns.push(info.name.clone());
                    }
                    record.push((info.name, csv_cell(&answer)));
                }
                records.push((copy.label.clone(), record));
            }
            Err(error) => skipped.push(SkippedField {
                field: copy.label.clone(),
                reason: error.to_string(),
            }),
        }
    }
    let rows: Vec<Vec<String>> = records
        .into_iter()
        .map(|(label, record)| {
            let mut row = vec![label];
            for column in &columns[1..] {
                row.push(
                    record
                        .iter()
                        .find(|(name, _)| name == column)
                        .map(|(_, value)| value.clone())
                        .unwrap_or_default(),
                );
            }
            row
        })
        .collect();
    let mut csv = csv_row(&columns);
    csv.push_str("\r\n");
    for row in &rows {
        csv.push_str(&csv_row(row));
        csv.push_str("\r\n");
    }
    MergedData {
        columns,
        rows,
        csv,
        skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_quotes_round_trip() {
        let cells = vec![
            "plain".to_owned(),
            "a, b".to_owned(),
            "say \"hi\"".to_owned(),
            "two\nlines".to_owned(),
        ];
        let text = format!("{}\r\n", csv_row(&cells));
        assert_eq!(parse_csv(&text), vec![cells]);
    }
}
