//! the kinds of file Letters registers for, and where each one opens.
//!
//! the installer registers these types (see app/shell/tauri.conf.json,
//! `bundle.fileAssociations`); a double click, "Open With" or several
//! files on the command line then reach `route_for` here. the list below
//! and the installer's list are checked against each other in the tests.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// where a file opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenRoute {
    /// a pdf
    Viewer,
    /// a word processor document: .docx and .odt are edited as they are,
    /// .rtf and .doc are converted to a .docx beside them first
    Writer,
    /// a picture: offered as a new pdf, with text recognition if wanted
    Picture,

    /// a spreadsheet or presentation: shown as a pdf made by Office or
    /// LibreOffice, when one is installed
    OfficeAsPdf,
    Unsupported,
}

/// one registered file type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileType {
    pub extensions: &'static [&'static str],
    /// the name the system shows for the type ("PDF Document")
    pub name: &'static str,
    /// "Editor": Letters can change and save it. "Viewer": it shows it
    pub role: &'static str,
    pub route: OpenRoute,
}

/// every type the installer registers, in the order of the config.
pub const FILE_TYPES: &[FileType] = &[
    FileType {
        extensions: &["pdf"],
        name: "PDF Document",
        role: "Editor",
        route: OpenRoute::Viewer,
    },
    FileType {
        extensions: &["docx"],
        name: "Word Document",
        role: "Editor",
        route: OpenRoute::Writer,
    },
    FileType {
        extensions: &["odt"],
        name: "OpenDocument Text",
        role: "Editor",
        route: OpenRoute::Writer,
    },
    FileType {
        extensions: &["ods"],
        name: "OpenDocument Spreadsheet",
        role: "Viewer",
        route: OpenRoute::OfficeAsPdf,
    },
    FileType {
        extensions: &["odp"],
        name: "OpenDocument Presentation",
        role: "Viewer",
        route: OpenRoute::OfficeAsPdf,
    },
    FileType {
        extensions: &["rtf"],
        name: "Rich Text Document",
        role: "Viewer",
        route: OpenRoute::Writer,
    },
    FileType {
        extensions: &["png"],
        name: "PNG Picture",
        role: "Viewer",
        route: OpenRoute::Picture,
    },
    FileType {
        extensions: &["jpg", "jpeg"],
        name: "JPEG Picture",
        role: "Viewer",
        route: OpenRoute::Picture,
    },
    FileType {
        extensions: &["tif", "tiff"],
        name: "TIFF Picture",
        role: "Viewer",
        route: OpenRoute::Picture,
    },
    FileType {
        extensions: &["bmp"],
        name: "Bitmap Picture",
        role: "Viewer",
        route: OpenRoute::Picture,
    },
    FileType {
        extensions: &["gif"],
        name: "GIF Picture",
        role: "Viewer",
        route: OpenRoute::Picture,
    },
    FileType {
        extensions: &["webp"],
        name: "WebP Picture",
        role: "Viewer",
        route: OpenRoute::Picture,
    },
];

/// where a file opens, by its extension. spreadsheets and presentations
/// Letters doesn't register for (.xlsx, .pptx) still open as pdfs when
/// dropped on the window.
pub fn route_for(path: &Path) -> OpenRoute {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if let Some(found) = FILE_TYPES
        .iter()
        .find(|kind| kind.extensions.contains(&extension.as_str()))
    {
        return found.route;
    }
    match extension.as_str() {
        "xlsx" | "xls" | "pptx" | "ppt" => OpenRoute::OfficeAsPdf,
        "doc" => OpenRoute::Writer,
        _ => OpenRoute::Unsupported,
    }
}

/// every file on the command line, in order. the first argument is the
/// program itself; switches (starting with "-") are skipped. windows
/// passes several files when they are opened together from Explorer.
pub fn file_arguments(arguments: &[String]) -> Vec<PathBuf> {
    arguments
        .iter()
        .skip(1)
        .filter(|argument| !argument.starts_with('-'))
        .map(PathBuf::from)
        .collect()
}

/// the one sentence of Settings about making Letters the default app.
pub fn default_app_explanation(macos: bool) -> &'static str {
    if macos {
        "In the Finder, select a file, choose File > Get Info, pick Letters under “Open with”, then click Change All."
    } else {
        "The installer has told Windows which files Letters can open; Windows lets only you choose the default, in Default Apps, so pick Letters there for each type."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_opens_where_it_belongs() {
        assert_eq!(route_for(Path::new("essay.PDF")), OpenRoute::Viewer);
        assert_eq!(route_for(Path::new("essay.docx")), OpenRoute::Writer);
        assert_eq!(route_for(Path::new("notes.odt")), OpenRoute::Writer);
        assert_eq!(route_for(Path::new("data.ods")), OpenRoute::OfficeAsPdf);
        assert_eq!(route_for(Path::new("talk.odp")), OpenRoute::OfficeAsPdf);
        assert_eq!(route_for(Path::new("data.xlsx")), OpenRoute::OfficeAsPdf);
        assert_eq!(route_for(Path::new("old.rtf")), OpenRoute::Writer);
        for picture in [
            "a.png", "b.jpg", "c.JPEG", "d.tif", "e.tiff", "f.bmp", "g.gif", "h.webp",
        ] {
            assert_eq!(
                route_for(Path::new(picture)),
                OpenRoute::Picture,
                "{picture}"
            );
        }
        assert_eq!(route_for(Path::new("song.mp3")), OpenRoute::Unsupported);
        assert_eq!(route_for(Path::new("no extension")), OpenRoute::Unsupported);
    }

    #[test]
    fn several_files_on_the_command_line_all_open() {
        let arguments: Vec<String> = ["letters.exe", "--flag", "a.pdf", "b.docx"]
            .iter()
            .map(|a| a.to_string())
            .collect();
        assert_eq!(
            file_arguments(&arguments),
            vec![PathBuf::from("a.pdf"), PathBuf::from("b.docx")]
        );
        assert!(file_arguments(&["letters".to_string()]).is_empty());
    }

    /// the installer's list and this one must name the same extensions.
    #[test]
    fn the_installer_registers_exactly_these_types() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../../shell/tauri.conf.json")).unwrap();
        let mut registered: Vec<String> = config["bundle"]["fileAssociations"]
            .as_array()
            .expect("tauri.conf.json lists fileAssociations")
            .iter()
            .flat_map(|entry| {
                entry["ext"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| e.as_str().unwrap().to_string())
            })
            .collect();
        let mut ours: Vec<String> = FILE_TYPES
            .iter()
            .flat_map(|kind| kind.extensions.iter().map(|e| e.to_string()))
            .collect();
        registered.sort();
        ours.sort();
        assert_eq!(registered, ours);
    }

    #[test]
    fn the_default_app_sentence_is_one_sentence() {
        for macos in [true, false] {
            let sentence = default_app_explanation(macos);
            assert!(
                sentence.ends_with('.') && !sentence.contains(". "),
                "{sentence}"
            );
        }
    }
}
