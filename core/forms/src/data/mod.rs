//! form data in and out: fdf and xfdf (the formats pdf apps exchange
//! answers in), csv and json (for spreadsheets and scripts), and many
//! filled copies of one form gathered into one csv table.

mod fdf;
mod table;
mod xfdf;

pub use table::{merge_form_data, FilledCopy, MergedData};

use lopdf::Document;
use serde::{Deserialize, Serialize};

use crate::error::FormResult;
use crate::fill::FieldValue;
use crate::logic::{fill_without_recalculating, form_logic, recalculate, FillReport};
use crate::objects::check_document;
use crate::read::{fields, FieldInfo, FieldKind};
use fdf::{read_fdf, write_fdf};
use table::{csv_cell, csv_row, decode_csv, read_csv_single, read_json, write_json};
use xfdf::{read_xfdf, write_xfdf};

/// a form data format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataFormat {
    /// forms data format (iso 32000 12.7.8), the binary-safe pdf-like one
    Fdf,
    /// xml forms data format (iso 19444-1)
    Xfdf,
    /// a header row of field names and one row of answers
    Csv,
    /// one object: field name to answer (text, or a list for multi-select)
    Json,
}

/// one field's answer as data: one value, several (multi-select lists),
/// or none.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Answer {
    One(String),
    Many(Vec<String>),
    Empty,
}

/// the answers of a form, in form order. checkboxes and radio groups give
/// their export value or "Off"; signatures and buttons are left out.
pub(super) fn answers(doc: &Document) -> FormResult<Vec<(FieldInfo, Answer)>> {
    let mut out = Vec::new();
    for info in fields(doc)? {
        let answer = match info.kind {
            FieldKind::Signature | FieldKind::Button => continue,
            FieldKind::Checkbox | FieldKind::RadioGroup => {
                Answer::One(info.value.clone().unwrap_or_else(|| "Off".to_owned()))
            }
            FieldKind::List if info.multi_select && info.selected.len() > 1 => {
                Answer::Many(info.selected.clone())
            }
            _ => match &info.value {
                Some(value) => Answer::One(value.clone()),
                None => Answer::Empty,
            },
        };
        out.push((info, answer));
    }
    Ok(out)
}

/// writes the form's answers in a data format.
pub fn export_data(doc: &Document, format: DataFormat) -> FormResult<Vec<u8>> {
    check_document(doc)?;
    let answers = answers(doc)?;
    Ok(match format {
        DataFormat::Fdf => write_fdf(&answers),
        DataFormat::Xfdf => write_xfdf(&answers).into_bytes(),
        DataFormat::Csv => {
            let header: Vec<String> = answers.iter().map(|(info, _)| info.name.clone()).collect();
            let row: Vec<String> = answers.iter().map(|(_, answer)| csv_cell(answer)).collect();
            format!("{}\r\n{}\r\n", csv_row(&header), csv_row(&row)).into_bytes()
        }
        DataFormat::Json => write_json(&answers).into_bytes(),
    })
}

/// a tree of names, so "address.city" is written as a field "address"
/// with a kid "city", as fdf and xfdf expect.
#[derive(Default)]
pub(super) struct NameTree {
    pub(super) answer: Option<(FieldKind, Answer)>,
    pub(super) kids: Vec<(String, NameTree)>,
}

impl NameTree {
    pub(super) fn build(answers: &[(FieldInfo, Answer)]) -> Self {
        let mut root = NameTree::default();
        for (info, answer) in answers {
            let mut node = &mut root;
            for part in info.name.split('.') {
                let position = match node.kids.iter().position(|(name, _)| name == part) {
                    Some(position) => position,
                    None => {
                        node.kids.push((part.to_owned(), NameTree::default()));
                        node.kids.len() - 1
                    }
                };
                node = &mut node.kids[position].1;
            }
            node.answer = Some((info.kind, answer.clone()));
        }
        root
    }
}

/// a field that was in the data but couldn't be filled.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedField {
    pub field: String,
    pub reason: String,
}

