//! copy and paste between the writer and other apps, through the office
//! engine's `clipboard` and html import.
//!
//! copying always carries the picture's own bytes (not an address another
//! app can't reach), and pasting a picture, a picture file or html with
//! pictures puts the bytes in the document's media, converted when word
//! couldn't show them.

use letters_office::base64;
use letters_office::clipboard::{add_image, copy_blocks};
use letters_office::import::from_html_with_images;
use letters_office::model::{Block, Document, MediaItem};
use serde::Serialize;
use serde_json::Value;

/// what goes on the system clipboard for a copy.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Copied {
    pub html: String,
    pub text: String,
    /// a lone picture's bytes as base64, and its content type
    pub image: Option<(String, String)>,
}

/// the clipboard forms of `blocks` (the copied part, as editor blocks),
/// with `document`'s pictures, styles and lists.
pub fn copy(document: &Document, blocks: Vec<Block>) -> Copied {
    let mut part = document.clone();
    let count = blocks.len();
    part.blocks = blocks;
    let selection = copy_blocks(&part, 0, count);
    Copied {
        html: selection.html,
        text: selection.text,
        image: selection
            .image
            .map(|(bytes, content_type)| (base64::encode(&bytes), content_type)),
    }
}

/// a picture pasted or dropped: the inline to insert at the cursor, sized
/// to fit the text width, and a sentence when it was converted.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastedPicture {
    pub inline: Value,
    pub media_id: String,
    pub url: String,
    pub note: Option<String>,
}

/// adds picture bytes to `document` (whose media list is the session's)
/// and returns the new media item with what the editor inserts.
pub fn paste_picture(
    document: &mut Document,
    bytes: &[u8],
    description: Option<String>,
) -> Result<(PastedPicture, MediaItem), String> {
    let pasted = add_image(document, bytes, description).map_err(|error| error.to_string())?;
    let item = document
        .media_item(&pasted.media_id)
        .cloned()
        .ok_or("The picture couldn’t be added.")?;
    let url = format!(
        "data:{};base64,{}",
        item.content_type,
        base64::encode(&item.bytes)
    );
    Ok((
        PastedPicture {
            inline: serde_json::to_value(&pasted.inline).unwrap_or(Value::Null),
            media_id: pasted.media_id,
            url,
            note: pasted.note,
        },
        item,
    ))
}

/// html from the clipboard as editor blocks, with the pictures it names
/// (as `data:` addresses or local files) read in.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastedHtml {
    pub blocks: Value,
    /// list definitions and instances the blocks use
    pub numbering: Value,
    /// (media id, data address) for each picture brought in
    pub pictures: Vec<(String, String)>,
}

/// reads `html`; pictures get ids that can't clash with `taken`.
pub fn paste_html(html: &str, taken: &[String]) -> (PastedHtml, Vec<MediaItem>) {
    let loader = |source: &str| load_picture(source);
    let mut pasted = from_html_with_images(html, Some(&loader));
    let mut items = Vec::new();
    let mut renames = Vec::new();
    for (index, item) in pasted.media.iter().enumerate() {
        let mut id = item.id.clone();
        let mut counter = index;
        while taken.contains(&id) || items.iter().any(|other: &MediaItem| other.id == id) {
            counter += 1;
            id = format!("letters-pasted-{counter}-{}", item.id);
        }
        renames.push((item.id.clone(), id.clone()));
        items.push(MediaItem { id, ..item.clone() });
    }
    let mut blocks = serde_json::to_value(&pasted.blocks).unwrap_or(Value::Null);
    rename_media(&mut blocks, &renames);
    pasted.media.clear();
    let pictures = items
        .iter()
        .map(|item| {
            (
                item.id.clone(),
                format!(
                    "data:{};base64,{}",
                    item.content_type,
                    base64::encode(&item.bytes)
                ),
            )
        })
        .collect();
    (
        PastedHtml {
            blocks,
            numbering: serde_json::to_value(&pasted.numbering).unwrap_or(Value::Null),
            pictures,
        },
        items,
    )
}

/// a picture named in pasted html: a data address, or a local file
/// (clipboard html from browsers and office apps points at temp files).
fn load_picture(source: &str) -> Option<Vec<u8>> {
    if let Some(rest) = source.strip_prefix("data:") {
        let (_, data) = rest.split_once(";base64,")?;
        return base64::decode(data);
    }
    let path = source
        .strip_prefix("file:///")
        .or_else(|| source.strip_prefix("file://"))?;
    let path = percent_decode(path);
    std::fs::read(path).ok()
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(&text[index + 1..index + 3], 16) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn rename_media(value: &mut Value, renames: &[(String, String)]) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(id)) = map.get_mut("mediaId") {
                if let Some((_, new)) = renames.iter().find(|(old, _)| old == id) {
                    *id = new.clone();
                }
            }
            map.values_mut()
                .for_each(|child| rename_media(child, renames));
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|child| rename_media(child, renames)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use letters_office::model::{Image, Inline, Paragraph};

    /// the smallest png: one transparent pixel
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

    fn document_with_picture() -> Document {
        let mut document = Document::new();
        document.media.push(MediaItem {
            id: "image1.png".into(),
            content_type: "image/png".into(),
            source_path: None,
            bytes: base64::decode(PNG).unwrap(),
        });
        document
    }

    #[test]
    fn copying_a_lone_picture_carries_its_bytes() {
        let document = document_with_picture();
        let blocks = vec![Block::Paragraph(Paragraph {
            content: vec![Inline::Image(Image {
                media_id: "image1.png".into(),
                width: 15,
                height: 15,
                ..Default::default()
            })],
            ..Default::default()
        })];
        let copied = copy(&document, blocks);
        let (bytes, content_type) = copied.image.unwrap();
        assert_eq!(content_type, "image/png");
        assert_eq!(bytes, PNG);
        assert!(copied.html.contains("<img"));
    }

    #[test]
    fn copying_text_gives_html_and_plain_text() {
        let copied = copy(&Document::new(), vec![Block::text("Hello")]);
        assert_eq!(copied.text, "Hello");
        assert!(copied.html.contains("Hello"));
        assert!(copied.image.is_none());
    }

    #[test]
    fn a_pasted_picture_joins_the_media_and_is_sized() {
        let mut document = document_with_picture();
        let (pasted, item) =
            paste_picture(&mut document, &base64::decode(PNG).unwrap(), None).unwrap();
        assert_ne!(pasted.media_id, "image1.png");
        assert_eq!(item.content_type, "image/png");
        assert_eq!(pasted.inline["type"], "image");
        assert_eq!(pasted.inline["width"], 15);
        assert!(pasted.url.starts_with("data:image/png;base64,"));
        assert!(paste_picture(&mut document, b"not a picture", None).is_err());
    }

    #[test]
    fn pasted_html_brings_its_pictures_with_ids_that_do_not_clash() {
        let html = format!("<p>Look <img src=\"data:image/png;base64,{PNG}\"></p>");
        let (pasted, items) = paste_html(&html, &["image1.png".to_string()]);
        assert_eq!(items.len(), 1);
        let text = pasted.blocks.to_string();
        assert!(text.contains(&items[0].id), "{text}");
        assert_eq!(pasted.pictures[0].0, items[0].id);
    }

    #[test]
    fn file_addresses_are_decoded() {
        assert_eq!(
            percent_decode("C:/My%20Pictures/a.png"),
            "C:/My Pictures/a.png"
        );
    }
}
