//! copy and paste: pictures from the clipboard into the media table,
//! html fragments through the safe html import, and a selection out as
//! html, plain text and (for a lone picture) the picture itself.
//!
//! pictures are kept in formats word, libreoffice and browsers all show:
//! png, jpeg and gif are stored as they are, bmp is converted to png.
//! webp and svg are stored as they are, because converting them needs a
//! full decoder; current word and libreoffice show them, older ones may
//! not (`word_compatibility` says so).

use std::io::Write;

use serde::Serialize;

use crate::error::OfficeError;
use crate::model::{Block, Document, Image, Inline, MediaItem, Placement, Twips};

/// what pasting a picture produced.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastedImage {
    /// the picture to put at the cursor
    pub inline: Inline,
    pub media_id: String,
    /// a plain sentence when the picture was converted or kept in a format
    /// some applications cannot show
    pub note: Option<String>,
}

/// picture formats recognised by their first bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PictureFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Webp,
    Svg,
}

impl PictureFormat {
    pub fn sniff(bytes: &[u8]) -> Option<PictureFormat> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(PictureFormat::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(PictureFormat::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(PictureFormat::Gif)
        } else if bytes.starts_with(b"BM") && bytes.len() > 54 {
            Some(PictureFormat::Bmp)
        } else if bytes.len() > 30 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
            Some(PictureFormat::Webp)
        } else {
            let start =
                String::from_utf8_lossy(&bytes[..bytes.len().min(1024)]).to_ascii_lowercase();
            start.contains("<svg").then_some(PictureFormat::Svg)
        }
    }

    fn content_type(self) -> &'static str {
        match self {
            PictureFormat::Png | PictureFormat::Bmp => "image/png",
            PictureFormat::Jpeg => "image/jpeg",
            PictureFormat::Gif => "image/gif",
            PictureFormat::Webp => "image/webp",
            PictureFormat::Svg => "image/svg+xml",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            PictureFormat::Png | PictureFormat::Bmp => "png",
            PictureFormat::Jpeg => "jpg",
            PictureFormat::Gif => "gif",
            PictureFormat::Webp => "webp",
            PictureFormat::Svg => "svg",
        }
    }
}

/// adds a pasted picture to the media table and returns the inline to
/// insert, sized at 96 pixels to the inch and no wider than the page's
/// text area.
pub fn add_image(
    document: &mut Document,
    bytes: &[u8],
    description: Option<String>,
) -> Result<PastedImage, OfficeError> {
    let format = PictureFormat::sniff(bytes)
        .ok_or_else(|| OfficeError::NotSupportedYet("this kind of picture".into()))?;
    let (stored, note) = match format {
        PictureFormat::Bmp => (
            bmp_to_png(bytes)?,
            Some("The bitmap picture was converted to PNG.".to_string()),
        ),
        PictureFormat::Webp => (
            bytes.to_vec(),
            Some("The picture is kept as WebP; Word 2019 and older can’t show it.".to_string()),
        ),
        PictureFormat::Svg => (
            bytes.to_vec(),
            Some("The picture is kept as SVG; Word 2013 and older can’t show it.".to_string()),
        ),
        _ => (bytes.to_vec(), None),
    };
    let (pixels_wide, pixels_high) = pixel_size(format, bytes).unwrap_or((96, 96));
    let section_width = document
        .sections
        .first()
        .map(|s| s.page.text_width())
        .unwrap_or(9360);
    let (width, height) = fit(
        (pixels_wide as Twips) * 15,
        (pixels_high as Twips) * 15,
        section_width,
    );
    let media_id = unique_media_id(document, format.extension());
    document.media.push(MediaItem {
        id: media_id.clone(),
        content_type: format.content_type().into(),
        source_path: None,
        bytes: stored,
    });
    let inline = Inline::Image(Image {
        media_id: media_id.clone(),
        width,
        height,
        description,
        placement: Placement::Inline,
        lock_aspect_ratio: Some(true),
        ..Default::default()
    });
    Ok(PastedImage {
        inline,
        media_id,
        note,
    })
}

fn unique_media_id(document: &Document, extension: &str) -> String {
    let mut number = document.media.len() + 1;
    loop {
        let id = format!("pasted-{number}.{extension}");
        if document.media_item(&id).is_none() {
            return id;
        }
        number += 1;
    }
}

/// scales a size down, keeping proportions, so it fits `limit` wide.
fn fit(width: Twips, height: Twips, limit: Twips) -> (Twips, Twips) {
    if width <= limit || width == 0 {
        return (width.max(15), height.max(15));
    }
    (
        limit,
        ((i64::from(height) * i64::from(limit)) / i64::from(width)) as Twips,
    )
}

