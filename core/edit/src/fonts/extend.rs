//! extending a font already in the file with glyphs it lacks, so new
//! text is drawn in the same font object as the old text.
//!
//! only embedded TrueType programs (FontFile2) can be extended: composite
//! fonts with Identity codes, and simple TrueType fonts. the font program
//! gains the glyphs at new glyph numbers (see `append.rs`); the pdf side
//! gains what readers need to reach them: a composite font gets new CIDs
//! with widths in /W (and entries in a /CIDToGIDMap stream when it has
//! one); a simple font gets free one-byte codes with /Widths, /Differences
//! names and cmap entries. /ToUnicode gains every new character so the
//! text copies and searches correctly. nothing that already exists in the
//! font changes number or width, so text already on the page is drawn
//! exactly as before.

use std::collections::{BTreeMap, BTreeSet};

use lopdf::{Dictionary, Object, ObjectId, Stream};

use crate::draft::Draft;
use crate::error::EditError;
use crate::fonts::append::{self, CmapEntry};
use crate::fonts::cmap;
use crate::fonts::donor::{GlyphSource, Outline, Target};
use crate::fonts::embed;
use crate::fonts::encodings;
use crate::fonts::pdf_font::EncodedChar;
use crate::pdf_values::{name, real};
use crate::summary::FontExtension;

/// the codes new characters got in the extended font.
pub(crate) struct Extension {
    pub codes: BTreeMap<char, EncodedChar>,
    pub record: FontExtension,
}

/// the parts of a font dictionary an extension changes.
struct FontParts {
    font: Dictionary,
    /// the CIDFont of a composite font
    descendant: Option<(ObjectId, Dictionary)>,
    descriptor_id: ObjectId,
    descriptor: Dictionary,
    program: Vec<u8>,
}

fn stream_bytes(draft: &Draft, object: &Object) -> Option<Vec<u8>> {
    let stream = draft.resolve(object).as_stream().ok()?;
    if stream.dict.has(b"Filter") {
        stream.decompressed_content().ok()
    } else {
        Some(stream.content.clone())
    }
}

fn reference(dict: &Dictionary, key: &[u8]) -> Option<ObjectId> {
    dict.get(key).ok()?.as_reference().ok()
}

/// reads the font's dictionaries and program, or none for a font that
/// can't be extended (not embedded TrueType, or written inline).
fn font_parts(draft: &Draft, font_id: ObjectId) -> Option<FontParts> {
    let font = draft.dictionary(font_id)?.clone();
    let subtype = font.get(b"Subtype").ok()?.as_name().ok()?;
    let descendant = match subtype {
        b"Type0" => {
            let encoding = font.get(b"Encoding").ok()?.as_name().ok()?;
            if encoding != b"Identity-H" && encoding != b"Identity-V" {
                return None;
            }
            let first = font
                .get(b"DescendantFonts")
                .ok()
                .map(|list| draft.resolve(list))?
                .as_array()
                .ok()?
                .first()?;
            let id = first.as_reference().ok()?;
            Some((id, draft.dictionary(id)?.clone()))
        }
        b"TrueType" => None,
        _ => return None,
    };
    let owner = descendant.as_ref().map_or(&font, |(_, dict)| dict);
    let descriptor_id = reference(owner, b"FontDescriptor")?;
    let descriptor = draft.dictionary(descriptor_id)?.clone();
    let program = stream_bytes(draft, descriptor.get(b"FontFile2").ok()?)?;
    Some(FontParts {
        font,
        descendant,
        descriptor_id,
        descriptor,
        program,
    })
}

/// adds glyphs for `characters` to the font, drawn by `source`. none when
/// the font can't be extended or the source lacks a character.
pub(crate) fn extend_font(
    draft: &mut Draft,
    font_id: ObjectId,
    font_name: &str,
    characters: &BTreeSet<char>,
    source: &dyn GlyphSource,
) -> Result<Option<Extension>, EditError> {
    let Some(parts) = font_parts(draft, font_id) else {
        return Ok(None);
    };
    let Ok(face) = ttf_parser::Face::parse(&parts.program, 0) else {
        return Ok(None);
    };
    let target = Target {
        units_per_em: face.units_per_em(),
        cap_height: parts
            .descriptor
            .get(b"CapHeight")
            .ok()
            .and_then(|value| value.as_float().ok()),
    };
    let mut outlines: Vec<(char, Outline)> = Vec::new();
    for character in characters {
        match source.outline(*character, target) {
            Some(outline) => outlines.push((*character, outline)),
            None => return Ok(None),
        }
    }
    let units = f32::from(target.units_per_em.max(1));
    let codes = match parts.descendant.clone() {
        Some(descendant) => extend_composite(draft, font_id, &parts, descendant, &outlines, units)?,
        None => match extend_simple(draft, font_id, &parts, &outlines, units)? {
            Some(codes) => codes,
            None => return Ok(None),
        },
    };
    Ok(Some(Extension {
        codes,
        record: FontExtension {
            font: font_name.to_owned(),
            characters: characters.iter().collect(),
            glyph_source: source.describe(),
        },
    }))
}

