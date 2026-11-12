//! WOFF 2.0: an sfnt font compressed with brotli, whose glyf, loca and
//! hmtx tables may also be stored in a smaller "transformed" form. this
//! decodes it back to a plain font, following the W3C WOFF2 spec.
//!
//! every read is bounds checked and every size is capped: a woff2 file
//! is untrusted input, and its sizes are claims that may be lies.
//! collections (several fonts in one woff2) are refused; they are rare
//! and the .ttc file can be added instead.

use std::io::Read;

use crate::sfnt::{read_u16, write_font, Tables};
use crate::woff::MAX_DECODED_BYTES;

/// the tags a table directory entry can name with one byte (spec 5.1).
const KNOWN_TAGS: [&[u8; 4]; 63] = [
    b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"OS/2", b"post", b"cvt ", b"fpgm",
    b"glyf", b"loca", b"prep", b"CFF ", b"VORG", b"EBDT", b"EBLC", b"gasp", b"hdmx", b"kern",
    b"LTSH", b"PCLT", b"VDMX", b"vhea", b"vmtx", b"BASE", b"GDEF", b"GPOS", b"GSUB", b"EBSC",
    b"JSTF", b"MATH", b"CBDT", b"CBLC", b"COLR", b"CPAL", b"SVG ", b"sbix", b"acnt", b"avar",
    b"bdat", b"bloc", b"bsln", b"cvar", b"fdsc", b"feat", b"fmtx", b"fvar", b"gvar", b"hsty",
    b"just", b"lcar", b"mort", b"morx", b"opbd", b"prop", b"trak", b"Zapf", b"Silf", b"Glat",
    b"Gloc", b"Feat", b"Sill",
];

/// reads the file from front to back, refusing to read past its end.
struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn bytes(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let slice = self.data.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes = self.bytes(2)?;
        Some(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn i16(&mut self) -> Option<i16> {
        self.u16().map(|value| value as i16)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.bytes(4)?.try_into().ok()?))
    }

    /// the spec's UIntBase128: seven bits a byte, at most five bytes.
    fn base128(&mut self) -> Option<u32> {
        let mut value: u32 = 0;
        for index in 0..5 {
            let byte = self.u8()?;
            // a leading zero byte is not allowed
            if index == 0 && byte == 0x80 {
                return None;
            }
            if value & 0xfe00_0000 != 0 {
                return None;
            }
            value = (value << 7) | u32::from(byte & 0x7f);
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    /// the spec's 255UInt16.
    fn u255(&mut self) -> Option<u16> {
        match self.u8()? {
            253 => self.u16(),
            255 => Some(u16::from(self.u8()?) + 253),
            254 => Some(u16::from(self.u8()?) + 506),
            code => Some(u16::from(code)),
        }
    }
}

struct TableEntry {
    tag: [u8; 4],
    original_length: usize,
    /// the length in the decompressed stream (transformed or not)
    stored_length: usize,
    transformed: bool,
}

/// the plain font inside a WOFF2 file, or None when the file is damaged
/// or uses something this decoder refuses (a collection).
pub fn decode(data: &[u8]) -> Option<Vec<u8>> {
    let mut reader = Reader::new(data);
    if reader.bytes(4)? != b"wOF2" {
        return None;
    }
    let flavor = reader.u32()?;
    if flavor == u32::from_be_bytes(*b"ttcf") {
        return None;
    }
    let _length = reader.u32()?;
    let table_count = usize::from(reader.u16()?);
    let _reserved = reader.u16()?;
    let _total_sfnt_size = reader.u32()?;
    let compressed_size = reader.u32()? as usize;
    reader.bytes(4 + 4 * 5)?; // version, metadata and private block

    let entries = read_directory(&mut reader, table_count)?;
    let compressed = reader.bytes(compressed_size)?;
    let stream_size = entries
        .iter()
        .try_fold(0usize, |sum, entry| sum.checked_add(entry.stored_length))?;
    if stream_size > MAX_DECODED_BYTES {
        return None;
    }
    let stream = decompress(compressed, stream_size)?;

    let mut stored = Vec::new();
    let mut at = 0usize;
    for entry in &entries {
        let end = at.checked_add(entry.stored_length)?;
        stored.push(stream.get(at..end)?);
        at = end;
    }
    let tables = rebuild(&entries, &stored)?;
    Some(write_font(flavor, &tables))
}

fn read_directory(reader: &mut Reader, count: usize) -> Option<Vec<TableEntry>> {
    let mut entries = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        let flags = reader.u8()?;
        let tag: [u8; 4] = match flags & 0x3f {
            63 => reader.bytes(4)?.try_into().ok()?,
            index => *KNOWN_TAGS[usize::from(index)],
        };
        let version = flags >> 6;
        let original_length = reader.base128()? as usize;
        // glyf and loca are transformed unless version 3; hmtx and every
        // other table only when version is not 0
        let transformed = if &tag == b"glyf" || &tag == b"loca" {
            version != 3
        } else {
            version != 0
        };
        let stored_length = if transformed {
            reader.base128()? as usize
        } else {
            original_length
        };
        if original_length > MAX_DECODED_BYTES {
            return None;
        }
        entries.push(TableEntry {
            tag,
            original_length,
            stored_length,
            transformed,
        });
    }
    Some(entries)
}

