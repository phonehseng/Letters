//! reading a picture someone chose, dropped or pasted, before it goes onto
//! a page. the engine takes png and jpeg; anything else is refused here
//! with a sentence that says what to do.

use std::fs;
use std::path::Path;

use crate::session::file_name;

/// pictures larger than this are refused before they are read: a page
/// never needs one, and reading it would take a lot of memory.
pub const MAX_PICTURE_BYTES: u64 = 50 * 1024 * 1024;

/// the kinds of picture a page can take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureKind {
    Png,
    Jpeg,
}

impl PictureKind {
    /// the extension a picture of this kind is saved with.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

/// what the bytes are, from their first few bytes rather than the name.
pub fn picture_kind(bytes: &[u8]) -> Option<PictureKind> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(PictureKind::Png)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(PictureKind::Jpeg)
    } else {
        None
    }
}

/// checks pasted or dropped bytes, with the same sentences as a file.
pub fn check_picture(bytes: &[u8], name: &str) -> Result<(), String> {
    if bytes.len() as u64 > MAX_PICTURE_BYTES {
        return Err(format!(
            "“{name}” is too large to put on a page. Use a picture under 50 MB."
        ));
    }
    if picture_kind(bytes).is_none() {
        return Err(format!(
            "“{name}” isn’t a PNG or JPEG picture. Save it as one of those, then try again. To make a PDF from other pictures, use New from Images."
        ));
    }
    Ok(())
}

/// reads a picture file for a page.
pub fn read_picture(path: &Path) -> Result<Vec<u8>, String> {
    let name = file_name(path);
    let size = fs::metadata(path)
        .map_err(|error| format!("“{name}” can’t be read. Choose the picture again. ({error})"))?
        .len();
    if size > MAX_PICTURE_BYTES {
        return Err(format!(
            "“{name}” is too large to put on a page. Use a picture under 50 MB."
        ));
    }
    let bytes = fs::read(path)
        .map_err(|error| format!("“{name}” can’t be read. Choose the picture again. ({error})"))?;
    check_picture(&bytes, &name)?;
    Ok(bytes)
}