/// what importing data did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub filled: Vec<String>,
    pub skipped: Vec<SkippedField>,
    /// the calculations worked out after filling
    pub calculation: FillReport,
}

/// fills the form from data. each answer goes through the field's own
/// format and checks; ones that fail, unknown names and calculated fields
/// are skipped and listed. the calculations run once at the end.
pub fn import_data(
    doc: &mut Document,
    data: &[u8],
    format: DataFormat,
) -> FormResult<ImportReport> {
    check_document(doc)?;
    let pairs = match format {
        DataFormat::Fdf => read_fdf(data)?,
        DataFormat::Xfdf => read_xfdf(&String::from_utf8_lossy(data))?,
        // excel writes csv in the computer's code page, often with ';' (table.rs)
        DataFormat::Csv => read_csv_single(&decode_csv(data))?,
        DataFormat::Json => read_json(data)?,
    };
    let logic = form_logic(doc)?;
    let infos = fields(doc)?;
    let mut report = ImportReport {
        filled: Vec::new(),
        skipped: Vec::new(),
        calculation: FillReport {
            changed: Vec::new(),
            issues: Vec::new(),
            custom_script_note: None,
        },
    };
    for (name, answer) in pairs {
        let skip = |reason: &str| SkippedField {
            field: name.clone(),
            reason: reason.to_owned(),
        };
        let Some(info) = infos.iter().find(|info| info.name == name) else {
            report
                .skipped
                .push(skip("This form has no field with that name."));
            continue;
        };
        if logic
            .fields
            .iter()
            .any(|field| field.field == name && field.calculation.is_some())
        {
            report
                .skipped
                .push(skip("It is calculated from other fields."));
            continue;
        }
        // an empty answer for a field that is already empty changes nothing,
        // and writing it would turn "no answer" into "an empty answer"
        let answer_is_empty = matches!(&answer, Answer::Empty)
            || matches!(&answer, Answer::One(text) if text.is_empty());
        if answer_is_empty && info.value.is_none() {
            continue;
        }
        let Some(value) = value_for(info, answer) else {
            report
                .skipped
                .push(skip("Signatures and buttons hold no answers."));
            continue;
        };
        match fill_without_recalculating(doc, &logic, &name, &value) {
            Ok(_) => report.filled.push(name),
            Err(error) => report.skipped.push(skip(&error.to_string())),
        }
    }
    report.calculation = recalculate(doc)?;
    Ok(report)
}

fn value_for(info: &FieldInfo, answer: Answer) -> Option<FieldValue> {
    let one = match &answer {
        Answer::One(value) => value.clone(),
        Answer::Many(values) => values.first().cloned().unwrap_or_default(),
        Answer::Empty => String::new(),
    };
    Some(match info.kind {
        FieldKind::Signature | FieldKind::Button => return None,
        FieldKind::Checkbox => {
            let lower = one.trim().to_ascii_lowercase();
            if lower.is_empty() || lower == "off" || lower == "false" || lower == "no" {
                FieldValue::Checked(false)
            } else if info
                .widgets
                .iter()
                .any(|w| w.on_value.as_deref() == Some(one.as_str()))
            {
                FieldValue::Text(one)
            } else {
                FieldValue::Checked(true)
            }
        }
        FieldKind::RadioGroup if one.is_empty() || one == "Off" => FieldValue::Checked(false),
        FieldKind::List | FieldKind::Dropdown => match answer {
            Answer::Many(values) => FieldValue::Selected(values),
            Answer::Empty => FieldValue::Selected(Vec::new()),
            Answer::One(value) if info.multi_select && value.contains("; ") => {
                FieldValue::Selected(value.split("; ").map(str::to_owned).collect())
            }
            Answer::One(value) if value.is_empty() => FieldValue::Selected(Vec::new()),
            Answer::One(value) => FieldValue::Selected(vec![value]),
        },
        _ => FieldValue::Text(one),
    })
}

/// the first column of a merged table: which file each row came from.
pub const MERGED_FILE_COLUMN: &str = "File";
