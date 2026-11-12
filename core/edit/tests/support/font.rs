//! a tiny TrueType font made for the tests: every glyph is a filled box.
//!
//! boxes are easy to find in a rendered page, and the font can be made
//! with any set of characters and any embedding permission (OS/2 fsType),
//! which real fonts can't offer.

use std::collections::BTreeMap;

/// one glyph: the character, its advance and its box height (font units,
/// 1000 to the em).
pub struct BoxGlyph {
    pub character: char,
    pub advance: u16,
    pub height: i16,
}

pub fn glyph(character: char, advance: u16, height: i16) -> BoxGlyph {
    BoxGlyph {
        character,
        advance,
        height,
    }
}

/// the box glyphs a few tests use: space, H, e, l, o.
pub fn hello_glyphs() -> Vec<BoxGlyph> {
    vec![
        glyph(' ', 300, 0),
        glyph('H', 700, 700),
        glyph('e', 550, 500),
        glyph('l', 250, 700),
        glyph('o', 550, 500),
    ]
}

fn u16s(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect()
}

/// writes the font file. glyph 0 is the empty "missing" glyph; the given
/// glyphs follow in order.
pub fn box_font(name: &str, glyphs: &[BoxGlyph], fs_type: u16) -> Vec<u8> {
    let count = glyphs.len() as u16 + 1;
    let mut glyf = Vec::new();
    // glyph 0 (the missing glyph) is empty: it starts and ends at 0
    let mut loca = vec![0u32, 0];
    for glyph in glyphs {
        if glyph.height > 0 {
            let (left, right, top) = (50i16, glyph.advance as i16 - 50, glyph.height);
            let mut outline = Vec::new();
            for value in [1i16, left, 0, right, top] {
                outline.extend_from_slice(&value.to_be_bytes());
            }
            outline.extend_from_slice(&3u16.to_be_bytes()); // last point of the contour
            outline.extend_from_slice(&0u16.to_be_bytes()); // no instructions
            outline.extend_from_slice(&[1, 1, 1, 1]); // four on-curve points
            for delta in [left, right - left, 0, left - right] {
                outline.extend_from_slice(&delta.to_be_bytes());
            }
            for delta in [0i16, 0, top, 0] {
                outline.extend_from_slice(&delta.to_be_bytes());
            }
            glyf.extend(outline);
            while glyf.len() % 4 != 0 {
                glyf.push(0);
            }
        }
        loca.push(glyf.len() as u32);
    }
    let loca: Vec<u8> = loca
        .iter()
        .flat_map(|offset| offset.to_be_bytes())
        .collect();

    let mut head = Vec::new();
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    head.extend_from_slice(&0u32.to_be_bytes());
    head.extend_from_slice(&0x5f0f_3cf5u32.to_be_bytes());
    head.extend(u16s(&[0, 1000]));
    head.extend_from_slice(&[0; 16]);
    head.extend(u16s(&[0, 0, 1000, 800, 0, 8, 2]));
    head.extend(u16s(&[1, 0])); // long loca offsets, glyph data format 0

    let max_advance = glyphs.iter().map(|g| g.advance).max().unwrap_or(500);
    let mut hhea = Vec::new();
    hhea.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea.extend(u16s(&[
        800,
        (-200i16) as u16,
        0,
        max_advance,
        0,
        0,
        max_advance,
        1,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        count,
    ]));

    let mut maxp = Vec::new();
    maxp.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    maxp.extend(u16s(&[count, 4, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0]));

    let mut hmtx = u16s(&[500, 0]);
    for glyph in glyphs {
        hmtx.extend(u16s(&[glyph.advance, 50]));
    }

    // a format 4 unicode cmap with one segment per character
    let mut mapped: Vec<(u16, u16)> = glyphs
        .iter()
        .enumerate()
        .map(|(index, g)| (g.character as u16, index as u16 + 1))
        .collect();
    mapped.sort_unstable();
    mapped.push((0xffff, 0));
    let segments = mapped.len() as u16;
    let mut table = u16s(&[4, 0, 0, segments * 2, 0, 0, 0]);
    table.extend(mapped.iter().flat_map(|(code, _)| code.to_be_bytes()));
    table.extend(u16s(&[0]));
    table.extend(mapped.iter().flat_map(|(code, _)| code.to_be_bytes()));
    table.extend(mapped.iter().flat_map(|(code, glyph)| {
        if *code == 0xffff {
            1u16
        } else {
            glyph.wrapping_sub(*code)
        }
        .to_be_bytes()
    }));
    table.extend(mapped.iter().flat_map(|_| 0u16.to_be_bytes()));
    let length = table.len() as u16;
    table[2..4].copy_from_slice(&length.to_be_bytes());
    let mut cmap = u16s(&[0, 1, 3, 1]);
    cmap.extend_from_slice(&12u32.to_be_bytes());
    cmap.extend(table);

    let mut post = Vec::new();
    post.extend_from_slice(&0x0003_0000u32.to_be_bytes());
    post.extend_from_slice(&[0; 28]);

    let mut os2 = vec![0u8; 86];
    os2[0..2].copy_from_slice(&1u16.to_be_bytes());
    os2[4..6].copy_from_slice(&400u16.to_be_bytes());
    os2[8..10].copy_from_slice(&fs_type.to_be_bytes());
    os2[68..70].copy_from_slice(&800i16.to_be_bytes());
    os2[70..72].copy_from_slice(&(-200i16).to_be_bytes());
    os2[74..76].copy_from_slice(&800u16.to_be_bytes());
    os2[76..78].copy_from_slice(&200u16.to_be_bytes());

    let names = [(1u16, name), (2, "Regular"), (4, name), (6, name)];
    let mut strings = Vec::new();
    let mut records = Vec::new();
    for (id, text) in names {
        let encoded: Vec<u8> = text
            .encode_utf16()
            .flat_map(|unit| unit.to_be_bytes())
            .collect();
        records.extend(u16s(&[
            3,
            1,
            0x409,
            id,
            encoded.len() as u16,
            strings.len() as u16,
        ]));
        strings.extend(encoded);
    }
    let mut name_table = u16s(&[0, names.len() as u16, 6 + records.len() as u16]);
    name_table.extend(records);
    name_table.extend(strings);

    let tables = BTreeMap::from([
        (*b"OS/2", os2),
        (*b"cmap", cmap),
        (*b"glyf", glyf),
        (*b"head", head),
        (*b"hhea", hhea),
        (*b"hmtx", hmtx),
        (*b"loca", loca),
        (*b"maxp", maxp),
        (*b"name", name_table),
        (*b"post", post),
    ]);
    write_font_tables(&tables)
}

/// writes tables into a font file with the directory readers expect.
pub fn write_font_tables(tables: &BTreeMap<[u8; 4], Vec<u8>>) -> Vec<u8> {
    let count = tables.len() as u16;
    let mut file = Vec::new();
    file.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    file.extend(u16s(&[count, 128, 3, count * 16 - 128]));
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, bytes) in tables {
        file.extend_from_slice(tag);
        file.extend_from_slice(&0u32.to_be_bytes());
        file.extend_from_slice(&(offset as u32).to_be_bytes());
        file.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        body.extend_from_slice(bytes);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    file.extend(body);
    file
}
