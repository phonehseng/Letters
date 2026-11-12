//! a tiny truetype font with a single plain glyph, for invisible text.
//!
//! recognised text is written into the pdf invisibly, so the font's shapes
//! never show. but a font for characters beyond the built-in western set
//! has to be stored in the file, and storing a real font with every script
//! in it would add megabytes. this font has one glyph, a plain box, that
//! every character uses; what each character *means* is told to readers
//! separately (the tounicode map in `fonts.rs`), and that is all that
//! selecting, searching and copying need. tesseract's own pdf output uses
//! the same idea ("glyphless" text). the box is never drawn, but readers
//! measure it for selection highlights, so it spans a normal line height.
//!
//! the file is built here from its tables rather than shipped as bytes so
//! that every part of it can be read and checked.

/// glyph sizes are in thousandths of the font size, like pdf widths.
const UNITS_PER_EM: u16 = 1000;
/// every glyph is half as wide as it is tall; the text layer stretches
/// each word to its box anyway (see `text_layer.rs`)
pub const GLYPH_WIDTH: u16 = 500;
/// how far the box reaches above and below the baseline, like the tall
/// letters and the tails of ordinary type
pub const ASCENT: i16 = 800;
pub const DESCENT: i16 = -200;

/// the font file's bytes.
pub fn font_file() -> Vec<u8> {
    let mut tables = vec![
        (*b"cmap", cmap()),
        (*b"glyf", glyf()),
        (*b"head", head()),
        (*b"hhea", hhea()),
        (*b"hmtx", hmtx()),
        (*b"loca", loca()),
        (*b"maxp", maxp()),
        (*b"post", post()),
    ];
    // readers look tables up by binary search, so they must be in tag order
    tables.sort_by_key(|(tag, _)| *tag);
    let mut font = assemble(&tables);
    // the head table holds a value that makes the whole file's checksum
    // come out to a fixed number
    let head_offset = table_offset(&font, b"head");
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&font));
    font[head_offset + 8..head_offset + 12].copy_from_slice(&adjustment.to_be_bytes());
    font
}

/// the outlines. glyph 0 (".notdef", for a missing character) is empty;
/// glyph 1, which every character uses, is one box from the descent to the
/// ascent, the full advance wide.
fn glyf() -> Vec<u8> {
    let width = GLYPH_WIDTH as i16;
    let height = ASCENT - DESCENT;
    let mut table = Vec::new();
    push_i16(&mut table, 1); // one contour
    for value in [0, DESCENT, width, ASCENT] {
        push_i16(&mut table, value); // its bounding box
    }
    push_u16(&mut table, 3); // the contour ends at point 3
    push_u16(&mut table, 0); // no hinting instructions
    table.extend_from_slice(&[0x01; 4]); // four points, all on the outline
                                         // each point is written as a step from the one before, starting at
                                         // 0, 0. truetype goes round outer outlines clockwise: up the left side,
                                         // across the top, down the right side
    for x in [0, 0, width, 0] {
        push_i16(&mut table, x);
    }
    for y in [DESCENT, height, 0, -height] {
        push_i16(&mut table, y);
    }
    pad(&mut table);
    table
}

/// where each glyph starts in `glyf`, in units of two bytes. glyph 0
/// starts and ends at the same place, which is how truetype says "empty".
fn loca() -> Vec<u8> {
    let end = (glyf().len() / 2) as u16;
    let mut table = Vec::new();
    for offset in [0, 0, end] {
        push_u16(&mut table, offset);
    }
    table
}

fn head() -> Vec<u8> {
    let mut table = Vec::new();
    push_u32(&mut table, 0x0001_0000); // version
    push_u32(&mut table, 0x0001_0000); // font revision
    push_u32(&mut table, 0); // checksum adjustment, filled in at the end
    push_u32(&mut table, 0x5F0F_3CF5); // the magic number every head has
    push_u16(&mut table, 0x000B); // baseline at 0, integer scaling
    push_u16(&mut table, UNITS_PER_EM);
    table.extend_from_slice(&[0; 16]); // created and modified dates
    for value in [0, DESCENT, GLYPH_WIDTH as i16, ASCENT] {
        push_i16(&mut table, value); // bounding box of all glyphs
    }
    push_u16(&mut table, 0); // regular style
    push_u16(&mut table, 3); // smallest readable size in pixels
    push_i16(&mut table, 2); // left to right
    push_i16(&mut table, 0); // `loca` uses two byte offsets
    push_i16(&mut table, 0); // glyph data format
    table
}

