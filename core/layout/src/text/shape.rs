//! shaping: characters to glyphs with advances.
//!
//! word draws latin text with each character's own glyph and advance: no
//! kerning and no ligatures unless a run asks for them (recipe 7.5, e3).
//! for that common case the glyph and advance come straight from the
//! face's cmap and hmtx tables, which is exact and fast. runs that need
//! kerning, ligatures or a complex script go through rustybuzz, a port of
//! harfbuzz.

use crate::fonts::{FontId, LoadedFace};

/// one shaped glyph, in points at the run's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    pub glyph: u16,
    pub font: FontId,
    pub advance: f64,
    pub x_offset: f64,
    pub y_offset: f64,
    /// the index (in chars of the shaped text) of the first character this
    /// glyph belongs to
    pub cluster: usize,
}

/// what a run asks of the shaper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeOptions {
    pub size: f64,
    /// round every advance to this unit in points (libreoffice lays out in
    /// whole twips); none keeps the exact advance
    pub advance_unit: Option<f64>,
    pub kerning: bool,
    pub ligatures: bool,
    pub right_to_left: bool,
}

/// true when every character can be drawn one glyph each, with no
/// reordering or joining: latin, greek, cyrillic and common symbols with
/// no combining marks.
pub fn is_simple_text(text: &[char]) -> bool {
    text.iter().all(|&ch| {
        let code = ch as u32;
        code < 0x0300 || (0x0370..0x0530).contains(&code) || (0x1e00..0x2c00).contains(&code)
    })
}

/// shapes `text` with one face.
pub fn shape(
    face: &LoadedFace,
    font: FontId,
    text: &[char],
    options: ShapeOptions,
) -> Vec<ShapedGlyph> {
    let simple =
        !options.kerning && !options.ligatures && !options.right_to_left && is_simple_text(text);
    let mut glyphs = if simple {
        shape_simple(face, font, text, options.size)
    } else {
        shape_with_rustybuzz(face, font, text, options)
            .unwrap_or_else(|| shape_simple(face, font, text, options.size))
    };
    if let Some(unit) = options.advance_unit {
        for glyph in &mut glyphs {
            glyph.advance = (glyph.advance / unit).round() * unit;
        }
    }
    glyphs
}

/// one glyph per character from cmap and hmtx; characters the face lacks
/// get glyph 0 (the caller looks for a fallback face first).
fn shape_simple(face: &LoadedFace, font: FontId, text: &[char], size: f64) -> Vec<ShapedGlyph> {
    let scale = face.scale(size);
    text.iter()
        .enumerate()
        .map(|(index, &ch)| {
            let (glyph, advance) = face.glyph(ch).unwrap_or((0, 0));
            ShapedGlyph {
                glyph,
                font,
                advance: f64::from(advance) * scale,
                x_offset: 0.0,
                y_offset: 0.0,
                cluster: index,
            }
        })
        .collect()
}

fn shape_with_rustybuzz(
    face: &LoadedFace,
    font: FontId,
    text: &[char],
    options: ShapeOptions,
) -> Option<Vec<ShapedGlyph>> {
    let mut buzz = rustybuzz::Face::from_slice(&face.bytes, face.index)?;
    let variations: Vec<rustybuzz::Variation> = face
        .variations
        .iter()
        .map(|(tag, value)| rustybuzz::Variation {
            tag: rustybuzz::ttf_parser::Tag::from_bytes(tag),
            value: *value,
        })
        .collect();
    buzz.set_variations(&variations);
    let string: String = text.iter().collect();
    // byte offsets of each char, to turn rustybuzz clusters back into chars
    let starts: Vec<usize> = string.char_indices().map(|(at, _)| at).collect();
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(&string);
    buffer.guess_segment_properties();
    if options.right_to_left {
        buffer.set_direction(rustybuzz::Direction::RightToLeft);
    }
    let features = feature_list(options);
    let output = rustybuzz::shape(&buzz, &features, buffer);
    let scale = face.scale(options.size);
    let glyphs = output
        .glyph_infos()
        .iter()
        .zip(output.glyph_positions())
        .map(|(info, position)| ShapedGlyph {
            glyph: info.glyph_id as u16,
            font,
            advance: f64::from(position.x_advance) * scale,
            x_offset: f64::from(position.x_offset) * scale,
            y_offset: f64::from(position.y_offset) * scale,
            cluster: starts.partition_point(|&start| start < info.cluster as usize),
        })
        .collect();
    Some(glyphs)
}

fn feature_list(options: ShapeOptions) -> Vec<rustybuzz::Feature> {
    let switch = |tag: &[u8; 4], on: bool| {
        rustybuzz::Feature::new(
            rustybuzz::ttf_parser::Tag::from_bytes(tag),
            u32::from(on),
            ..,
        )
    };
    vec![
        switch(b"kern", options.kerning),
        switch(b"liga", options.ligatures),
        switch(b"clig", options.ligatures),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_text_is_recognised() {
        assert!(is_simple_text(
            &"Hello, wörld – 3.5".chars().collect::<Vec<_>>()
        ));
        assert!(!is_simple_text(&"שלום".chars().collect::<Vec<_>>()));
        assert!(!is_simple_text(&"e\u{301}".chars().collect::<Vec<_>>()));
    }
}