fn u16_be(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes([
        *bytes.get(at)?,
        *bytes.get(at + 1)?,
    ])))
}

fn u32_le(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// a picture's size in pixels from its header.
pub fn pixel_size(format: PictureFormat, bytes: &[u8]) -> Option<(u32, u32)> {
    match format {
        PictureFormat::Png => Some((
            u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?),
            u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?),
        )),
        PictureFormat::Gif => Some((
            u32::from(u16::from_le_bytes([bytes[6], bytes[7]])),
            u32::from(u16::from_le_bytes([bytes[8], bytes[9]])),
        )),
        PictureFormat::Bmp => Some((
            u32_le(bytes, 18)?,
            (u32_le(bytes, 22)? as i32).unsigned_abs(),
        )),
        PictureFormat::Jpeg => jpeg_size(bytes),
        PictureFormat::Webp => webp_size(bytes),
        PictureFormat::Svg => svg_size(bytes),
    }
}

/// walks jpeg markers to the frame header, which holds the size.
fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    while at + 9 < bytes.len() {
        if bytes[at] != 0xFF {
            return None;
        }
        let marker = bytes[at + 1];
        let length = u16_be(bytes, at + 2)? as usize;
        let frame = (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker);
        if frame {
            return Some((u16_be(bytes, at + 7)?, u16_be(bytes, at + 5)?));
        }
        at += 2 + length;
    }
    None
}

fn webp_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let three = |at: usize| -> Option<u32> {
        Some(
            u32::from(*bytes.get(at)?)
                | (u32::from(*bytes.get(at + 1)?) << 8)
                | (u32::from(*bytes.get(at + 2)?) << 16),
        )
    };
    match bytes.get(12..16)? {
        b"VP8X" => Some((three(24)? + 1, three(27)? + 1)),
        b"VP8 " => Some((
            u32::from(u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3FFF),
            u32::from(u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3FFF),
        )),
        b"VP8L" => {
            let bits = u32_le(bytes, 21)?;
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        _ => None,
    }
}

fn svg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let text = String::from_utf8_lossy(bytes);
    let document = crate::xml::parse(&text).ok()?;
    let number = |name: &str| {
        let value = document.root.attr(name)?;
        let digits: String = value
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        digits.parse::<f64>().ok().map(|v| v.round() as u32)
    };
    if let (Some(width), Some(height)) = (number("width"), number("height")) {
        return Some((width, height));
    }
    let view: Vec<f64> = document
        .root
        .attr("viewBox")?
        .split([' ', ','])
        .filter_map(|v| v.parse().ok())
        .collect();
    (view.len() == 4).then(|| (view[2].round() as u32, view[3].round() as u32))
}

/// converts an uncompressed 24 or 32 bit bmp (what windows puts on the
/// clipboard) to png.
pub fn bmp_to_png(bytes: &[u8]) -> Result<Vec<u8>, OfficeError> {
    let unsupported = || OfficeError::NotSupportedYet("this kind of bitmap picture".into());
    let offset = u32_le(bytes, 10).ok_or_else(unsupported)? as usize;
    let width = u32_le(bytes, 18).ok_or_else(unsupported)? as usize;
    let raw_height = u32_le(bytes, 22).ok_or_else(unsupported)? as i32;
    let bits = u16_be(&[bytes[29], bytes[28]], 0).ok_or_else(unsupported)?;
    let compression = u32_le(bytes, 30).ok_or_else(unsupported)?;
    let height = raw_height.unsigned_abs() as usize;
    if !(bits == 24 || bits == 32)
        || !(compression == 0 || compression == 3)
        || width == 0
        || height == 0
    {
        return Err(unsupported());
    }
    if width > 30_000 || height > 30_000 {
        return Err(OfficeError::TooLarge(
            "the picture is over 30,000 pixels across".into(),
        ));
    }
    let bytes_per_pixel = bits as usize / 8;
    let stride = (width * bytes_per_pixel).div_ceil(4) * 4;
    if bytes.len() < offset + stride * height {
        return Err(OfficeError::Damaged(
            "the bitmap picture is cut short".into(),
        ));
    }
    let mut rows = Vec::with_capacity(height * (width * 4 + 1));
    for row in 0..height {
        // bmp rows go bottom to top unless the height is negative
        let source_row = if raw_height > 0 {
            height - 1 - row
        } else {
            row
        };
        let start = offset + source_row * stride;
        rows.push(0u8);
        for x in 0..width {
            let pixel = &bytes[start + x * bytes_per_pixel..];
            let alpha = if bytes_per_pixel == 4 && compression == 0 {
                255
            } else if bytes_per_pixel == 4 {
                pixel[3]
            } else {
                255
            };
            rows.extend_from_slice(&[pixel[2], pixel[1], pixel[0], alpha]);
        }
    }
    Ok(encode_png(width as u32, height as u32, &rows))
}

