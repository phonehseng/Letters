//! "Export as Code File…": a word document (or a pdf read as one) written
//! as LaTeX, reStructuredText, AsciiDoc, a json outline or csv tables.
//!
//! the office model is first flattened into a few simple blocks (headings,
//! paragraphs, list items, code, tables, pictures; see simple.rs), and each
//! writer renders those. markdown, html and plain text come from
//! letters-office's own writers and are not repeated here.

pub mod asciidoc;
pub mod csv;
pub mod files;
pub mod latex;
pub mod rst;
pub mod simple;
pub mod structure_json;

use serde::{Deserialize, Serialize};

/// the formats this module writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CodeExportFormat {
    Markdown,
    Html,
    PlainText,
    Latex,
    RestructuredText,
    AsciiDoc,
    Json,
    Csv,
}

/// a written file and the pictures it refers to, under "images/".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WrittenText {
    pub text: String,
    /// (relative path, bytes)
    pub files: Vec<(String, Vec<u8>)>,
}

#[cfg(test)]
pub(crate) mod test_document {
    use letters_office::model::Document;

    /// a short document with each thing the writers handle.
    pub fn sample() -> Document {
        letters_office::import::from_html(
            "<h1>Lab Report</h1><p>Some <b>bold</b>, <i>italic</i> and <a href=\"https://example.org\">a link</a>.</p>\
             <h2>Steps</h2><ul><li>first</li><li>second</li></ul><ol><li>one</li><li>two</li></ol>\
             <table><tr><td>name</td><td>value</td></tr><tr><td>a, b</td><td>2</td></tr></table>\
             <p>50% done &amp; $5 #1</p>",
        )
    }
}
