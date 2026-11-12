//! the font a piece of page text is drawn in, as a font file the web view
//! can load, so the editor typed into sits over the text in the same font
//! and nothing visibly jumps when editing starts.
//!
//! only fonts embedded as TrueType (FontFile2) or OpenType (FontFile3 with
//! subtype OpenType) can be handed over: bare CFF and Type1 programs are
//! not font files a browser reads. a subset font usually holds only the
//! letters already on the page, and some use private codes instead of
//! unicode; the font is only offered when it really draws every letter of
//! the sample, otherwise the editor uses the closest installed font.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::Serialize;

/// what kind of font file the bytes are, for the web view's FontFace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FontFormat {
    TrueType,
    OpenType,
}

/// a font file taken out of a pdf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayFont {
    pub bytes: Vec<u8>,
    pub format: FontFormat,
}

/// the embedded font named by an id from letters-edit's `FontInfo`
/// ("font:12-0"), when it is a font file that draws every character of
/// `sample` (spaces aside). none otherwise.
pub fn display_font(doc: &Document, font_id: &str, sample: &str) -> Option<DisplayFont> {
    let font = doc.get_dictionary(object_id(font_id)?).ok()?;
    let descriptor = descriptor_of(doc, font)?;
    let font = font_program(doc, descriptor)?;
    draws_sample(&font.bytes, sample).then_some(font)
}

/// "font:12-0" to the object (12, 0). other ids (fonts written inline,
/// "font:#3") have no object of their own.
pub fn object_id(font_id: &str) -> Option<ObjectId> {
    let numbers = font_id.strip_prefix("font:")?;
    let (number, generation) = numbers.split_once('-')?;
    Some((number.parse().ok()?, generation.parse().ok()?))
}

fn resolve<'a>(doc: &'a Document, object: &'a Object) -> Option<&'a Object> {
    match object {
        Object::Reference(id) => doc.get_object(*id).ok(),
        other => Some(other),
    }
}

/// the font descriptor, looking inside the descendant font of a
/// composite (Type0) font.
fn descriptor_of<'a>(doc: &'a Document, font: &'a Dictionary) -> Option<&'a Dictionary> {
    let font = if font.get(b"Subtype").ok()?.as_name().ok()? == b"Type0" {
        let descendants = resolve(doc, font.get(b"DescendantFonts").ok()?)?
            .as_array()
            .ok()?;
        resolve(doc, descendants.first()?)?.as_dict().ok()?
    } else {
        font
    };
    resolve(doc, font.get(b"FontDescriptor").ok()?)?
        .as_dict()
        .ok()
}

fn font_program(doc: &Document, descriptor: &Dictionary) -> Option<DisplayFont> {
    if let Ok(object) = descriptor.get(b"FontFile2") {
        let stream = resolve(doc, object)?.as_stream().ok()?;
        return Some(DisplayFont {
            bytes: stream_bytes(stream)?,
            format: FontFormat::TrueType,
        });
    }
    let stream = resolve(doc, descriptor.get(b"FontFile3").ok()?)?
        .as_stream()
        .ok()?;
    // bare cff (Type1C, CIDFontType0C) is not a font file a browser reads
    let subtype = stream.dict.get(b"Subtype").ok()?.as_name().ok()?;
    (subtype == b"OpenType").then(|| DisplayFont {
        bytes: stream_bytes(stream).unwrap_or_default(),
        format: FontFormat::OpenType,
    })
}

fn stream_bytes(stream: &lopdf::Stream) -> Option<Vec<u8>> {
    if stream.dict.get(b"Filter").is_err() {
        return Some(stream.content.clone());
    }
    stream.decompressed_content().ok()
}

/// the font's own character map draws every letter of the sample.
fn draws_sample(bytes: &[u8], sample: &str) -> bool {
    let Ok(face) = ttf_parser::Face::parse(bytes, 0) else {
        return false;
    };
    sample
        .chars()
        .filter(|character| !character.is_whitespace())
        .all(|character| face.glyph_index(character).is_some())
}
