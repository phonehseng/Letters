//! embedding fonts that a document only names. the caller finds a font
//! file (the same family, or a metric-compatible substitute such as
//! liberation sans for arial); this module checks the font's licence
//! (fsType, through letters-fonts), subsets it to the characters the
//! encoding can show, and rewrites the font as an embedded TrueType font
//! with widths from the program itself and a ToUnicode map.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::fonts::font_facts;
use crate::objects::{dict_of, get_name};

/// what the caller is asked for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontRequest {
    /// the font's name without a subset tag, such as "Arial-BoldMT"
    pub name: String,
    pub bold: bool,
    pub italic: bool,
    /// fonts that have the same widths, from letters-fonts' table, best first
    pub metric_compatible: Vec<String>,
}

/// a font file the caller found for a request.
#[derive(Clone, Debug, PartialEq)]
pub struct FontFile {
    pub data: Vec<u8>,
    /// which font in a collection (.ttc)
    pub index: u32,
    /// the family it is, for the report
    pub family: String,
    /// true when it is the same font, or one with the same widths
    pub same_widths: bool,
}

/// where fonts come from: in the app, letters-fonts' library of fonts on
/// this computer. none means no suitable font was found.
pub trait FontSource {
    fn find(&self, request: &FontRequest) -> Option<FontFile>;
}

/// what happened to one font.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontOutcome {
    pub font: String,
    /// the family embedded, when one was
    pub embedded_as: Option<String>,
    /// the embedded font has other widths, so lines of text may run
    /// slightly longer or shorter than before
    pub widths_differ: bool,
    /// why it couldn't be embedded, when it wasn't
    pub problem: Option<String>,
}

/// winansi codes 128 to 159 as unicode (iso 32000 annex d.2); the other
/// codes from 32 up are latin-1.
const WIN_ANSI_HIGH: [u16; 32] = [
    0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
    0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
];
const FIRST_CODE: u8 = 32;
const LAST_CODE: u8 = 255;
/// the subset tag added to the name of a subsetted font (iso 32000 9.6.4).
const SUBSET_TAG: &str = "LTRSAA+";

pub(crate) fn win_ansi(code: u8) -> Option<char> {
    match code {
        0..=31 | 127 => None,
        128..=159 => {
            char::from_u32(WIN_ANSI_HIGH[(code - 128) as usize] as u32).filter(|c| *c != '\0')
        }
        other => Some(other as char),
    }
}

/// font dictionaries written straight into a resource dictionary become
/// objects of their own, so they can be rewritten in one place.
pub(super) fn promote_direct_fonts(doc: &mut Document) {
    let holders: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for holder in holders {
        let Some(dict) = doc.get_object(holder).ok().and_then(dict_of).cloned() else {
            continue;
        };
        let mut changed = dict.clone();
        if let Ok(Object::Dictionary(resources)) = dict.get(b"Resources") {
            changed.set(
                "Resources",
                Object::Dictionary(promote_in(doc, resources.clone())),
            );
        }
        if dict.has(b"Font") && !dict.has(b"Type") {
            changed = promote_in(doc, changed);
        }
        if changed != dict {
            match doc.get_object_mut(holder) {
                Ok(Object::Dictionary(target)) => *target = changed,
                Ok(Object::Stream(stream)) => stream.dict = changed,
                _ => {}
            }
        }
    }
}

/// a resource dictionary with its direct /Font entries made objects.
fn promote_in(doc: &mut Document, mut resources: Dictionary) -> Dictionary {
    let Ok(Object::Dictionary(fonts)) = resources.get(b"Font").cloned() else {
        return resources;
    };
    let mut promoted = Dictionary::new();
    for (name, value) in fonts.iter() {
        let value = match value {
            Object::Dictionary(font) => {
                Object::Reference(doc.add_object(Object::Dictionary(font.clone())))
            }
            other => other.clone(),
        };
        promoted.set(name.clone(), value);
    }
    resources.set("Font", Object::Dictionary(promoted));
    resources
}

/// the fonts drawn with but not embedded, by object id.
pub(super) fn fonts_to_embed(doc: &Document) -> Vec<ObjectId> {
    doc.objects
        .iter()
        .filter_map(|(id, object)| {
            let dict = dict_of(object)?;
            let is_font = get_name(doc, dict, b"Type").as_deref() == Some(b"Font");
            let facts = font_facts(doc, dict);
            (is_font && !facts.embedded && !facts.is_type3).then_some(*id)
        })
        .collect()
}

