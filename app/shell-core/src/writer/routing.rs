//! which screen a file opens in, decided by its name.
//!
//! pdfs go to the viewer, word processor documents to the writer, and
//! anything else gets a plain "not supported yet" instead of an error
//! from deep inside an engine.

use std::path::Path;

use letters_office::FileKind;
use serde::Serialize;

/// where a file goes when it is opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileRoute {
    Viewer,
    Writer,
    Unsupported,
}

/// the lower-case extension of a path, without the dot.
fn extension(path: &Path) -> String {
    path.extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn route_for_path(path: &Path) -> FileRoute {
    match extension(path).as_str() {
        "pdf" => FileRoute::Viewer,
        // rich text and old word files open by conversion (document.rs)
        "docx" | "odt" | "rtf" | "doc" => FileRoute::Writer,
        _ => FileRoute::Unsupported,
    }
}

/// the format a writer document is saved in, chosen by the name it is
/// saved under. `None` for names that are neither docx nor odt.
pub fn save_kind_for_path(path: &Path) -> Option<FileKind> {
    match extension(path).as_str() {
        "docx" => Some(FileKind::Docx),
        "odt" => Some(FileKind::Odt),
        _ => None,
    }
}

/// a plain sentence for a file the app can't open yet.
pub fn unsupported_message(path: &Path) -> String {
    let name = crate::session::file_name(path);
    match extension(path).as_str() {
        "doc" => format!("“{name}” is an older Word document. Open it in Word and save it as .docx, then open that here."),
        "txt" | "pages" | "xlsx" | "pptx" | "ods" | "odp" => {
            format!("Letters can’t open “{name}” yet. It opens PDFs and Word (.docx) or OpenDocument (.odt) documents.")
        }
        _ => format!("Letters can’t open “{name}”. It opens PDFs and Word (.docx) or OpenDocument (.odt) documents."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_go_to_the_screen_for_their_type() {
        assert_eq!(
            route_for_path(Path::new("C:/a/essay.pdf")),
            FileRoute::Viewer
        );
        assert_eq!(
            route_for_path(Path::new("C:/a/Essay.PDF")),
            FileRoute::Viewer
        );
        assert_eq!(route_for_path(Path::new("essay.docx")), FileRoute::Writer);
        assert_eq!(route_for_path(Path::new("notes.ODT")), FileRoute::Writer);
        assert_eq!(route_for_path(Path::new("old.doc")), FileRoute::Writer);
        assert_eq!(route_for_path(Path::new("notes.rtf")), FileRoute::Writer);
        assert_eq!(
            route_for_path(Path::new("no extension")),
            FileRoute::Unsupported
        );
    }

    #[test]
    fn the_saved_format_follows_the_name() {
        assert_eq!(
            save_kind_for_path(Path::new("a.docx")),
            Some(FileKind::Docx)
        );
        assert_eq!(save_kind_for_path(Path::new("a.Odt")), Some(FileKind::Odt));
        assert_eq!(save_kind_for_path(Path::new("a.pdf")), None);
    }

    #[test]
    fn unsupported_files_get_a_sentence_that_says_what_to_do() {
        assert!(unsupported_message(Path::new("old.doc")).contains("save it as .docx"));
        assert!(unsupported_message(Path::new("sheet.xlsx")).contains("yet"));
    }
}