fn hhea() -> Vec<u8> {
    let mut table = Vec::new();
    push_u32(&mut table, 0x0001_0000);
    push_i16(&mut table, ASCENT);
    push_i16(&mut table, DESCENT);
    push_i16(&mut table, 0); // line gap
    push_u16(&mut table, GLYPH_WIDTH); // widest advance
    push_i16(&mut table, 0); // smallest left side bearing
    push_i16(&mut table, 0); // smallest right side bearing
    push_i16(&mut table, GLYPH_WIDTH as i16); // furthest extent
    push_i16(&mut table, 1); // caret slope: upright
    push_i16(&mut table, 0);
    push_i16(&mut table, 0); // caret offset
    table.extend_from_slice(&[0; 8]); // reserved
    push_i16(&mut table, 0); // metric data format
    push_u16(&mut table, 2); // both glyphs have their own width
    table
}

fn hmtx() -> Vec<u8> {
    let mut table = Vec::new();
    for _ in 0..2 {
        push_u16(&mut table, GLYPH_WIDTH);
        push_i16(&mut table, 0);
    }
    table
}

fn maxp() -> Vec<u8> {
    let mut table = Vec::new();
    push_u32(&mut table, 0x0001_0000);
    push_u16(&mut table, 2); // glyphs
    push_u16(&mut table, 4); // most points in a glyph
    push_u16(&mut table, 1); // most contours in a glyph
    push_u16(&mut table, 0); // composite points
    push_u16(&mut table, 0); // composite contours
    push_u16(&mut table, 2); // zones (no twilight zone is used)
    table.extend_from_slice(&[0; 16]); // no hinting storage of any kind
    table
}

/// version 3 of `post`: no glyph names, just the basics.
fn post() -> Vec<u8> {
    let mut table = Vec::new();
    push_u32(&mut table, 0x0003_0000);
    table.extend_from_slice(&[0; 28]);
    table
}

/// a character map with no characters: pdf picks glyphs by number
/// (the cid to gid map), but some readers insist the table exists.
fn cmap() -> Vec<u8> {
    let mut table = Vec::new();
    push_u16(&mut table, 0); // version
    push_u16(&mut table, 1); // one encoding
    push_u16(&mut table, 3); // windows
    push_u16(&mut table, 1); // unicode
    push_u32(&mut table, 12); // where the map starts
                              // format 4 with just the closing segment every format 4 map ends with
    for value in [4, 24, 0, 2, 2, 0, 0, 0xFFFF, 0, 0xFFFF, 1, 0] {
        push_u16(&mut table, value);
    }
    table
}

/// puts the tables after the directory that lists them.
fn assemble(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let count = tables.len() as u16;
    let largest_power = 1u16 << (15 - count.leading_zeros());
    let mut font = Vec::new();
    push_u32(&mut font, 0x0001_0000); // truetype outlines
    push_u16(&mut font, count);
    push_u16(&mut font, largest_power * 16); // search range
    push_u16(&mut font, largest_power.trailing_zeros() as u16);
    push_u16(&mut font, count * 16 - largest_power * 16);
    let mut offset = 12 + 16 * tables.len();
    for (tag, data) in tables {
        font.extend_from_slice(tag);
        push_u32(&mut font, checksum(data));
        push_u32(&mut font, offset as u32);
        push_u32(&mut font, data.len() as u32);
        offset += data.len().div_ceil(4) * 4;
    }
    for (_, data) in tables {
        font.extend_from_slice(data);
        pad(&mut font);
    }
    font
}

fn table_offset(font: &[u8], wanted: &[u8; 4]) -> usize {
    let count = u16::from_be_bytes([font[4], font[5]]) as usize;
    (0..count)
        .map(|index| 12 + index * 16)
        .find(|&record| &font[record..record + 4] == wanted)
        .map(|record| {
            u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap()) as usize
        })
        .expect("the table was just written")
}

/// the truetype checksum: the sum of the data read as 32 bit numbers.
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

fn pad(bytes: &mut Vec<u8>) {
    while !bytes.len().is_multiple_of(4) {
        bytes.push(0);
    }
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_i16(bytes: &mut Vec<u8>, value: i16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_listed_in_order_and_aligned() {
        let font = font_file();
        let count = u16::from_be_bytes([font[4], font[5]]) as usize;
        assert_eq!(count, 8);
        let mut previous = [0u8; 4];
        for index in 0..count {
            let record = 12 + index * 16;
            let tag: [u8; 4] = font[record..record + 4].try_into().unwrap();
            let offset = u32::from_be_bytes(font[record + 8..record + 12].try_into().unwrap());
            let length = u32::from_be_bytes(font[record + 12..record + 16].try_into().unwrap());
            assert!(tag > previous, "tables out of order");
            assert_eq!(offset % 4, 0);
            assert!(offset as usize + length as usize <= font.len());
            previous = tag;
        }
    }

    #[test]
    fn the_whole_file_checksum_is_the_fixed_value() {
        assert_eq!(checksum(&font_file()), 0xB1B0_AFBA);
    }

    #[test]
    fn table_sizes_match_the_format() {
        assert_eq!(head().len(), 54);
        assert_eq!(hhea().len(), 36);
        assert_eq!(maxp().len(), 32);
        assert_eq!(post().len(), 32);
        assert_eq!(loca().len(), 6);
    }
}