fn decompress(compressed: &[u8], size: usize) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(size);
    let decoder = brotli_decompressor::Decompressor::new(compressed, 4096);
    // one byte more than expected tells a longer stream apart
    decoder
        .take(size as u64 + 1)
        .read_to_end(&mut output)
        .ok()?;
    (output.len() == size).then_some(output)
}

/// turns the stored tables back into plain ones, undoing transforms.
fn rebuild(entries: &[TableEntry], stored: &[&[u8]]) -> Option<Tables> {
    let mut tables = Tables::new();
    let find = |tag: &[u8; 4]| entries.iter().position(|entry| &entry.tag == tag);
    let glyf_index = find(b"glyf");
    let mut glyph_x_min: Option<Vec<i16>> = None;
    if let Some(index) = glyf_index.filter(|index| entries[*index].transformed) {
        let rebuilt = rebuild_glyf(stored[index])?;
        glyph_x_min = Some(rebuilt.x_min);
        tables.insert(*b"glyf", rebuilt.glyf);
        tables.insert(*b"loca", rebuilt.loca);
    }
    for (entry, bytes) in entries.iter().zip(stored) {
        if tables.contains_key(&entry.tag) {
            continue;
        }
        if !entry.transformed {
            tables.insert(entry.tag, bytes.to_vec());
            continue;
        }
        match &entry.tag {
            b"hmtx" => {
                let hhea = entries
                    .iter()
                    .zip(stored)
                    .find(|(e, _)| &e.tag == b"hhea")?
                    .1;
                let maxp = entries
                    .iter()
                    .zip(stored)
                    .find(|(e, _)| &e.tag == b"maxp")?
                    .1;
                let x_min = glyph_x_min.as_deref()?;
                let hmtx = rebuild_hmtx(bytes, hhea, maxp, x_min)?;
                if hmtx.len() != entry.original_length {
                    return None;
                }
                tables.insert(*b"hmtx", hmtx);
            }
            // a transformed loca without glyf, or a transform we don't know
            _ => return None,
        }
    }
    // the rebuilt loca uses long offsets; head must say so
    if glyph_x_min.is_some() {
        let head = tables.get_mut(b"head")?;
        if head.len() < 54 {
            return None;
        }
        head[50..52].copy_from_slice(&[0, 1]);
        head[8..12].copy_from_slice(&[0; 4]);
    }
    Some(tables)
}

struct RebuiltGlyf {
    glyf: Vec<u8>,
    loca: Vec<u8>,
    /// each glyph's xMin, which a transformed hmtx needs
    x_min: Vec<i16>,
}

/// the seven streams of a transformed glyf table (spec 5.1).
struct GlyfStreams<'a> {
    contours: Reader<'a>,
    points: Reader<'a>,
    flags: Reader<'a>,
    glyphs: Reader<'a>,
    composites: Reader<'a>,
    bbox_bitmap: &'a [u8],
    bboxes: Reader<'a>,
    instructions: Reader<'a>,
    overlap_bitmap: Option<&'a [u8]>,
}

