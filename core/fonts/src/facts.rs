//! reading the facts of one face: its names, weight, width and slant,
//! whether it is monospaced or variable, what it can write, what kind of
//! font it is, what its licence allows and the measurements used to find
//! look-alikes.

use ttf_parser::{name_id, Face, GlyphId, Tag};

use crate::coverage;
use crate::model::{Axis, Category, Embedding, FaceFacts, Measurements, Outlines};
use crate::outline::glyph_segments;
use crate::raster::fill;
use crate::sfnt::{read_i16, read_u16};

/// the facts of the face at `index` in a plain font file, or None when
/// it can't be read or has no name (a font without a name is unusable).
pub fn read_facts(data: &[u8], index: u32) -> Option<FaceFacts> {
    let face = Face::parse(data, index).ok()?;
    facts_of(&face)
}

pub fn facts_of(face: &Face) -> Option<FaceFacts> {
    let family = name(face, name_id::TYPOGRAPHIC_FAMILY)
        .or_else(|| name(face, name_id::FAMILY))
        .filter(|family| !family.trim().is_empty())?;
    let style_name = name(face, name_id::TYPOGRAPHIC_SUBFAMILY)
        .or_else(|| name(face, name_id::SUBFAMILY))
        .unwrap_or_else(|| "Regular".to_string());
    let os2 = face
        .raw_face()
        .table(Tag::from_bytes(b"OS/2"))
        .unwrap_or(&[]);
    let panose = panose(os2);
    let measurements = measure(face, panose);
    let monospace = face.is_monospaced() || panose[3] == 9 || equal_letter_widths(&measurements);
    let category = guess_category(&family, panose, family_class(os2), monospace);
    let coverage = character_ranges(face);
    Some(FaceFacts {
        writing: coverage::writing_of(|c| coverage::ranges_contain(&coverage, c)),
        coverage,
        full_name: name(face, name_id::FULL_NAME)
            .unwrap_or_else(|| format!("{family} {style_name}")),
        postscript_name: name(face, name_id::POST_SCRIPT_NAME).unwrap_or_default(),
        weight: face.weight().to_number().clamp(1, 1000),
        width: face.width().to_number(),
        italic: face.is_italic(),
        oblique: face.is_oblique(),
        monospace,
        axes: axes(face),
        category,
        embedding: Embedding::from_fs_type(read_u16(os2, 8).unwrap_or(0)),
        version: name(face, name_id::VERSION)
            .map(|version| version.trim_start_matches("Version ").to_string())
            .unwrap_or_default(),
        glyph_count: face.number_of_glyphs(),
        outlines: outlines(face),
        measurements,
        family,
        style_name,
    })
}

/// a name from the name table, preferring english windows names, which
/// every font has; mac roman names are read as latin-1 when that's all.
pub fn name(face: &Face, id: u16) -> Option<String> {
    let names = face.names();
    let mut fallback = None;
    for record in names.into_iter().filter(|record| record.name_id == id) {
        if let Some(text) = record.to_string() {
            let text = text.trim_matches(char::from(0)).trim().to_string();
            if text.is_empty() {
                continue;
            }
            if record.language_id == 0x0409 || record.language_id == 0 {
                return Some(text);
            }
            fallback.get_or_insert(text);
        } else if record.platform_id == ttf_parser::PlatformId::Macintosh && fallback.is_none() {
            let text: String = record.name.iter().map(|byte| char::from(*byte)).collect();
            if !text.trim().is_empty() {
                fallback = Some(text.trim().to_string());
            }
        }
    }
    fallback
}

/// fonts with more characters than this are cut off there; no real font
/// comes close (the largest CJK fonts map about 65 000).
const MAX_CODE_POINTS: usize = 300_000;

