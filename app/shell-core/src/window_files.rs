//! which file each window shows, so opening a file that is already open
//! brings its window forward instead of opening a second copy.
//!
//! two windows on one file can each save over the other's changes (the
//! disk stamp in disk_stamp.rs catches that, but only at save); Word,
//! Excel and Preview all answer a second open with the window already
//! showing the file, and so does Letters.
//!
//! paths are compared under the system's own rules (letters_search's
//! `path_key`): "Report.PDF" and "report.pdf" are one file on windows and
//! on a case-insensitive mac volume, two on linux.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use letters_search::index::path_key;

#[derive(Default)]
pub struct WindowFiles {
    /// window label to the key of the file it shows
    shown: HashMap<String, String>,
}

impl WindowFiles {
    /// the window now shows `file`, or nothing (the home screen).
    pub fn show(&mut self, window: &str, file: Option<&Path>) {
        match file {
            Some(file) => {
                self.shown.insert(window.to_string(), key(file));
            }
            None => {
                self.shown.remove(window);
            }
        }
    }

    /// the window closed.
    pub fn forget(&mut self, window: &str) {
        self.shown.remove(window);
    }

    /// a window other than `except` that shows `file`, if there is one.
    pub fn window_showing(&self, file: &Path, except: Option<&str>) -> Option<String> {
        let wanted = key(file);
        let mut found: Vec<&String> = self
            .shown
            .iter()
            .filter(|(window, shown)| **shown == wanted && Some(window.as_str()) != except)
            .map(|(window, _)| window)
            .collect();
        // the same answer every time if (by an earlier version) two windows show it
        found.sort();
        found.first().map(|window| (*window).clone())
    }
}

fn key(file: &Path) -> String {
    // a relative path from the command line means the same file as its full path
    let full: PathBuf = std::path::absolute(file).unwrap_or_else(|_| file.to_path_buf());
    path_key(&full.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_window_already_showing_a_file() {
        let mut files = WindowFiles::default();
        let report = Path::new("/school/Lab Report.pdf");
        files.show("window-1", Some(report));
        files.show("window-2", Some(Path::new("/school/Essay.docx")));
        assert_eq!(files.window_showing(report, None).as_deref(), Some("window-1"));
        // a window asking about its own file isn't sent to itself
        assert_eq!(files.window_showing(report, Some("window-1")), None);
        assert_eq!(files.window_showing(Path::new("/school/Other.pdf"), None), None);
    }

    #[test]
    fn forgets_windows_that_closed_or_went_home() {
        let mut files = WindowFiles::default();
        let report = Path::new("/school/Lab Report.pdf");
        files.show("window-1", Some(report));
        files.show("window-1", None);
        assert_eq!(files.window_showing(report, None), None);
        files.show("window-2", Some(report));
        files.forget("window-2");
        assert_eq!(files.window_showing(report, None), None);
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_capitals_and_slashes_name_the_same_file() {
        let mut files = WindowFiles::default();
        files.show("window-1", Some(Path::new(r"C:\School\Lab Report.pdf")));
        assert_eq!(
            files.window_showing(Path::new("c:/school/lab report.PDF"), None).as_deref(),
            Some("window-1")
        );
    }
}