fn bit_set(bitmap: &[u8], index: usize) -> bool {
    bitmap
        .get(index / 8)
        .is_some_and(|byte| byte & (0x80 >> (index % 8)) != 0)
}

fn rebuild_glyf(table: &[u8]) -> Option<RebuiltGlyf> {
    let mut header = Reader::new(table);
    let _reserved = header.u16()?;
    let option_flags = header.u16()?;
    let glyph_count = usize::from(header.u16()?);
    let _index_format = header.u16()?;
    let mut sizes = [0usize; 7];
    for size in &mut sizes {
        *size = header.u32()? as usize;
    }
    let mut take = |size: usize| header.bytes(size);
    let contours = Reader::new(take(sizes[0])?);
    let points = Reader::new(take(sizes[1])?);
    let flags = Reader::new(take(sizes[2])?);
    let glyphs = Reader::new(take(sizes[3])?);
    let composites = Reader::new(take(sizes[4])?);
    let bbox_stream = take(sizes[5])?;
    let instructions = Reader::new(take(sizes[6])?);
    let overlap_bitmap = if option_flags & 1 != 0 {
        Some(take(glyph_count.div_ceil(8))?)
    } else {
        None
    };
    let bitmap_size = 4 * glyph_count.div_ceil(32);
    let mut streams = GlyfStreams {
        contours,
        points,
        flags,
        glyphs,
        composites,
        bbox_bitmap: bbox_stream.get(..bitmap_size)?,
        bboxes: Reader::new(bbox_stream.get(bitmap_size..)?),
        instructions,
        overlap_bitmap,
    };

    let mut glyf = Vec::new();
    let mut loca = Vec::with_capacity((glyph_count + 1) * 4);
    let mut x_min = Vec::with_capacity(glyph_count);
    for glyph in 0..glyph_count {
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        let (bytes, min) = rebuild_glyph(&mut streams, glyph)?;
        glyf.extend_from_slice(&bytes);
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
        if glyf.len() > MAX_DECODED_BYTES {
            return None;
        }
        x_min.push(min);
    }
    loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
    Some(RebuiltGlyf { glyf, loca, x_min })
}

/// one glyph in plain glyf form, with its xMin.
fn rebuild_glyph(streams: &mut GlyfStreams, glyph: usize) -> Option<(Vec<u8>, i16)> {
    let contour_count = streams.contours.i16()?;
    let explicit_bbox = bit_set(streams.bbox_bitmap, glyph);
    if contour_count == 0 {
        // an empty glyph may not claim a bounding box
        return if explicit_bbox {
            None
        } else {
            Some((Vec::new(), 0))
        };
    }
    if contour_count < 0 {
        if !explicit_bbox {
            return None;
        }
        let bbox = read_bbox(&mut streams.bboxes)?;
        return composite_glyph(streams, bbox).map(|bytes| (bytes, bbox[0]));
    }
    simple_glyph(streams, glyph, contour_count as usize, explicit_bbox)
}

fn read_bbox(bboxes: &mut Reader) -> Option<[i16; 4]> {
    Some([bboxes.i16()?, bboxes.i16()?, bboxes.i16()?, bboxes.i16()?])
}

fn simple_glyph(
    streams: &mut GlyfStreams,
    glyph: usize,
    contour_count: usize,
    explicit_bbox: bool,
) -> Option<(Vec<u8>, i16)> {
    let mut end_points = Vec::with_capacity(contour_count);
    let mut total = 0usize;
    for _ in 0..contour_count {
        total = total.checked_add(usize::from(streams.points.u255()?))?;
        if total == 0 || total > 0xffff {
            return None;
        }
        end_points.push((total - 1) as u16);
    }
    let points = decode_points(streams, total)?;
    let instruction_length = usize::from(streams.glyphs.u255()?);
    let instructions = streams.instructions.bytes(instruction_length)?;
    let bbox = if explicit_bbox {
        read_bbox(&mut streams.bboxes)?
    } else {
        bounds(&points)
    };
    let overlap = streams
        .overlap_bitmap
        .is_some_and(|bitmap| bit_set(bitmap, glyph));

    let mut out = Vec::new();
    out.extend_from_slice(&(contour_count as i16).to_be_bytes());
    for value in bbox {
        out.extend_from_slice(&value.to_be_bytes());
    }
    for end in end_points {
        out.extend_from_slice(&end.to_be_bytes());
    }
    out.extend_from_slice(&(instruction_length as u16).to_be_bytes());
    out.extend_from_slice(instructions);
    // every coordinate is written as a full 16 bit delta: larger than
    // the original, but simple and always valid
    for (index, point) in points.iter().enumerate() {
        let mut flag = u8::from(point.on_curve);
        if index == 0 && overlap {
            flag |= 0x40;
        }
        out.push(flag);
    }
    let mut previous = (0i32, 0i32);
    for point in &points {
        out.extend_from_slice(&((point.x - previous.0) as i16).to_be_bytes());
        previous.0 = point.x;
    }
    for point in &points {
        out.extend_from_slice(&((point.y - previous.1) as i16).to_be_bytes());
        previous.1 = point.y;
    }
    Some((out, bbox[0]))
}