/// embeds one font if the source has one and its licence allows it.
pub(super) fn embed(doc: &mut Document, id: ObjectId, source: &dyn FontSource) -> FontOutcome {
    let dict = doc.get_dictionary(id).cloned().unwrap_or_default();
    let name = get_name(doc, &dict, b"BaseFont")
        .map(|name| String::from_utf8_lossy(&name).into_owned())
        .unwrap_or_default();
    let mut outcome = FontOutcome {
        font: name.clone(),
        embedded_as: None,
        widths_differ: false,
        problem: None,
    };
    if let Some(problem) = unsupported(doc, &dict) {
        outcome.problem = Some(problem);
        return outcome;
    }
    let request = request_for(&name);
    let Some(file) = source.find(&request) else {
        outcome.problem = Some(format!(
            "No font file for {name} was found on this computer."
        ));
        return outcome;
    };
    let characters: String = (FIRST_CODE..=LAST_CODE).filter_map(win_ansi).collect();
    let embedding = match letters_fonts::embedding_data(&file.data, file.index, &characters) {
        Ok(embedding) => embedding,
        Err(letters_fonts::FontError::EmbeddingNotAllowed) => {
            outcome.problem = Some(format!(
                "The licence of {} doesn’t allow embedding it.",
                file.family
            ));
            return outcome;
        }
        Err(error) => {
            outcome.problem = Some(error.to_string());
            return outcome;
        }
    };
    match rewrite_font(doc, id, &dict, &embedding) {
        Ok(()) => {
            outcome.embedded_as = Some(file.family.clone());
            outcome.widths_differ = !file.same_widths;
        }
        Err(problem) => outcome.problem = Some(problem),
    }
    outcome
}

/// only simple fonts in a standard encoding are rewritten; others would
/// need their own character mapping kept.
fn unsupported(doc: &Document, dict: &Dictionary) -> Option<String> {
    let subtype = get_name(doc, dict, b"Subtype").unwrap_or_default();
    if subtype == b"Type0" {
        return Some("Letters can’t embed fonts with two-byte characters yet.".into());
    }
    if dict
        .get(b"Encoding")
        .ok()
        .and_then(|value| value.as_dict().ok())
        .is_some()
        || matches!(dict.get(b"Encoding"), Ok(Object::Reference(_)))
    {
        return Some(
            "The font uses its own character encoding, which Letters can’t carry over yet.".into(),
        );
    }
    let encoding = get_name(doc, dict, b"Encoding");
    let symbolic_standard = get_name(doc, dict, b"BaseFont")
        .is_some_and(|name| name == b"Symbol" || name == b"ZapfDingbats");
    if symbolic_standard
        || encoding.is_some_and(|name| name != b"WinAnsiEncoding" && name != b"StandardEncoding")
    {
        return Some(
            "The font’s characters aren’t Western text Letters can match to a font file.".into(),
        );
    }
    None
}

fn request_for(name: &str) -> FontRequest {
    let clean = name
        .split_once('+')
        .map(|(_, rest)| rest)
        .unwrap_or(name)
        .to_owned();
    let lower = clean.to_ascii_lowercase();
    let family = clean.split(['-', ',']).next().unwrap_or(&clean).to_owned();
    let metric_compatible = letters_fonts::substitute::substitute_for(&family)
        .filter(|known| known.metric_compatible)
        .map(|known| vec![known.with.to_owned()])
        .unwrap_or_default();
    FontRequest {
        bold: lower.contains("bold") || lower.contains("black") || lower.contains("heavy"),
        italic: lower.contains("italic") || lower.contains("oblique"),
        name: clean,
        metric_compatible,
    }
}