/// the characters the font's unicode character maps reach, as ranges.
fn character_ranges(face: &Face) -> Vec<(u32, u32)> {
    let Some(cmap) = face.tables().cmap else {
        return Vec::new();
    };
    let mut points = Vec::new();
    for subtable in cmap
        .subtables
        .into_iter()
        .filter(|table| table.is_unicode())
    {
        subtable.codepoints(|point| {
            if points.len() < MAX_CODE_POINTS
                && subtable
                    .glyph_index(point)
                    .is_some_and(|glyph| glyph.0 != 0)
            {
                points.push(point);
            }
        });
    }
    coverage::merge_ranges(points)
}

fn panose(os2: &[u8]) -> [u8; 10] {
    let mut panose = [0u8; 10];
    if let Some(bytes) = os2.get(32..42) {
        panose.copy_from_slice(bytes);
    }
    panose
}

/// the IBM font class in OS/2 (its high byte is the class).
fn family_class(os2: &[u8]) -> u8 {
    read_i16(os2, 30)
        .map(|class| (class >> 8) as u8)
        .unwrap_or(0)
}

fn axes(face: &Face) -> Vec<Axis> {
    face.variation_axes()
        .into_iter()
        .map(|axis| Axis {
            tag: String::from_utf8_lossy(&axis.tag.to_bytes()).into_owned(),
            min: axis.min_value,
            default: axis.def_value,
            max: axis.max_value,
        })
        .collect()
}

fn outlines(face: &Face) -> Outlines {
    let tables = face.tables();
    if tables.glyf.is_some() {
        Outlines::TrueType
    } else if tables.cff.is_some() || tables.cff2.is_some() {
        Outlines::Cff
    } else {
        Outlines::Bitmap
    }
}

/// the printable ascii characters, in order, whose widths are compared.
pub const ASCII_SAMPLE: std::ops::RangeInclusive<char> = ' '..='~';

fn measure(face: &Face, panose: [u8; 10]) -> Measurements {
    let units = face.units_per_em().max(16);
    let per_mille = |value: f32| value * 1000.0 / f32::from(units);
    let ascii_widths = ASCII_SAMPLE
        .map(|c| {
            face.glyph_index(c)
                .and_then(|glyph| face.glyph_hor_advance(glyph))
                .map(|advance| per_mille(f32::from(advance)).round().clamp(0.0, 65535.0) as u16)
                .unwrap_or(0)
        })
        .collect();
    let glyph_top = |c: char| {
        face.glyph_index(c)
            .and_then(|glyph| face.glyph_bounding_box(glyph))
            .map(|bounds| bounds.y_max)
    };
    let x_height = face
        .x_height()
        .filter(|h| *h > 0)
        .or_else(|| glyph_top('x'))
        .unwrap_or(0);
    let cap_height = face
        .capital_height()
        .filter(|h| *h > 0)
        .or_else(|| glyph_top('H'))
        .unwrap_or(0);
    let (stem, contrast) = stroke_of_o(face).unwrap_or((0.0, 1.0));
    let scale_i16 = |value: i16| per_mille(f32::from(value)).round() as i16;
    Measurements {
        units_per_em: units,
        ascender: scale_i16(face.ascender()),
        descender: scale_i16(face.descender()),
        x_height: scale_i16(x_height),
        cap_height: scale_i16(cap_height),
        panose,
        ascii_widths,
        contrast,
        stem,
    }
}