struct Point {
    x: i32,
    y: i32,
    on_curve: bool,
}

/// the spec's triplet encoding: a flag byte says how many of the next
/// bytes hold the x and y deltas and their signs.
fn decode_points(streams: &mut GlyfStreams, count: usize) -> Option<Vec<Point>> {
    let with_sign = |flag: u8, value: i32| if flag & 1 != 0 { value } else { -value };
    let mut points = Vec::with_capacity(count);
    let (mut x, mut y) = (0i32, 0i32);
    for _ in 0..count {
        let raw = streams.flags.u8()?;
        let on_curve = raw >> 7 == 0;
        let flag = raw & 0x7f;
        let extra = match flag {
            0..=83 => 1,
            84..=119 => 2,
            120..=123 => 3,
            _ => 4,
        };
        let b = streams.glyphs.bytes(extra)?;
        let b0 = i32::from(b[0]);
        let (dx, dy) = match flag {
            0..=9 => (0, with_sign(flag, (i32::from(flag & 14) << 7) + b0)),
            10..=19 => (with_sign(flag, (i32::from((flag - 10) & 14) << 7) + b0), 0),
            20..=83 => {
                let f = i32::from(flag - 20);
                (
                    with_sign(flag, 1 + (f & 0x30) + (b0 >> 4)),
                    with_sign(flag >> 1, 1 + ((f & 0x0c) << 2) + (b0 & 0x0f)),
                )
            }
            84..=119 => {
                let f = i32::from(flag - 84);
                (
                    with_sign(flag, 1 + ((f / 12) << 8) + b0),
                    with_sign(flag >> 1, 1 + (((f % 12) >> 2) << 8) + i32::from(b[1])),
                )
            }
            120..=123 => {
                let b1 = i32::from(b[1]);
                (
                    with_sign(flag, (b0 << 4) + (b1 >> 4)),
                    with_sign(flag >> 1, ((b1 & 0x0f) << 8) + i32::from(b[2])),
                )
            }
            _ => (
                with_sign(flag, (b0 << 8) + i32::from(b[1])),
                with_sign(flag >> 1, (i32::from(b[2]) << 8) + i32::from(b[3])),
            ),
        };
        x = x.checked_add(dx)?;
        y = y.checked_add(dy)?;
        if !(-32768..=32767).contains(&x) || !(-32768..=32767).contains(&y) {
            return None;
        }
        points.push(Point { x, y, on_curve });
    }
    Some(points)
}

fn bounds(points: &[Point]) -> [i16; 4] {
    let mut bbox = [i16::MAX, i16::MAX, i16::MIN, i16::MIN];
    for point in points {
        bbox[0] = bbox[0].min(point.x as i16);
        bbox[1] = bbox[1].min(point.y as i16);
        bbox[2] = bbox[2].max(point.x as i16);
        bbox[3] = bbox[3].max(point.y as i16);
    }
    if points.is_empty() {
        [0; 4]
    } else {
        bbox
    }
}

