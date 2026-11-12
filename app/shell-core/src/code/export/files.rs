//! writes an export to disk: the text file the person chose, and beside it
//! an "images" folder for pictures and extra csv files for extra tables.
//! the document itself and its own file never change.

use std::fs;
use std::path::{Path, PathBuf};

use letters_office::model::Document;
use serde::Serialize;

use super::{asciidoc, csv, latex, rst, structure_json, CodeExportFormat, WrittenText};
use crate::session::file_name;
use crate::writer::save::write_atomically;

/// what was written, for the sentence after the export.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeExported {
    pub path: String,
    /// every file written, the chosen one first
    pub files: Vec<String>,
    pub sentence: String,
}

/// the text and picture files of `document` in `format`.
pub fn render(document: &Document, format: CodeExportFormat) -> Result<Vec<WrittenText>, String> {
    let written = match format {
        CodeExportFormat::Markdown => {
            let markdown = letters_office::markdown::to_markdown(document);
            WrittenText {
                text: markdown.text,
                files: markdown.files,
            }
        }
        CodeExportFormat::Html => plain(letters_office::export::to_html(document)),
        CodeExportFormat::PlainText => plain(letters_office::export::to_plain_text(document)),
        CodeExportFormat::Latex => latex::to_latex(document),
        CodeExportFormat::RestructuredText => rst::to_rst(document),
        CodeExportFormat::AsciiDoc => asciidoc::to_asciidoc(document),
        CodeExportFormat::Json => structure_json::to_structure_json(document),
        CodeExportFormat::Csv => return csv_files(document),
    };
    Ok(vec![written])
}

fn plain(text: String) -> WrittenText {
    WrittenText {
        text,
        files: Vec::new(),
    }
}

fn csv_files(document: &Document) -> Result<Vec<WrittenText>, String> {
    let tables = csv::tables_to_csv(document);
    if tables.is_empty() {
        return Err("This document has no tables, so there is nothing to save as CSV.".into());
    }
    Ok(tables.into_iter().map(plain).collect())
}

/// writes `document` as `format` to `path`. a second table goes to
/// "name (table 2).csv" beside it, pictures to "images/" beside it.
pub fn export_document(
    document: &Document,
    format: CodeExportFormat,
    path: &Path,
) -> Result<CodeExported, String> {
    let rendered = render(document, format)?;
    let folder = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut written: Vec<PathBuf> = Vec::new();
    for (index, part) in rendered.iter().enumerate() {
        let destination = numbered_path(path, index);
        write_atomically(part.text.as_bytes(), &destination)?;
        written.push(destination);
        for (relative, bytes) in &part.files {
            written.push(write_picture(&folder, relative, bytes)?);
        }
    }
    Ok(CodeExported {
        path: path.to_string_lossy().into_owned(),
        files: written
            .iter()
            .map(|file| file.to_string_lossy().into_owned())
            .collect(),
        sentence: summary(path, rendered.len(), written.len()),
    })
}

fn numbered_path(path: &Path, index: usize) -> PathBuf {
    if index == 0 {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{stem} (table {}).{extension}", index + 1))
}

fn write_picture(folder: &Path, relative: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let destination = folder.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    write_atomically(bytes, &destination)?;
    Ok(destination)
}

fn summary(path: &Path, text_files: usize, all_files: usize) -> String {
    let name = file_name(path);
    let pictures = all_files - text_files;
    let mut sentence = format!("Saved “{name}”.");
    if text_files > 1 {
        sentence.push_str(&format!(
            " Each of the {text_files} tables has its own file beside it."
        ));
    }
    if pictures == 1 {
        sentence.push_str(" Its picture is in the “images” folder beside it.");
    } else if pictures > 1 {
        sentence.push_str(&format!(
            " Its {pictures} pictures are in the “images” folder beside it."
        ));
    }
    sentence.push_str(" The document itself didn’t change.");
    sentence
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::export::test_document::sample;

    #[test]
    fn a_latex_export_is_written_to_the_chosen_file() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("report.tex");
        let exported = export_document(&sample(), CodeExportFormat::Latex, &path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\\section{Lab Report}"));
        assert_eq!(exported.files.len(), 1);
        assert!(exported.sentence.starts_with("Saved “report.tex”."));
    }

    #[test]
    fn markdown_comes_from_the_office_writer() {
        let rendered = render(&sample(), CodeExportFormat::Markdown).unwrap();
        assert!(rendered[0].text.starts_with("# Lab Report"));
    }

    #[test]
    fn csv_without_tables_says_so() {
        let error = render(&Document::new(), CodeExportFormat::Csv).unwrap_err();
        assert!(error.contains("no tables"));
    }

    #[test]
    fn a_second_table_gets_a_numbered_name() {
        let second = numbered_path(Path::new("/home/sam/data.csv"), 1);
        assert_eq!(second, Path::new("/home/sam/data (table 2).csv"));
    }
}
