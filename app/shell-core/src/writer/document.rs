//! one open word processor document, as the shell keeps it.
//!
//! the editor in the ui works on the document model as json. two parts of
//! the model never travel to the ui: `carry` (what the file had that the
//! model does not understand, needed only for writing the file back) and
//! the bytes of the pictures (which can be megabytes, fetched one at a
//! time when a picture is shown). the shell keeps both here and puts them
//! back into the model the ui sends when it saves.

use std::path::{Path, PathBuf};

use letters_office::model::{Carry, Document, MediaItem};
use letters_office::{FileKind, OfficeError};
use serde_json::Value;

use crate::writer::routing::save_kind_for_path;

pub struct WriterDocument {
    /// where save writes to; changes after save as
    pub path: PathBuf,
    /// the format save writes
    pub kind: FileKind,
    /// the format the document was read from, which decides what saving
    /// in the other format would lose
    pub source_kind: FileKind,
    carry: Carry,
    media: Vec<MediaItem>,
    added_pictures: u32,
}

impl WriterDocument {
    /// reads a .docx or .odt file. the format comes from the content, so a
    /// misnamed file still opens. rich text and old .doc files open through
    /// the engine's `open_any` (a .doc needs word or libreoffice); they are
    /// then saved as a .docx beside the original, which stays untouched.
    pub fn open(path: &Path) -> Result<(WriterDocument, Document), OfficeError> {
        let bytes = std::fs::read(path).map_err(|error| {
            OfficeError::NotAPackage(format!("the file can’t be read: {error}"))
        })?;
        match letters_office::detect(&bytes) {
            Ok(kind) => {
                let document = letters_office::open(&bytes)?;
                Ok((
                    WriterDocument::from_document(path, kind, &document),
                    document,
                ))
            }
            Err(OfficeError::LegacyWordFormat) | Err(OfficeError::NotAPackage(_))
                if is_converted_kind(path, &bytes) =>
            {
                let document = letters_office::formats::open_any(&bytes)?;
                let docx = path.with_extension("docx");
                Ok((
                    WriterDocument::from_document(&docx, FileKind::Docx, &document),
                    document,
                ))
            }
            Err(error) => Err(error),
        }
    }

    /// keeps what the ui does not get from a document already read.
    pub fn from_document(path: &Path, kind: FileKind, document: &Document) -> WriterDocument {
        WriterDocument {
            path: path.to_path_buf(),
            kind: save_kind_for_path(path).unwrap_or(kind),
            source_kind: kind,
            carry: document.carry.clone(),
            media: document.media.clone(),
            added_pictures: 0,
        }
    }

    /// the picture with this id as a `data:` address the web view can show.
    pub fn media_data_url(&self, id: &str) -> Option<String> {
        let item = self.media.iter().find(|item| item.id == id)?;
        Some(format!(
            "data:{};base64,{}",
            item.content_type,
            letters_office::base64::encode(&item.bytes)
        ))
    }

    /// adds a picture file's bytes and returns the new media id.
    pub fn add_picture(&mut self, file_name: &str, bytes: Vec<u8>) -> String {
        self.added_pictures += 1;
        let extension = file_name
            .rsplit('.')
            .next()
            .unwrap_or("png")
            .to_ascii_lowercase();
        let mut id = format!("letters-picture-{}.{extension}", self.added_pictures);
        while self.media.iter().any(|item| item.id == id) {
            self.added_pictures += 1;
            id = format!("letters-picture-{}.{extension}", self.added_pictures);
        }
        self.media.push(MediaItem {
            id: id.clone(),
            content_type: MediaItem::content_type_for(file_name).to_string(),
            source_path: None,
            bytes,
        });
        id
    }

    /// turns the json the ui sent back into a whole document, with the
    /// carry and the picture bytes put back.
    pub fn merge(&self, ui_model: Value) -> Result<Document, String> {
        let mut document: Document = serde_json::from_value(ui_model).map_err(|error| {
            format!("The document couldn’t be read back from the editor. ({error})")
        })?;
        document.carry = self.carry.clone();
        document.media = self.media_to_keep(&document);
        document.normalize_sections();
        Ok(document)
    }

    /// every picture the file came with (preserved xml may still point at
    /// one the model does not show) plus the added ones still in use.
    fn media_to_keep(&self, document: &Document) -> Vec<MediaItem> {
        let used = used_media_ids(document);
        self.media
            .iter()
            .filter(|item| item.source_path.is_some() || used.iter().any(|id| id == &item.id))
            .cloned()
            .collect()
    }

    pub fn follow_new_path(&mut self, path: &Path, kind: FileKind) {
        self.path = path.to_path_buf();
        self.kind = kind;
    }

    /// the document's pictures, bytes included.
    pub fn media(&self) -> &[MediaItem] {
        &self.media
    }

