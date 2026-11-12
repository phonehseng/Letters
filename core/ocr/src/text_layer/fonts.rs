//! the two fonts the text layer writes with.
//!
//! - "latin": helvetica, which every pdf reader has built in, so nothing is
//!   stored. its characters are the windows "ansi" set (see `winansi.rs`).
//! - "any script": a font that stores two bytes per character, the
//!   character's unicode value, with a map telling readers exactly that
//!   (the tounicode cmap). every character shares one plain glyph (see
//!   `glyphless.rs`), which is fine because the layer is invisible.
//!
//! the writer knows every character's width in both (helvetica's real
//! widths, and one width for all in the other), so it can stretch each word
//! exactly over its box. each font is added to a document once and found
//! again by a marker on later pages.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use super::glyphless::{font_file, ASCENT, DESCENT, GLYPH_WIDTH};
use super::helvetica;

/// the key and value that mark objects as part of Letters' text layer.
pub const MARKER_KEY: &[u8] = b"LettersOcr";

/// the names the fonts have in a page's resources.
pub const LATIN_FONT_NAME: &[u8] = b"LettersOcrLatin";
pub const ANY_SCRIPT_FONT_NAME: &[u8] = b"LettersOcrAny";

const LATIN_MARK: &[u8] = b"LatinFont";
const ANY_SCRIPT_MARK: &[u8] = b"AnyScriptFont";

/// the latin font's id, adding it if the document doesn't have it yet.
pub fn latin_font(doc: &mut Document) -> ObjectId {
    if let Some(id) = find_marked(doc, LATIN_MARK) {
        return id;
    }
    let widths: Vec<Object> = helvetica::WIDTHS
        .iter()
        .map(|&width| Object::Integer(width.into()))
        .collect();
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
        // helvetica's own widths, written out so that every reader uses
        // the same ones the layer was measured with
        "FirstChar" => i64::from(helvetica::FIRST_CODE),
        "LastChar" => 255,
        "Widths" => widths,
        MARKER_KEY => Object::Name(LATIN_MARK.to_vec()),
    })
}

/// the any-script font's id, adding it (and the parts it needs) if the
/// document doesn't have it yet.
pub fn any_script_font(doc: &mut Document) -> ObjectId {
    if let Some(id) = find_marked(doc, ANY_SCRIPT_MARK) {
        return id;
    }
    let file = font_file();
    let file_length = file.len() as i64;
    let font_file_id = doc.add_object(compressed(dictionary! { "Length1" => file_length }, &file));
    let descriptor_id = doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "LettersGlyphless",
        // "symbolic": the font's characters aren't the standard latin set
        "Flags" => 4,
        "FontBBox" => vec![
            0.into(), i64::from(DESCENT).into(), i64::from(GLYPH_WIDTH).into(), i64::from(ASCENT).into(),
        ],
        "ItalicAngle" => 0,
        "Ascent" => i64::from(ASCENT),
        "Descent" => i64::from(DESCENT),
        "CapHeight" => i64::from(ASCENT),
        "StemV" => 80,
        "FontFile2" => font_file_id,
    });
    let glyph_map_id = doc.add_object(compressed(
        Dictionary::new(),
        &every_character_to_glyph_one(),
    ));
    let descendant_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => "LettersGlyphless",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => 0,
        },
        "FontDescriptor" => descriptor_id,
        "DW" => i64::from(GLYPH_WIDTH),
        "CIDToGIDMap" => glyph_map_id,
    });
    let to_unicode_id = doc.add_object(compressed(Dictionary::new(), to_unicode_cmap().as_bytes()));
    doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "LettersGlyphless",
        // two bytes per character, read as the character number itself
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![descendant_id.into()],
        "ToUnicode" => to_unicode_id,
        MARKER_KEY => Object::Name(ANY_SCRIPT_MARK.to_vec()),
    })
}

/// how text is written in the any-script font: each character as its
/// utf-16 code units, two bytes each. characters beyond the first 65536
/// (some emoji and rare chinese characters) take two units.
pub fn encode_any_script(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

/// every two byte code is glyph 1, the blank one.
fn every_character_to_glyph_one() -> Vec<u8> {
    [0u8, 1].repeat(65536)
}

/// tells readers that code n means unicode character n.
///
/// a cmap range may only differ in its last byte, so the 65536 codes are
/// written as 256 ranges of 256, and a block may hold at most 100 ranges.
fn to_unicode_cmap() -> String {
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\n\
         begincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n\
         /CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let high_bytes: Vec<u32> = (0..256).collect();
    for block in high_bytes.chunks(100) {
        cmap.push_str(&format!("{} beginbfrange\n", block.len()));
        for high in block {
            cmap.push_str(&format!("<{high:02X}00> <{high:02X}FF> <{high:02X}00>\n"));
        }
        cmap.push_str("endbfrange\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap
}

/// a stream compressed with deflate, the way pdf streams usually are.
pub fn compressed(mut dict: Dictionary, data: &[u8]) -> Stream {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder
        .write_all(data)
        .expect("writing to memory cannot fail");
    let bytes = encoder.finish().expect("writing to memory cannot fail");
    dict.set("Filter", "FlateDecode");
    // the stream is already compressed; lopdf must not compress it again
    Stream::new(dict, bytes).with_compression(false)
}

/// an object this crate added earlier, found by its marker.
fn find_marked(doc: &Document, mark: &[u8]) -> Option<ObjectId> {
    doc.objects.iter().find_map(|(id, object)| {
        let dict = object.as_dict().ok()?;
        let value = dict.get(MARKER_KEY).ok()?.as_name().ok()?;
        (value == mark).then_some(*id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_are_added_once() {
        let mut doc = Document::with_version("1.7");
        let first = latin_font(&mut doc);
        assert_eq!(latin_font(&mut doc), first);
        let any = any_script_font(&mut doc);
        let objects = doc.objects.len();
        assert_eq!(any_script_font(&mut doc), any);
        assert_eq!(doc.objects.len(), objects);
    }

    #[test]
    fn the_map_covers_every_code_in_legal_ranges() {
        let cmap = to_unicode_cmap();
        assert_eq!(cmap.matches("beginbfrange").count(), 3);
        assert!(cmap.contains("<0000> <00FF> <0000>"));
        assert!(cmap.contains("<FF00> <FFFF> <FF00>"));
        assert_eq!(
            cmap.lines()
                .filter(|line| line.starts_with('<') && line.len() == 20)
                .count(),
            256
        );
    }

    #[test]
    fn text_becomes_two_bytes_per_character() {
        assert_eq!(encode_any_script("łA"), [0x01, 0x42, 0x00, 0x41]);
        // a character beyond the first 65536 takes two codes
        assert_eq!(encode_any_script("😀").len(), 4);
    }
}
