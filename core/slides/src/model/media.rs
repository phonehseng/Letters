//! the media table: each picture, video, sound and embedded font's bytes,
//! stored once however often they are shown (the same shape as
//! letters-office's media item, so the two can merge).

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaItem {
    /// what objects refer to; also the file name it is saved under
    pub id: String,
    /// "image/png", "video/mp4"...
    pub content_type: String,
    /// where the item lived in the file it came from
    /// ("ppt/media/image1.png"), so it is saved back to the same place
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// the bytes; base64 in json
    #[serde(with = "crate::base64::serde_bytes")]
    pub bytes: Vec<u8>,
}

impl MediaItem {
    /// a content type for a file name's extension.
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
            "mp4" => "video/mp4",
            "mp3" => "audio/mpeg",
            "wav" => "audio/wav",
            _ => "application/octet-stream",
        }
    }
}