/// a composite glyph is stored as it is, except that its instructions
/// live in the instruction stream.
fn composite_glyph(streams: &mut GlyfStreams, bbox: [i16; 4]) -> Option<Vec<u8>> {
    const ARGS_ARE_WORDS: u16 = 0x1;
    const HAS_SCALE: u16 = 0x8;
    const MORE_COMPONENTS: u16 = 0x20;
    const HAS_XY_SCALE: u16 = 0x40;
    const HAS_TWO_BY_TWO: u16 = 0x80;
    const HAS_INSTRUCTIONS: u16 = 0x100;
    let mut out = Vec::new();
    out.extend_from_slice(&(-1i16).to_be_bytes());
    for value in bbox {
        out.extend_from_slice(&value.to_be_bytes());
    }
    let mut has_instructions = false;
    loop {
        let flags = streams.composites.u16()?;
        let glyph = streams.composites.u16()?;
        let mut size = if flags & ARGS_ARE_WORDS != 0 { 4 } else { 2 };
        size += if flags & HAS_SCALE != 0 {
            2
        } else if flags & HAS_XY_SCALE != 0 {
            4
        } else if flags & HAS_TWO_BY_TWO != 0 {
            8
        } else {
            0
        };
        out.extend_from_slice(&flags.to_be_bytes());
        out.extend_from_slice(&glyph.to_be_bytes());
        out.extend_from_slice(streams.composites.bytes(size)?);
        has_instructions |= flags & HAS_INSTRUCTIONS != 0;
        if flags & MORE_COMPONENTS == 0 {
            break;
        }
    }
    if has_instructions {
        let length = usize::from(streams.glyphs.u255()?);
        out.extend_from_slice(&(length as u16).to_be_bytes());
        out.extend_from_slice(streams.instructions.bytes(length)?);
    }
    Some(out)
}

/// a transformed hmtx leaves out side bearings that equal each glyph's
/// xMin; they are put back from the rebuilt glyf.
fn rebuild_hmtx(table: &[u8], hhea: &[u8], maxp: &[u8], x_min: &[i16]) -> Option<Vec<u8>> {
    let metric_count = usize::from(read_u16(hhea, 34)?);
    let glyph_count = usize::from(read_u16(maxp, 4)?);
    if metric_count == 0 || metric_count > glyph_count || x_min.len() < glyph_count {
        return None;
    }
    let mut reader = Reader::new(table);
    let flags = reader.u8()?;
    let mut advances = Vec::with_capacity(metric_count);
    for _ in 0..metric_count {
        advances.push(reader.u16()?);
    }
    let mut out = Vec::with_capacity(metric_count * 4 + (glyph_count - metric_count) * 2);
    for (glyph, advance) in advances.iter().enumerate() {
        let lsb = if flags & 1 != 0 {
            x_min[glyph]
        } else {
            reader.i16()?
        };
        out.extend_from_slice(&advance.to_be_bytes());
        out.extend_from_slice(&lsb.to_be_bytes());
    }
    for glyph_x_min in &x_min[metric_count..glyph_count] {
        let lsb = if flags & 2 != 0 {
            *glyph_x_min
        } else {
            reader.i16()?
        };
        out.extend_from_slice(&lsb.to_be_bytes());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base128_and_255_numbers() {
        assert_eq!(Reader::new(&[0x3f]).base128(), Some(63));
        assert_eq!(Reader::new(&[0x81, 0x00]).base128(), Some(128));
        assert_eq!(Reader::new(&[0x80, 0x01]).base128(), None);
        assert_eq!(Reader::new(&[0xff, 0xff, 0xff, 0xff, 0xff]).base128(), None);
        assert_eq!(Reader::new(&[200]).u255(), Some(200));
        assert_eq!(Reader::new(&[255, 1]).u255(), Some(254));
        assert_eq!(Reader::new(&[254, 1]).u255(), Some(507));
        assert_eq!(Reader::new(&[253, 1, 0]).u255(), Some(256));
    }

    #[test]
    fn garbage_is_refused() {
        assert!(decode(b"wOF2").is_none());
        assert!(decode(&[b'w', b'O', b'F', b'2', 0, 1, 0, 0, 0, 0, 0, 0, 0xff, 0xff]).is_none());
        let mut fake = b"wOF2".to_vec();
        fake.extend_from_slice(&[0x5a; 200]);
        assert!(decode(&fake).is_none());
    }
}