/// writes the extended program as a new font file and points the
/// descriptor at it. a CIDSet, which lists the glyphs present, is
/// dropped rather than left wrong (it is optional).
fn store_program(draft: &mut Draft, parts: &FontParts, program: Vec<u8>) {
    let mut file = Dictionary::new();
    file.set("Length1", Object::Integer(program.len() as i64));
    let mut stream = Stream::new(file, program);
    let _ = stream.compress();
    let file_id = draft.add(Object::Stream(stream));
    let mut descriptor = parts.descriptor.clone();
    descriptor.set("FontFile2", Object::Reference(file_id));
    descriptor.remove(b"CIDSet");
    draft.set(parts.descriptor_id, Object::Dictionary(descriptor));
}

/// thousandths of the em, as pdf widths are.
fn pdf_width(outline: &Outline, units: f32) -> f32 {
    (outline.advance * 1000.0 / units * 1000.0).round() / 1000.0
}

fn extend_composite(
    draft: &mut Draft,
    font_id: ObjectId,
    parts: &FontParts,
    descendant: (ObjectId, Dictionary),
    outlines: &[(char, Outline)],
    units: f32,
) -> Result<BTreeMap<char, EncodedChar>, EditError> {
    let shapes: Vec<Outline> = outlines
        .iter()
        .map(|(_, outline)| outline.clone())
        .collect();
    let appended = append::append_glyphs(&parts.program, &shapes, &[])?;
    store_program(draft, parts, appended.program);
    let (descendant_id, mut cid_font) = descendant;
    let map = cid_font
        .get(b"CIDToGIDMap")
        .ok()
        .and_then(|map| stream_bytes(draft, map));
    let mut map = map;
    let mut widths = match cid_font.get(b"W").ok().map(|w| draft.resolve(w)) {
        Some(Object::Array(list)) => list.clone(),
        _ => Vec::new(),
    };
    let mut codes = BTreeMap::new();
    let mut to_unicode = BTreeMap::new();
    for (index, (character, outline)) in outlines.iter().enumerate() {
        let glyph = appended.first_glyph + index as u16;
        // with an identity map the cid is the glyph; with a map stream the
        // next free cid is given the glyph
        let cid = match map.as_mut() {
            Some(bytes) => {
                let cid = (bytes.len() / 2) as u16;
                bytes.extend_from_slice(&glyph.to_be_bytes());
                cid
            }
            None => glyph,
        };
        let width = pdf_width(outline, units);
        widths.push(Object::Integer(i64::from(cid)));
        widths.push(Object::Array(vec![real(width)]));
        to_unicode.insert(cid, character.to_string());
        codes.insert(
            *character,
            EncodedChar {
                code: cid.to_be_bytes().to_vec(),
                width,
                is_word_space: false,
            },
        );
    }
    cid_font.set("W", Object::Array(widths));
    if let Some(bytes) = map {
        let mut stream = Stream::new(Dictionary::new(), bytes);
        let _ = stream.compress();
        cid_font.set(
            "CIDToGIDMap",
            Object::Reference(draft.add(Object::Stream(stream))),
        );
    }
    draft.set(descendant_id, Object::Dictionary(cid_font));
    embed::extend_to_unicode(draft, font_id, &to_unicode);
    Ok(codes)
}

/// the one-byte codes a simple font doesn't use yet: no width, no name
/// in its encoding differences, nothing in its /ToUnicode.
fn free_codes(draft: &Draft, font: &Dictionary) -> Vec<u8> {
    let first = font
        .get(b"FirstChar")
        .ok()
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0);
    let widths: Vec<f32> = match font.get(b"Widths").ok().map(|w| draft.resolve(w)) {
        Some(Object::Array(list)) => list
            .iter()
            .map(|w| draft.resolve(w).as_float().unwrap_or(0.0))
            .collect(),
        _ => Vec::new(),
    };
    let mapped: BTreeSet<u32> = font
        .get(b"ToUnicode")
        .ok()
        .and_then(|map| stream_bytes(draft, map))
        .map(|bytes| {
            cmap::parse(&bytes)
                .to_text
                .keys()
                .map(|(_, code)| *code)
                .collect()
        })
        .unwrap_or_default();
    let used = |code: u8| {
        let index = i64::from(code) - first;
        let has_width = usize::try_from(index)
            .ok()
            .and_then(|i| widths.get(i))
            .is_some_and(|w| *w > 0.0);
        has_width || mapped.contains(&u32::from(code))
    };
    // codes from 128 up first: they are free in most latin subsets, and
    // 32 (space) and below are never taken, since readers treat them apart
    (128..=255u8)
        .chain(33..128)
        .filter(|code| !used(*code))
        .collect()
}