    /// a picture made elsewhere (pasted html, the clipboard) joins the
    /// document. ids are the caller's job: they must not clash.
    pub fn adopt(&mut self, item: MediaItem) {
        self.media.retain(|existing| existing.id != item.id);
        self.media.push(item);
    }
}

/// rich text or an old word file, which the writer opens by converting.
pub fn is_converted_kind(path: &Path, bytes: &[u8]) -> bool {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    bytes.starts_with(b"{\\rtf") || extension == "rtf" || extension == "doc"
}

/// base64 from the ui (a pasted picture).
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    letters_office::base64::decode(text)
}

/// the model as the ui gets it: no carry, and pictures listed without
/// their bytes.
pub fn ui_model(document: &Document) -> Value {
    let mut value = serde_json::to_value(document).unwrap_or(Value::Null);
    if let Value::Object(map) = &mut value {
        map.remove("carry");
        if let Some(Value::Array(media)) = map.get_mut("media") {
            for item in media.iter_mut() {
                if let Value::Object(fields) = item {
                    fields.remove("bytes");
                }
            }
        }
        map.remove("media");
    }
    value
}

/// media ids of every picture the model shows, wherever it is.
fn used_media_ids(document: &Document) -> Vec<String> {
    // the json form reaches every nested place a picture can sit (tables,
    // notes, headers, comments) without a walker per container
    let value = serde_json::to_value(&document.blocks).unwrap_or(Value::Null);
    let mut ids = Vec::new();
    collect_media_ids(&value, &mut ids);
    for part in [
        serde_json::to_value(&document.headers_footers),
        serde_json::to_value(&document.notes),
        serde_json::to_value(&document.comments),
    ]
    .into_iter()
    .flatten()
    {
        collect_media_ids(&part, &mut ids);
    }
    ids
}

fn collect_media_ids(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(id)) = map.get("mediaId") {
                out.push(id.clone());
            }
            map.values().for_each(|child| collect_media_ids(child, out));
        }
        Value::Array(items) => items.iter().for_each(|child| collect_media_ids(child, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{Block, Inline, Paragraph};

    // built from json, so fields the engine adds later need no change here
    fn picture(media_id: &str) -> Inline {
        serde_json::from_value(serde_json::json!({ "type": "image", "mediaId": media_id, "width": 1440, "height": 1440 })).unwrap()
    }

    #[test]
    fn the_ui_model_has_no_carry_and_no_picture_bytes() {
        let mut document = Document::new();
        document.media.push(MediaItem {
            id: "image1.png".into(),
            content_type: "image/png".into(),
            source_path: Some("word/media/image1.png".into()),
            bytes: vec![1, 2, 3],
        });
        let value = ui_model(&document);
        assert!(value.get("carry").is_none());
        assert!(value.get("media").is_none());
        assert!(value.get("blocks").is_some());
    }

    #[test]
    fn merging_puts_carry_and_pictures_back_and_drops_unused_added_ones() {
        let mut document = Document::new();
        document.media.push(MediaItem {
            id: "image1.png".into(),
            content_type: "image/png".into(),
            source_path: Some("word/media/image1.png".into()),
            bytes: vec![1, 2, 3],
        });
        let mut writer =
            WriterDocument::from_document(Path::new("a.docx"), FileKind::Docx, &document);
        let kept = writer.add_picture("photo.jpg", vec![9]);
        let dropped = writer.add_picture("other.png", vec![8]);
        assert_ne!(kept, dropped);
        let mut edited = document.clone();
        edited.blocks = vec![Block::Paragraph(Paragraph {
            content: vec![picture(&kept)],
            ..Default::default()
        })];
        let merged = writer.merge(ui_model(&edited)).unwrap();
        let ids: Vec<_> = merged.media.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, ["image1.png", kept.as_str()]);
        assert_eq!(merged.media[1].content_type, "image/jpeg");
        assert_eq!(merged.carry, document.carry);
    }

    #[test]
    fn pictures_are_shown_as_data_addresses() {
        let mut writer =
            WriterDocument::from_document(Path::new("a.odt"), FileKind::Odt, &Document::new());
        let id = writer.add_picture("dot.png", vec![0x89, b'P']);
        assert_eq!(
            writer.media_data_url(&id).unwrap(),
            "data:image/png;base64,iVA="
        );
        assert!(writer.media_data_url("missing").is_none());
    }

    #[test]
    fn a_model_the_shell_cannot_read_is_refused_with_a_sentence() {
        let writer =
            WriterDocument::from_document(Path::new("a.docx"), FileKind::Docx, &Document::new());
        let error = writer.merge(serde_json::json!({"blocks": 3})).unwrap_err();
        assert!(error.starts_with("The document couldn’t be read back"));
    }
}
