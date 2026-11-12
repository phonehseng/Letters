//! the picture Recents shows for a word document.
//!
//! a pdf's thumbnail is its first page, drawn by the pdf engine. Letters
//! has no layout engine for word documents, so for now they all get the
//! same plain page: a heading line and some body lines.

use std::fs;
use std::path::Path;

use crate::recents::thumbnail_name;

const GENERIC_PAGE: &[u8] = include_bytes!("generic-page.png");

/// writes the generic page as the thumbnail of `document`. a failed
/// thumbnail is never worth an error, so problems are ignored.
pub fn save_generic_thumbnail(thumbnail_dir: &Path, document: &Path) {
    let _ = fs::create_dir_all(thumbnail_dir);
    let _ = fs::write(
        thumbnail_dir.join(format!("{}.png", thumbnail_name(document))),
        GENERIC_PAGE,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_generic_page_is_a_png_saved_under_the_recents_name() {
        assert!(GENERIC_PAGE.starts_with(b"\x89PNG"));
        let dir = std::env::temp_dir().join(format!("letters-thumbs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        save_generic_thumbnail(&dir, Path::new("C:/a/essay.docx"));
        let name = format!("{}.png", thumbnail_name(Path::new("C:/a/essay.docx")));
        assert!(dir.join(name).is_file());
    }
}