/// a minimal png: rgba, 8 bits a channel, rows already filtered (type 0).
fn encode_png(width: u32, height: u32, rows: &[u8]) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let compressed = encoder
        .write_all(rows)
        .and_then(|_| encoder.finish())
        .unwrap_or_default();
    chunk(&mut out, b"IDAT", &compressed);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(kind);
    hasher.update(data);
    out.extend_from_slice(&hasher.finalize().to_be_bytes());
}

/// a selection ready for the system clipboard.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub html: String,
    pub text: String,
    /// when the selection is a single picture: its bytes and content type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<(Vec<u8>, String)>,
}

/// copies blocks `start..end` (styles, lists and pictures come along).
pub fn copy_blocks(document: &Document, start: usize, end: usize) -> Selection {
    let end = end.min(document.blocks.len());
    let start = start.min(end);
    let mut part = document.clone();
    part.blocks = document.blocks[start..end].to_vec();
    part.notes.clear();
    let image = single_image(&part.blocks).and_then(|id| {
        let item = document.media_item(&id)?;
        Some((item.bytes.clone(), item.content_type.clone()))
    });
    Selection {
        html: crate::export::to_html(&part),
        text: crate::export::to_plain_text(&part)
            .trim_end_matches('\n')
            .to_string(),
        image,
    }
}

/// the media id when the blocks hold one picture and no text.
fn single_image(blocks: &[Block]) -> Option<String> {
    let mut found = None;
    for block in blocks {
        let paragraph = block.paragraph()?;
        for inline in &paragraph.content {
            match inline {
                Inline::Image(image) if found.is_none() => found = Some(image.media_id.clone()),
                Inline::Text { text, .. } if text.trim().is_empty() => {}
                _ => return None,
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a 2 by 1 bmp: a red and a blue pixel.
    fn bmp() -> Vec<u8> {
        let mut bytes = b"BM".to_vec();
        bytes.extend_from_slice(&(62u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&(54u32).to_le_bytes());
        bytes.extend_from_slice(&(40u32).to_le_bytes());
        bytes.extend_from_slice(&(2u32).to_le_bytes());
        bytes.extend_from_slice(&(1u32).to_le_bytes());
        bytes.extend_from_slice(&(1u16).to_le_bytes());
        bytes.extend_from_slice(&(24u16).to_le_bytes());
        bytes.extend_from_slice(&[0; 24]);
        bytes.extend_from_slice(&[0, 0, 255, 255, 0, 0, 0, 0]);
        bytes
    }

    #[test]
    fn a_bitmap_becomes_a_png() {
        let png = bmp_to_png(&bmp()).unwrap();
        assert_eq!(PictureFormat::sniff(&png), Some(PictureFormat::Png));
        assert_eq!(pixel_size(PictureFormat::Png, &png), Some((2, 1)));
        let mut document = Document::new();
        let pasted = add_image(&mut document, &bmp(), Some("dots".into())).unwrap();
        assert!(pasted.note.unwrap().contains("PNG"));
        assert_eq!(document.media[0].content_type, "image/png");
        let Inline::Image(image) = pasted.inline else {
            panic!()
        };
        assert_eq!((image.width, image.height), (30, 15));
    }

    #[test]
    fn sizes_from_headers() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100"/>"#;
        assert_eq!(pixel_size(PictureFormat::Svg, svg), Some((200, 100)));
        let gif = b"GIF89a\x10\x00\x08\x00rest";
        assert_eq!(pixel_size(PictureFormat::Gif, gif), Some((16, 8)));
        assert!(PictureFormat::sniff(b"hello").is_none());
    }

    #[test]
    fn copying_one_picture_gives_its_bytes() {
        let mut document = Document::new();
        let pasted = add_image(&mut document, &bmp(), None).unwrap();
        document.blocks = vec![Block::Paragraph(crate::model::Paragraph {
            content: vec![pasted.inline],
            ..Default::default()
        })];
        let selection = copy_blocks(&document, 0, 1);
        assert_eq!(selection.image.unwrap().1, "image/png");
        assert!(selection.html.contains("data:image/png;base64,"));
    }
}
