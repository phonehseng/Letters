//! the font files stored inside a pdf, for pages that want to show text
//! in the pdf's own fonts.
//!
//! a font is offered only when its maker allows embedding (the
//! "fsType" permission in the font: anything but "restricted"), and is
//! used for the page only when a browser can find the page's characters
//! in it. many pdfs keep a cut-down copy that maps glyphs by the pdf's own
//! codes instead of by unicode; a browser would draw the wrong letters
//! with those, so the page falls back to the font's family name.

use lopdf::{Document, Object, ObjectId};

use crate::source::resolve;

/// a font program found in the pdf.
pub(crate) struct EmbeddedFont {
    pub bytes: Vec<u8>,
    /// "ttf" or "otf"
    pub extension: &'static str,
    pub mime: &'static str,
}

/// the font file of the font letters-edit calls `font_ref` ("font:12-0").
pub(crate) fn embedded_font(doc: &Document, font_ref: &str) -> Option<EmbeddedFont> {
    let id = parse_font_ref(font_ref)?;
    let font = doc.get_dictionary(id).ok()?;
    let descriptor_owner = match font.get(b"Subtype").and_then(Object::as_name) {
        Ok(b"Type0") => {
            let descendants = resolve(doc, font.get(b"DescendantFonts").ok()?)
                .as_array()
                .ok()?;
            resolve(doc, descendants.first()?).as_dict().ok()?
        }
        _ => font,
    };
    let descriptor = resolve(doc, descriptor_owner.get(b"FontDescriptor").ok()?)
        .as_dict()
        .ok()?;
    let (stream, extension, mime) = if let Ok(file) = descriptor.get(b"FontFile2") {
        (resolve(doc, file).as_stream().ok()?, "ttf", "font/ttf")
    } else {
        let stream = resolve(doc, descriptor.get(b"FontFile3").ok()?)
            .as_stream()
            .ok()?;
        // only opentype; bare cff and type 1 programs can't be used by browsers
        let is_opentype = matches!(
            stream.dict.get(b"Subtype").and_then(Object::as_name),
            Ok(b"OpenType")
        );
        if !is_opentype {
            return None;
        }
        (stream, "otf", "font/otf")
    };
    let bytes = stream
        .decompressed_content()
        .unwrap_or_else(|_| stream.content.clone());
    Some(EmbeddedFont {
        bytes,
        extension,
        mime,
    })
}

fn parse_font_ref(font_ref: &str) -> Option<ObjectId> {
    let (number, generation) = font_ref.strip_prefix("font:")?.split_once('-')?;
    Some((number.parse().ok()?, generation.parse().ok()?))
}

/// whether the font's maker allows copying it into documents.
pub(crate) fn embedding_allowed(bytes: &[u8]) -> bool {
    match ttf_parser::Face::parse(bytes, 0) {
        Ok(face) => !matches!(
            face.permissions(),
            Some(ttf_parser::Permissions::Restricted)
        ),
        Err(_) => false,
    }
}

/// whether a browser would find every one of these characters in the font.
pub(crate) fn covers(bytes: &[u8], text: &str) -> bool {
    let Ok(face) = ttf_parser::Face::parse(bytes, 0) else {
        return false;
    };
    text.chars()
        .filter(|c| !c.is_whitespace())
        .all(|c| face.glyph_index(c).is_some_and(|glyph| glyph.0 != 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_refs_name_objects() {
        assert_eq!(parse_font_ref("font:12-0"), Some((12, 0)));
        assert_eq!(parse_font_ref("font:#3"), None);
    }

    #[test]
    fn broken_font_files_are_neither_allowed_nor_used() {
        assert!(!embedding_allowed(b"not a font"));
        assert!(!covers(b"not a font", "abc"));
    }
}
