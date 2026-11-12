//! form data files: which kind a file is from its name, reading one the
//! person chose, and merging many filled copies of a form into one table.
//!
//! the engine (core/forms' data module) reads and writes the formats;
//! this is only the part that touches files, so it can be tested with them.

use std::path::Path;

use letters_forms::{merge_form_data, DataFormat, FilledCopy, MergedData, SkippedField};

use super::files::read_chosen_file;
use crate::session::file_name;

/// a data file of answers is small; anything bigger is the wrong file.
pub const MAX_DATA_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// a filled form can hold scans; this keeps one huge file from stalling a merge.
pub const MAX_FILLED_COPY_BYTES: u64 = 256 * 1024 * 1024;

/// the format a data file is written in, from its extension.
pub fn data_format_for(path: &Path) -> Option<DataFormat> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match extension.as_str() {
        "fdf" => Some(DataFormat::Fdf),
        "xfdf" => Some(DataFormat::Xfdf),
        "csv" => Some(DataFormat::Csv),
        "json" => Some(DataFormat::Json),
        _ => None,
    }
}

/// the sentence for a file whose kind Letters doesn't know.
pub fn unknown_kind(path: &Path) -> String {
    format!(
        "“{}” isn’t a form data file Letters reads. Choose an FDF, XFDF, CSV or JSON file.",
        file_name(path)
    )
}

/// reads a data file the person chose, with its format.
pub fn read_data_file(path: &Path) -> Result<(DataFormat, Vec<u8>), String> {
    let format = data_format_for(path).ok_or_else(|| unknown_kind(path))?;
    let bytes = read_chosen_file(path, MAX_DATA_FILE_BYTES)?;
    Ok((format, bytes))
}

/// reads every filled pdf and puts their answers in one table. a file
/// that can't be read is listed in `skipped` with why, like the engine
/// does for a pdf it can't understand.
pub fn merge_filled_files(paths: &[&Path]) -> MergedData {
    let mut copies = Vec::new();
    let mut unreadable = Vec::new();
    for path in paths {
        match read_chosen_file(path, MAX_FILLED_COPY_BYTES) {
            Ok(bytes) => copies.push(FilledCopy {
                label: file_name(path),
                bytes,
            }),
            Err(reason) => unreadable.push(SkippedField {
                field: file_name(path),
                reason,
            }),
        }
    }
    let mut merged = merge_form_data(&copies);
    merged.skipped.extend(unreadable);
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_pdf::{temp_dir, write_pdf};
    use letters_forms::{add_field, export_data, fill_field, FieldValue, NewField, NewFieldKind};
    use lopdf::Document;

    fn filled_form(path: &Path, name: &str) {
        write_pdf(path, 1);
        let mut document = Document::load(path).unwrap();
        let field = NewField {
            name: "student".into(),
            page: 0,
            rect: letters_forms::VisibleRect {
                x: 72.0,
                y: 72.0,
                width: 200.0,
                height: 20.0,
            },
            kind: NewFieldKind::Text {
                max_length: None,
                comb: false,
            },
            tooltip: None,
            required: false,
            read_only: false,
            font_size: 0.0,
            alignment: letters_forms::Alignment::Left,
        };
        add_field(&mut document, &field).unwrap();
        fill_field(&mut document, "student", &FieldValue::Text(name.into())).unwrap();
        document.save(path).unwrap();
    }

    #[test]
    fn the_kind_of_data_file_comes_from_its_name() {
        assert_eq!(data_format_for(Path::new("a.FDF")), Some(DataFormat::Fdf));
        assert_eq!(data_format_for(Path::new("a.xfdf")), Some(DataFormat::Xfdf));
        assert_eq!(data_format_for(Path::new("a.csv")), Some(DataFormat::Csv));
        assert_eq!(data_format_for(Path::new("a.json")), Some(DataFormat::Json));
        assert_eq!(data_format_for(Path::new("a.txt")), None);
        assert!(read_data_file(Path::new("answers.txt"))
            .unwrap_err()
            .contains("Choose an FDF, XFDF, CSV or JSON file."));
    }

    #[test]
    fn many_filled_copies_merge_into_one_table_and_unreadable_ones_are_listed() {
        let dir = temp_dir("merge-copies");
        let ada = dir.join("ada.pdf");
        let grace = dir.join("grace.pdf");
        filled_form(&ada, "Ada");
        filled_form(&grace, "Grace");
        let missing = dir.join("missing.pdf");

        let merged = merge_filled_files(&[&ada, &grace, &missing]);

        assert_eq!(merged.columns, vec!["File", "student"]);
        assert_eq!(
            merged.rows,
            vec![vec!["ada.pdf", "Ada"], vec!["grace.pdf", "Grace"]]
        );
        assert_eq!(merged.skipped.len(), 1);
        assert_eq!(merged.skipped[0].field, "missing.pdf");
        assert!(merged.csv.starts_with("File,student"));
    }

    #[test]
    fn exported_answers_read_back_as_the_same_kind() {
        let dir = temp_dir("data-file");
        let form = dir.join("form.pdf");
        filled_form(&form, "Ada");
        let json = dir.join("answers.json");
        let document = Document::load(&form).unwrap();
        std::fs::write(&json, export_data(&document, DataFormat::Json).unwrap()).unwrap();

        let (format, bytes) = read_data_file(&json).unwrap();

        assert_eq!(format, DataFormat::Json);
        assert!(String::from_utf8(bytes).unwrap().contains("Ada"));
    }
}