/// draws "o" and measures its strokes: the thick one on the left side
/// and the thin one at the top. their ratio is the font's contrast (high
/// for didone serifs, near 1 for geometric sans), a quick way to tell
/// fonts with the same widths but different looks apart.
fn stroke_of_o(face: &Face) -> Option<(f32, f32)> {
    let glyph: GlyphId = face.glyph_index('o')?;
    let bounds = face.glyph_bounding_box(glyph)?;
    let size = 160.0f32;
    let scale = size / f32::from(face.units_per_em().max(16));
    let width = ((f32::from(bounds.x_max) - f32::from(bounds.x_min)) * scale).ceil() as usize + 4;
    let height = ((f32::from(bounds.y_max) - f32::from(bounds.y_min)) * scale).ceil() as usize + 4;
    if width < 8 || height < 8 || width > 1000 || height > 1000 {
        return None;
    }
    let x = 2.0 - f32::from(bounds.x_min) * scale;
    let baseline = 2.0 + f32::from(bounds.y_max) * scale;
    let bitmap = fill(
        &glyph_segments(face, glyph, scale, x, baseline),
        width,
        height,
    );
    let inked = |value: u8| value >= 128;
    let middle_row = height / 2;
    let horizontal = (0..width)
        .skip_while(|column| !inked(bitmap.at(*column, middle_row)))
        .take_while(|column| inked(bitmap.at(*column, middle_row)))
        .count();
    let middle_column = width / 2;
    let vertical = (0..height)
        .skip_while(|row| !inked(bitmap.at(middle_column, *row)))
        .take_while(|row| inked(bitmap.at(middle_column, *row)))
        .count();
    if horizontal == 0 || vertical == 0 {
        return None;
    }
    let stem = horizontal as f32 * 1000.0 / size;
    Some((stem, horizontal as f32 / vertical as f32))
}

/// some monospaced fonts forget to say so; every letter and digit
/// having one width gives them away.
fn equal_letter_widths(measurements: &Measurements) -> bool {
    let letters: Vec<u16> = ASCII_SAMPLE
        .zip(&measurements.ascii_widths)
        .filter(|(c, _)| c.is_ascii_alphanumeric())
        .map(|(_, width)| *width)
        .collect();
    letters.len() == 62
        && letters
            .iter()
            .all(|width| *width > 0 && *width == letters[0])
}

/// the kind of font, from what the font says about itself: its panose
/// numbers, its IBM class, and words in its name.
pub fn guess_category(family: &str, panose: [u8; 10], class: u8, monospace: bool) -> Category {
    let name = family.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| name.contains(word));
    if has(&[
        "symbol", "dingbat", "wingding", "webding", "emoji", "marlett", "icons", "ornament",
    ]) {
        return Category::Symbol;
    }
    if monospace || has(&["mono", "code", "courier", "consol", "typewriter"]) {
        return Category::Monospace;
    }
    if has(&[
        "script", "hand", "brush", "ink free", "marker", "print", "comic",
    ]) || panose[0] == 3
    {
        return Category::Handwriting;
    }
    match panose[0] {
        4 => return Category::Display,
        5 => return Category::Symbol,
        _ => {}
    }
    if has(&["sans"]) {
        return Category::SansSerif;
    }
    if panose[0] == 2 {
        match panose[1] {
            2..=10 => return Category::Serif,
            11..=15 => return Category::SansSerif,
            _ => {}
        }
    }
    match class {
        1..=5 | 7 => Category::Serif,
        8 => Category::SansSerif,
        9 => Category::Display,
        10 => Category::Handwriting,
        12 => Category::Symbol,
        _ if has(&["serif", "roman", "times", "garamond", "book"]) => Category::Serif,
        _ => Category::SansSerif,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_come_from_panose_class_and_name() {
        let none = [0; 10];
        let mut sans = [0; 10];
        sans[0] = 2;
        sans[1] = 11;
        let mut serif = [0; 10];
        serif[0] = 2;
        serif[1] = 2;
        assert_eq!(guess_category("Arial", sans, 8, false), Category::SansSerif);
        assert_eq!(
            guess_category("Times New Roman", serif, 1, false),
            Category::Serif
        );
        assert_eq!(
            guess_category("Courier New", serif, 0, true),
            Category::Monospace
        );
        assert_eq!(
            guess_category("Segoe Script", none, 0, false),
            Category::Handwriting
        );
        assert_eq!(
            guess_category("Wingdings", none, 0, false),
            Category::Symbol
        );
        assert_eq!(
            guess_category("Noto Sans Display", serif, 0, false),
            Category::SansSerif
        );
        assert_eq!(
            guess_category("Mystery", none, 0, false),
            Category::SansSerif
        );
        assert_eq!(guess_category("Old Book", none, 0, false), Category::Serif);
    }
}
