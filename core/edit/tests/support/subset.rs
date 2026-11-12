//! a small TrueType subsetter for the tests: keeps only the glyphs some
//! text needs (every glyph keeps its number), the way pdf writers embed
//! fonts, so the tests can make realistic subset fonts from a real font.

use std::collections::{BTreeMap, BTreeSet};

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([bytes[at], bytes[at + 1]])
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

fn tables(font: &[u8]) -> BTreeMap<[u8; 4], Vec<u8>> {
    let count = usize::from(u16_at(font, 4));
    (0..count)
        .map(|record| {
            let at = 12 + record * 16;
            let tag: [u8; 4] = font[at..at + 4].try_into().unwrap();
            let offset = u32_at(font, at + 8) as usize;
            let length = u32_at(font, at + 12) as usize;
            (tag, font[offset..offset + length].to_vec())
        })
        .collect()
}

/// the glyph numbers `text` uses in the font (through its unicode cmap).
pub fn glyphs_for(font: &[u8], text: &str) -> BTreeMap<char, u16> {
    let face = ttf_parser::Face::parse(font, 0).unwrap();
    text.chars()
        .filter_map(|c| Some((c, face.glyph_index(c)?.0)))
        .collect()
}

/// the font with only the glyphs of `text` (and glyph 0) left.
pub fn subset(font: &[u8], text: &str) -> Vec<u8> {
    let mut all = tables(font);
    let keep: BTreeSet<u16> = glyphs_for(font, text)
        .values()
        .copied()
        .chain([0])
        .collect();
    let head = all[b"head"].clone();
    let count = usize::from(u16_at(&all[b"maxp"], 4));
    let long = u16_at(&head, 50) == 1;
    let loca = &all[b"loca"];
    let offset = |glyph: usize| {
        if long {
            u32_at(loca, glyph * 4) as usize
        } else {
            usize::from(u16_at(loca, glyph * 2)) * 2
        }
    };
    let glyf = &all[b"glyf"];
    let mut new_glyf = Vec::new();
    let mut new_loca = Vec::new();
    for glyph in 0..count {
        new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
        if keep.contains(&(glyph as u16)) {
            new_glyf.extend_from_slice(&glyf[offset(glyph)..offset(glyph + 1)]);
            while new_glyf.len() % 4 != 0 {
                new_glyf.push(0);
            }
        }
    }
    new_loca.extend_from_slice(&(new_glyf.len() as u32).to_be_bytes());
    let mut new_head = head;
    new_head[50..52].copy_from_slice(&[0, 1]);
    all.insert(*b"head", new_head);
    all.insert(*b"glyf", new_glyf);
    all.insert(*b"loca", new_loca);
    // keep what a pdf reader needs, as pdf writers do
    let wanted: [&[u8; 4]; 11] = [
        b"head", b"hhea", b"hmtx", b"maxp", b"loca", b"glyf", b"cmap", b"post", b"cvt ", b"fpgm",
        b"prep",
    ];
    let kept: BTreeMap<[u8; 4], Vec<u8>> = all
        .into_iter()
        .filter(|(tag, _)| wanted.contains(&tag))
        .collect();
    super::font::write_font_tables(&kept)
}

/// a glyph's advance in thousandths of the em.
pub fn advance(font: &[u8], glyph: u16) -> f32 {
    let face = ttf_parser::Face::parse(font, 0).unwrap();
    let advance = face
        .glyph_hor_advance(ttf_parser::GlyphId(glyph))
        .unwrap_or(0);
    f32::from(advance) * 1000.0 / f32::from(face.units_per_em())
}