fn extend_simple(
    draft: &mut Draft,
    font_id: ObjectId,
    parts: &FontParts,
    outlines: &[(char, Outline)],
    units: f32,
) -> Result<Option<BTreeMap<char, EncodedChar>>, EditError> {
    let free = free_codes(draft, &parts.font);
    if free.len() < outlines.len() {
        return Ok(None);
    }
    let symbolic = parts
        .descriptor
        .get(b"Flags")
        .ok()
        .and_then(|flags| flags.as_i64().ok())
        .unwrap_or(0)
        & 4
        != 0;
    let mut cmap_entries = Vec::new();
    for (index, (character, _)) in outlines.iter().enumerate() {
        let glyph = index as u16;
        let code = free[index];
        if symbolic {
            cmap_entries.push(CmapEntry::Symbol(code, glyph));
            cmap_entries.push(CmapEntry::Mac(code, glyph));
        } else {
            cmap_entries.push(CmapEntry::Unicode(*character as u32, glyph));
        }
    }
    let shapes: Vec<Outline> = outlines
        .iter()
        .map(|(_, outline)| outline.clone())
        .collect();
    let appended = append::append_glyphs(&parts.program, &shapes, &cmap_entries)?;
    store_program(draft, parts, appended.program);
    let mut font = parts.font.clone();
    let mut codes = BTreeMap::new();
    let mut to_unicode = BTreeMap::new();
    for (index, (character, outline)) in outlines.iter().enumerate() {
        let code = free[index];
        let width = pdf_width(outline, units);
        set_width(draft, &mut font, code, width);
        add_difference(draft, &mut font, code, *character);
        to_unicode.insert(code, character.to_string());
        codes.insert(
            *character,
            EncodedChar {
                code: vec![code],
                width,
                is_word_space: false,
            },
        );
    }
    if font.has(b"ToUnicode") {
        extend_one_byte_to_unicode(draft, &mut font, &to_unicode);
    }
    draft.set(font_id, Object::Dictionary(font));
    Ok(Some(codes))
}

/// gives a code its width, growing /FirstChar ... /LastChar to reach it.
fn set_width(draft: &Draft, font: &mut Dictionary, code: u8, width: f32) {
    let first = font
        .get(b"FirstChar")
        .ok()
        .and_then(|v| v.as_i64().ok())
        .unwrap_or(i64::from(code));
    let mut widths: Vec<Object> = match font.get(b"Widths").ok().map(|w| draft.resolve(w)) {
        Some(Object::Array(list)) => list.clone(),
        _ => Vec::new(),
    };
    let code = i64::from(code);
    let mut first_char = first;
    if code < first {
        let padding = (first - code) as usize;
        widths.splice(0..0, std::iter::repeat_n(Object::Integer(0), padding));
        first_char = code;
    }
    let index = (code - first_char) as usize;
    if widths.len() <= index {
        widths.resize(index + 1, Object::Integer(0));
    }
    widths[index] = real(width);
    font.set("FirstChar", Object::Integer(first_char));
    font.set(
        "LastChar",
        Object::Integer(first_char + widths.len() as i64 - 1),
    );
    font.set("Widths", Object::Array(widths));
}

/// names a code in the encoding's /Differences, keeping its base encoding.
fn add_difference(draft: &Draft, font: &mut Dictionary, code: u8, character: char) {
    let mut encoding = match font.get(b"Encoding").ok().map(|e| draft.resolve(e)) {
        Some(Object::Dictionary(dict)) => dict.clone(),
        Some(Object::Name(base)) => {
            let mut dict = Dictionary::new();
            dict.set("BaseEncoding", Object::Name(base.clone()));
            dict
        }
        _ => Dictionary::new(),
    };
    let mut differences = match encoding.get(b"Differences") {
        Ok(Object::Array(list)) => list.clone(),
        _ => Vec::new(),
    };
    let glyph_name = encodings::name_of_char(character)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("uni{:04X}", character as u32));
    differences.push(Object::Integer(i64::from(code)));
    differences.push(name(&glyph_name));
    encoding.set("Type", name("Encoding"));
    encoding.set("Differences", Object::Array(differences));
    font.set("Encoding", Object::Dictionary(encoding));
}

/// a simple font's /ToUnicode written again with the new codes.
fn extend_one_byte_to_unicode(
    draft: &mut Draft,
    font: &mut Dictionary,
    additions: &BTreeMap<u8, String>,
) {
    let existing = font
        .get(b"ToUnicode")
        .ok()
        .and_then(|map| stream_bytes(draft, map))
        .map(|bytes| cmap::parse(&bytes))
        .unwrap_or_default();
    let mut entries: BTreeMap<u8, String> = existing
        .to_text
        .iter()
        .filter(|((length, code), _)| *length == 1 && *code < 256)
        .map(|((_, code), text)| (*code as u8, text.clone()))
        .collect();
    entries.extend(additions.clone());
    let mut stream = Stream::new(Dictionary::new(), cmap::write_one_byte_to_unicode(&entries));
    let _ = stream.compress();
    font.set(
        "ToUnicode",
        Object::Reference(draft.add(Object::Stream(stream))),
    );
}
