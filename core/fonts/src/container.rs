//! reading a font file from disk safely: size limits, telling the formats
//! apart by their first bytes (never by the name alone), and decoding
//! WOFF and WOFF2 into the plain font the rest of the crate reads.

use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::FontError;
use crate::sfnt;

/// fonts the person adds are refused above this size. real fonts are
/// almost always under 20 MB; the largest CJK fonts are about 30.
pub const MAX_CUSTOM_FONT_BYTES: u64 = 64 * 1024 * 1024;
/// installed fonts are trusted a little more, but not without limit.
pub const MAX_SYSTEM_FONT_BYTES: u64 = 128 * 1024 * 1024;

/// the file formats Letters reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Format {
    TrueType,
    OpenType,
    Collection,
    Woff,
    Woff2,
}

impl Format {
    /// the format the first bytes say, whatever the file's name claims.
    pub fn sniff(data: &[u8]) -> Option<Format> {
        match data.get(0..4)? {
            [0, 1, 0, 0] | b"true" => Some(Format::TrueType),
            b"OTTO" => Some(Format::OpenType),
            b"ttcf" => Some(Format::Collection),
            b"wOFF" => Some(Format::Woff),
            b"wOF2" => Some(Format::Woff2),
            _ => None,
        }
    }

    /// the extension the decoded font is kept under.
    pub fn plain_extension(plain: &[u8]) -> &'static str {
        match Format::sniff(plain) {
            Some(Format::OpenType) => "otf",
            Some(Format::Collection) => "ttc",
            _ => "ttf",
        }
    }
}

/// true for the names of files that may be fonts.
pub fn has_font_extension(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "ttf" | "otf" | "ttc" | "otc" | "woff" | "woff2"
    )
}

/// the file's name for messages, without the folders.
pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// reads at most `limit` bytes of a file, refusing anything larger
/// without reading it all.
pub fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, FontError> {
    let name = display_name(path);
    let file = std::fs::File::open(path).map_err(|_| FontError::CannotRead(name.clone()))?;
    let size = file
        .metadata()
        .map_err(|_| FontError::CannotRead(name.clone()))?
        .len();
    if size > limit {
        return Err(FontError::TooLarge {
            name,
            megabytes: size.div_ceil(1024 * 1024),
            limit: limit / (1024 * 1024),
        });
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FontError::CannotRead(name.clone()))?;
    if bytes.len() as u64 > limit {
        return Err(FontError::TooLarge {
            name,
            megabytes: bytes.len().div_ceil(1024 * 1024) as u64,
            limit: limit / (1024 * 1024),
        });
    }
    Ok(bytes)
}

/// the plain font (TrueType, OpenType or a collection) in `data`,
/// decoding WOFF and WOFF2. `name` is only for messages.
pub fn to_plain_font(data: Vec<u8>, name: &str) -> Result<Vec<u8>, FontError> {
    match Format::sniff(&data) {
        Some(Format::TrueType | Format::OpenType | Format::Collection) => Ok(data),
        Some(Format::Woff) => {
            crate::woff::decode(&data).ok_or_else(|| FontError::Damaged(name.to_string()))
        }
        Some(Format::Woff2) => {
            crate::woff2::decode(&data).ok_or_else(|| FontError::Damaged(name.to_string()))
        }
        None => Err(FontError::NotAFont(name.to_string())),
    }
}

/// every face in a plain font file that ttf-parser accepts, by index.
pub fn parseable_faces(plain: &[u8]) -> Vec<u32> {
    (0..sfnt::face_count(plain))
        .filter(|index| ttf_parser::Face::parse(plain, *index).is_ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_come_from_the_first_bytes() {
        assert_eq!(Format::sniff(&[0, 1, 0, 0, 9]), Some(Format::TrueType));
        assert_eq!(Format::sniff(b"OTTO...."), Some(Format::OpenType));
        assert_eq!(Format::sniff(b"wOF2"), Some(Format::Woff2));
        assert_eq!(Format::sniff(b"%PDF-1.7"), None);
        assert_eq!(Format::sniff(b"ab"), None);
    }

    #[test]
    fn a_renamed_text_file_is_not_a_font() {
        let error = to_plain_font(b"hello there".to_vec(), "essay.ttf").unwrap_err();
        assert_eq!(
            error.to_string(),
            "“essay.ttf” isn’t a font file Letters can use. Choose a .ttf, .otf, .ttc, .woff or .woff2 file."
        );
    }

    #[test]
    fn large_files_are_refused_before_reading() {
        let folder =
            std::env::temp_dir().join(format!("letters-fonts-limit-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("big.ttf");
        std::fs::write(&path, vec![0u8; 3 * 1024 * 1024]).unwrap();
        let error = read_limited(&path, 2 * 1024 * 1024).unwrap_err();
        assert!(matches!(
            error,
            FontError::TooLarge {
                megabytes: 3,
                limit: 2,
                ..
            }
        ));
        std::fs::remove_dir_all(&folder).ok();
    }
}
