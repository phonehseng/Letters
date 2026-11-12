//! the media table: each picture's bytes, stored once.

use serde::{Deserialize, Serialize};

/// one picture file inside the document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    /// what images refer to; also the file name it is saved under
    pub id: String,
    /// "image/png", "image/jpeg"...
    pub content_type: String,
    /// where the picture lived in the file it came from
    /// ("word/media/image1.png"), so it is saved back to the same place
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// the picture's bytes; base64 in json
    #[serde(with = "crate::base64::serde_bytes")]
    pub bytes: Vec<u8>,
}

impl MediaItem {
    /// a content type for a file name's extension, for pictures added
    /// by the interface or found without one.
    pub fn content_type_for(file_name: &str) -> &'static str {
        let extension = file_name
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match extension.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" | "jpe" => "image/jpeg",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "tif" | "tiff" => "image/tiff",
            "svg" => "image/svg+xml",
            "emf" => "image/x-emf",
            "wmf" => "image/x-wmf",
            "webp" => "image/webp",
            _ => "application/octet-stream",
        }
    }
}