fn rewrite_font(
    doc: &mut Document,
    id: ObjectId,
    old: &Dictionary,
    embedding: &letters_fonts::EmbedData,
) -> Result<(), String> {
    let face = ttf_parser::Face::parse(&embedding.bytes, 0)
        .map_err(|_| "the font file can’t be read".to_owned())?;
    let units = face.units_per_em() as f32;
    let scale = |value: f32| (value * 1000.0 / units).round() as i64;
    let widths: Vec<Object> = (FIRST_CODE..=LAST_CODE)
        .map(|code| {
            let advance = win_ansi(code)
                .and_then(|c| face.glyph_index(c))
                .and_then(|glyph| face.glyph_hor_advance(glyph))
                .unwrap_or(0);
            Object::Integer(scale(advance as f32))
        })
        .collect();
    let subset = matches!(embedding.kind, letters_fonts::EmbedKind::TrueTypeSubset);
    let base = if subset {
        format!("{SUBSET_TAG}{}", embedding.postscript_name)
    } else {
        embedding.postscript_name.clone()
    };
    let program_key = match embedding.kind {
        letters_fonts::EmbedKind::WholeOpenType => "FontFile3",
        _ => "FontFile2",
    };
    let mut program_dict = dictionary! { "Length1" => embedding.bytes.len() as i64 };
    if program_key == "FontFile3" {
        program_dict.set("Subtype", Object::Name(b"OpenType".to_vec()));
    }
    let mut program = Stream::new(program_dict, embedding.bytes.clone());
    let _ = program.compress();
    let program = doc.add_object(program);
    let descriptor = descriptor(doc, &face, &base, program_key, program, &scale);
    let to_unicode = doc.add_object(to_unicode_stream());
    let mut font = old.clone();
    font.set("Subtype", Object::Name(b"TrueType".to_vec()));
    font.set("BaseFont", Object::Name(base.into_bytes()));
    font.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    font.set("FirstChar", Object::Integer(FIRST_CODE as i64));
    font.set("LastChar", Object::Integer(LAST_CODE as i64));
    font.set("Widths", Object::Array(widths));
    font.set("FontDescriptor", Object::Reference(descriptor));
    font.set("ToUnicode", Object::Reference(to_unicode));
    if let Ok(target) = doc.get_dictionary_mut(id) {
        *target = font;
    }
    Ok(())
}

fn descriptor(
    doc: &mut Document,
    face: &ttf_parser::Face,
    name: &str,
    program_key: &str,
    program: ObjectId,
    scale: &dyn Fn(f32) -> i64,
) -> ObjectId {
    let bbox = face.global_bounding_box();
    // flags (iso 32000 table 123): 32 nonsymbolic, 64 italic, 1 fixed pitch
    let mut flags = 32;
    if face.is_italic() {
        flags |= 64;
    }
    if face.is_monospaced() {
        flags |= 1;
    }
    let ascent = scale(face.ascender() as f32);
    let descent = scale(face.descender() as f32);
    let cap_height = face
        .capital_height()
        .map(|height| scale(height as f32))
        .unwrap_or(ascent);
    let mut dict = dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(name.as_bytes().to_vec()),
        "Flags" => flags,
        "FontBBox" => vec![
            scale(bbox.x_min as f32).into(), scale(bbox.y_min as f32).into(),
            scale(bbox.x_max as f32).into(), scale(bbox.y_max as f32).into(),
        ],
        "ItalicAngle" => face.italic_angle() as i64,
        "Ascent" => ascent,
        "Descent" => descent,
        "CapHeight" => cap_height,
        // the vertical stem width; fonts don't record it, 80 is the usual
        // value for a regular weight and readers use it only as a hint
        "StemV" => 80,
    };
    dict.set(program_key, Object::Reference(program));
    doc.add_object(Object::Dictionary(dict))
}

/// a ToUnicode cmap (iso 32000 9.10.3) for winansi codes.
fn to_unicode_stream() -> Stream {
    let mut entries = Vec::new();
    for code in FIRST_CODE..=LAST_CODE {
        if let Some(c) = win_ansi(code) {
            entries.push(format!("<{code:02X}> <{:04X}>", c as u32));
        }
    }
    let mut body = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Letters-WinAnsi-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    // bfchar blocks hold at most 100 entries each (adobe technical note 5411)
    for chunk in entries.chunks(100) {
        body.push_str(&format!(
            "{} beginbfchar\n{}\nendbfchar\n",
            chunk.len(),
            chunk.join("\n")
        ));
    }
    body.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    Stream::new(Dictionary::new(), body.into_bytes())
}
