//! tests for writing completed fonts, on the Atkinson Hyperlegible fixture.

use super::*;
use crate::matching::glyphs::{glyph_shape, read_glyphs};
use crate::matching::shape::tests::square;
use ttf_parser::Face;

fn atkinson() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/AtkinsonHyperlegible-Regular.ttf"
    ))
    .unwrap()
}

fn box_glyph() -> GlyphData {
    GlyphData {
        shape: Shape {
            contours: vec![square(0.05, 0.0, 0.45, 0.7)],
        },
        advance: 0.5,
    }
}

/// "Hello" from the fixture with a box glyph appended at U+E000, kerned
/// after "H": the font, its original map, and the written result.
fn appended() -> (Vec<u8>, BTreeMap<char, u16>, WrittenFont) {
    let base = atkinson();
    let face = Face::parse(&base, 0).unwrap();
    let original: BTreeMap<char, u16> = "Hello"
        .chars()
        .map(|c| (c, face.glyph_index(c).unwrap().0))
        .collect();
    let metrics = crate::matching::glyphs::metrics_of(&face);
    let new = vec![('\u{E000}', box_glyph())];
    let written = write_completed_font(&FontToWrite {
        base: Some(&base),
        metrics: &metrics,
        original_ids: &original,
        original_glyphs: &BTreeMap::new(),
        new_glyphs: &new,
        kerning: &[('H', '\u{E000}', -0.02)],
        family: "Letters Completed 0001",
        fs_type: 0,
    })
    .unwrap();
    (base, original, written)
}

#[test]
fn appended_glyphs_leave_old_ones_byte_for_byte() {
    let (base, original, written) = appended();
    let face = Face::parse(&base, 0).unwrap();
    let out = Face::parse(&written.bytes, 0).unwrap();
    assert_eq!(out.number_of_glyphs(), face.number_of_glyphs() + 1);
    for (c, glyph) in &original {
        assert_eq!(
            out.glyph_index(*c).unwrap().0,
            *glyph,
            "{c} kept its number"
        );
        assert_eq!(
            glyph_shape(&out, ttf_parser::GlyphId(*glyph)),
            glyph_shape(&face, ttf_parser::GlyphId(*glyph))
        );
        assert_eq!(
            out.glyph_hor_advance(ttf_parser::GlyphId(*glyph)),
            face.glyph_hor_advance(ttf_parser::GlyphId(*glyph))
        );
    }
    // the old glyph data is the same bytes
    let old_tables = read_tables(&base, 0).unwrap();
    let new_tables = read_tables(&written.bytes, 0).unwrap();
    let h = usize::from(original[&'H']);
    let old_offsets = read_loca(
        &old_tables[b"loca"],
        usize::from(face.number_of_glyphs()),
        read_i16(&old_tables[b"head"], 50) == Some(1),
    )
    .unwrap();
    let new_offsets = read_loca(
        &new_tables[b"loca"],
        usize::from(out.number_of_glyphs()),
        true,
    )
    .unwrap();
    assert_eq!(
        &old_tables[b"glyf"][old_offsets[h]..old_offsets[h + 1]],
        &new_tables[b"glyf"]
            [new_offsets[h]..new_offsets[h] + (old_offsets[h + 1] - old_offsets[h])]
    );
}

#[test]
fn appended_glyphs_are_mapped_measured_kerned_and_neutrally_named() {
    let (base, original, written) = appended();
    let face = Face::parse(&base, 0).unwrap();
    let out = Face::parse(&written.bytes, 0).unwrap();
    let added = out.glyph_index('\u{E000}').unwrap();
    assert_eq!(added.0, face.number_of_glyphs());
    let units = f64::from(out.units_per_em());
    assert_eq!(
        out.glyph_hor_advance(added),
        Some((0.5 * units).round() as u16)
    );
    let bounds = out.glyph_bounding_box(added).unwrap();
    assert_eq!(bounds.y_max, (0.7 * units).round() as i16);
    let kern = out.tables().kern.unwrap();
    let value = kern
        .subtables
        .into_iter()
        .find_map(|t| t.glyphs_kerning(ttf_parser::GlyphId(original[&'H']), added));
    assert_eq!(value, Some((-0.02 * units).round() as i16));
    // the name is the neutral one
    assert_eq!(
        crate::facts::name(&out, ttf_parser::name_id::FAMILY).as_deref(),
        Some("Letters Completed 0001")
    );
    // and the file's checksum adjustment is right
    assert_eq!(checksum(&written.bytes), 0xB1B0_AFBA);
}

#[test]
fn fonts_from_outlines_alone_are_valid() {
    let set = read_glyphs(&atkinson(), 0, &[], "ab".chars()).unwrap();
    let new = vec![('c', box_glyph())];
    let written = write_completed_font(&FontToWrite {
        base: None,
        metrics: &set.metrics,
        original_ids: &BTreeMap::new(),
        original_glyphs: &set.glyphs,
        new_glyphs: &new,
        kerning: &[],
        family: "Letters Completed 0002",
        fs_type: 8,
    })
    .unwrap();
    let out = Face::parse(&written.bytes, 0).unwrap();
    assert_eq!(out.number_of_glyphs(), 4);
    let a = read_glyphs(&written.bytes, 0, &[], "abc".chars()).unwrap();
    let distance = crate::matching::distance::shape_distance(
        &crate::matching::Samples::of(&a.glyphs[&'a'].shape),
        &crate::matching::Samples::of(&set.glyphs[&'a'].shape),
    );
    // only rounding to font units differs
    assert!(distance.max < 0.002, "{distance:?}");
    assert!(a.has('c'));
    let facts = crate::facts::facts_of(&out).unwrap();
    assert_eq!(
        facts.embedding.permission,
        crate::model::EmbeddingPermission::Editable
    );
}

#[test]
fn points_use_short_forms() {
    let mut out = Vec::new();
    encode_points(
        &[(10, 0, true), (10, 300, false), (-5, 300, true)],
        &mut out,
    );
    // three flags, then x: 10 (short), same, -15 (short); y: same, 300 (word), same
    assert_eq!(out.len(), 3 + 2 + 2);
    assert_eq!(out[0], 0x01 | 0x02 | 0x10 | 0x20);
}

#[test]
fn broken_bases_are_refused() {
    let metrics = FontMetrics::default();
    let result = write_completed_font(&FontToWrite {
        base: Some(b"junk"),
        metrics: &metrics,
        original_ids: &BTreeMap::new(),
        original_glyphs: &BTreeMap::new(),
        new_glyphs: &[],
        kerning: &[],
        family: "X",
        fs_type: 0,
    });
    assert!(result.is_err());
}
