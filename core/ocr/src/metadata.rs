//! reading the few facts we need straight from image file headers:
//! the resolution a scanner recorded, and the layout of a jpeg.
//!
//! the `image` crate decodes pixels but does not report resolution, and a
//! jpeg is put into a pdf without decoding it, so these small readers look
//! at the bytes themselves. each gives up quietly (returns none) on
//! anything it does not understand, and the caller falls back to a default.

/// dots per inch across and down. scanners almost always use the same for
/// both; fax machines famously do not (204 by 98).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dpi {
    pub x: f32,
    pub y: f32,
}

/// resolutions outside this range are certainly wrong (a "1 dpi" png is
/// a common mistake of image editors), so they are ignored.
const SENSIBLE_DPI: std::ops::RangeInclusive<f32> = 30.0..=4800.0;

const CENTIMETRES_PER_INCH: f32 = 2.54;

/// the resolution recorded in a png, jpeg or tiff file, if any.
pub fn read_dpi(bytes: &[u8]) -> Option<Dpi> {
    let dpi = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png_dpi(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        jpeg_dpi(bytes)
    } else if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        tiff_dpi(bytes)
    } else {
        None
    }?;
    (SENSIBLE_DPI.contains(&dpi.x) && SENSIBLE_DPI.contains(&dpi.y)).then_some(dpi)
}

/// the png "pHYs" chunk: pixels per unit across and down, and the unit
/// (1 means metre; 0 means only the shape of a pixel is known).
fn png_dpi(bytes: &[u8]) -> Option<Dpi> {
    let mut position = 8;
    while position + 8 <= bytes.len() {
        let length = be_u32(bytes, position)? as usize;
        let kind = bytes.get(position + 4..position + 8)?;
        let data = bytes.get(position + 8..position + 8 + length)?;
        if kind == b"pHYs" && length == 9 {
            if data[8] != 1 {
                return None;
            }
            let per_metre = |offset| be_u32(data, offset).map(|value| value as f32 * 0.0254);
            return Some(Dpi {
                x: per_metre(0)?,
                y: per_metre(4)?,
            });
        }
        // the pixels come after pHYs, so there is no need to look further
        if kind == b"IDAT" {
            return None;
        }
        position += 12 + length;
    }
    None
}

/// the jfif header: a unit (1 inch, 2 centimetre) and two densities.
fn jpeg_dpi(bytes: &[u8]) -> Option<Dpi> {
    let segment = jpeg_segments(bytes).find(|segment| {
        segment.marker == 0xE0 && segment.data.starts_with(b"JFIF\0") && segment.data.len() >= 12
    })?;
    let data = segment.data;
    let x = be_u16(data, 8)? as f32;
    let y = be_u16(data, 10)? as f32;
    match data[7] {
        1 => Some(Dpi { x, y }),
        2 => Some(Dpi {
            x: x * CENTIMETRES_PER_INCH,
            y: y * CENTIMETRES_PER_INCH,
        }),
        _ => None,
    }
}

/// a tiff's first page: tags 282 and 283 (resolution) and 296 (its unit).
fn tiff_dpi(bytes: &[u8]) -> Option<Dpi> {
    let tiff = Tiff::new(bytes)?;
    let first_page = tiff.u32_at(4)? as usize;
    let x = tiff.rational_tag(first_page, 282)?;
    let y = tiff.rational_tag(first_page, 283).unwrap_or(x);
    // the default unit is the inch
    let unit = tiff.short_tag(first_page, 296).unwrap_or(2);
    match unit {
        2 => Some(Dpi { x, y }),
        3 => Some(Dpi {
            x: x * CENTIMETRES_PER_INCH,
            y: y * CENTIMETRES_PER_INCH,
        }),
        _ => None,
    }
}

/// what a pdf needs to know to show a jpeg without decoding it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JpegLayout {
    pub width: u32,
    pub height: u32,
    /// 1 gray, 3 colour, 4 cmyk
    pub components: u8,
    /// adobe software writes cmyk jpegs with every value flipped, and says
    /// so with its own header; the pdf then has to flip them back
    pub adobe_inverted_cmyk: bool,
    /// how the camera says the picture should be turned (exif, 1 to 8)
    pub exif_orientation: u8,
}

/// reads a jpeg's size, colour layout and camera orientation.
pub fn jpeg_layout(bytes: &[u8]) -> Option<JpegLayout> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut frame = None;
    let mut adobe = false;
    let mut orientation = 1;
    for segment in jpeg_segments(bytes) {
        match segment.marker {
            // every start-of-frame kind except the three markers in that
            // range that mean something else (huffman, arithmetic, jpeg-ls tables)
            0xC0..=0xCF if !matches!(segment.marker, 0xC4 | 0xC8 | 0xCC) => {
                let data = segment.data;
                frame = Some((
                    u32::from(be_u16(data, 3)?),
                    u32::from(be_u16(data, 1)?),
                    *data.get(5)?,
                ));
            }
            0xEE if segment.data.starts_with(b"Adobe") => adobe = true,
            0xE1 if segment.data.starts_with(b"Exif\0\0") => {
                orientation = exif_orientation(&segment.data[6..]).unwrap_or(1);
            }
            _ => {}
        }
    }
    let (width, height, components) = frame?;
    Some(JpegLayout {
        width,
        height,
        components,
        adobe_inverted_cmyk: adobe && components == 4,
        exif_orientation: orientation,
    })
}

/// tag 274 of the first exif directory, which is laid out like a tiff.
fn exif_orientation(tiff_bytes: &[u8]) -> Option<u8> {
    let tiff = Tiff::new(tiff_bytes)?;
    let first = tiff.u32_at(4)? as usize;
    let value = tiff.short_tag(first, 274)?;
    (1..=8).contains(&value).then_some(value as u8)
}

struct JpegSegment<'a> {
    marker: u8,
    data: &'a [u8],
}

/// walks the header segments of a jpeg up to the start of the pixel data.
fn jpeg_segments(bytes: &[u8]) -> impl Iterator<Item = JpegSegment<'_>> {
    let mut position = 2;
    std::iter::from_fn(move || loop {
        if *bytes.get(position)? != 0xFF {
            return None;
        }
        let marker = *bytes.get(position + 1)?;
        // padding bytes and markers without a length
        if marker == 0xFF {
            position += 1;
            continue;
        }
        if marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
            position += 2;
            continue;
        }
        // start of scan: the compressed pixels follow, the header is over
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let length = be_u16(bytes, position + 2)? as usize;
        let data = bytes.get(position + 4..position + 2 + length)?;
        position += 2 + length;
        return Some(JpegSegment { marker, data });
    })
}

/// the little part of a tiff file needed to read single tags.
struct Tiff<'a> {
    bytes: &'a [u8],
    little_endian: bool,
}

impl<'a> Tiff<'a> {
    fn new(bytes: &'a [u8]) -> Option<Self> {
        let little_endian = match bytes.get(0..4)? {
            b"II*\0" => true,
            b"MM\0*" => false,
            _ => return None,
        };
        Some(Self {
            bytes,
            little_endian,
        })
    }

    fn u16_at(&self, offset: usize) -> Option<u16> {
        let pair: [u8; 2] = self.bytes.get(offset..offset + 2)?.try_into().ok()?;
        Some(if self.little_endian {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        })
    }

    fn u32_at(&self, offset: usize) -> Option<u32> {
        let four: [u8; 4] = self.bytes.get(offset..offset + 4)?.try_into().ok()?;
        Some(if self.little_endian {
            u32::from_le_bytes(four)
        } else {
            u32::from_be_bytes(four)
        })
    }

    /// where a tag's 12 byte entry starts in the directory at `directory`.
    fn find_tag(&self, directory: usize, tag: u16) -> Option<usize> {
        let count = self.u16_at(directory)? as usize;
        (0..count)
            .map(|index| directory + 2 + index * 12)
            .find(|&entry| self.u16_at(entry) == Some(tag))
    }

    fn short_tag(&self, directory: usize, tag: u16) -> Option<u16> {
        let entry = self.find_tag(directory, tag)?;
        self.u16_at(entry + 8)
    }

    /// a fraction stored elsewhere in the file, pointed to by the entry.
    fn rational_tag(&self, directory: usize, tag: u16) -> Option<f32> {
        let entry = self.find_tag(directory, tag)?;
        let offset = self.u32_at(entry + 8)? as usize;
        let numerator = self.u32_at(offset)? as f32;
        let denominator = self.u32_at(offset + 4)? as f32;
        (denominator > 0.0).then(|| numerator / denominator)
    }
}

fn be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_with_phys(per_metre: u32, unit: u8) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        // a stand-in header chunk; only its length matters here
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&[0; 13 + 4]);
        bytes.extend_from_slice(&9u32.to_be_bytes());
        bytes.extend_from_slice(b"pHYs");
        bytes.extend_from_slice(&per_metre.to_be_bytes());
        bytes.extend_from_slice(&per_metre.to_be_bytes());
        bytes.push(unit);
        bytes.extend_from_slice(&[0; 4]);
        bytes
    }

    #[test]
    fn reads_png_resolution() {
        // 11811 pixels per metre is 300 dots per inch
        let dpi = read_dpi(&png_with_phys(11811, 1)).unwrap();
        assert!((dpi.x - 300.0).abs() < 0.1);
    }

    #[test]
    fn a_png_without_a_unit_has_no_resolution() {
        assert_eq!(read_dpi(&png_with_phys(1, 0)), None);
    }

    fn jfif(unit: u8, density: u16) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        bytes.extend_from_slice(b"JFIF\0");
        bytes.extend_from_slice(&[1, 2, unit]);
        bytes.extend_from_slice(&density.to_be_bytes());
        bytes.extend_from_slice(&density.to_be_bytes());
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&[0xFF, 0xDA]);
        bytes
    }

    #[test]
    fn reads_jpeg_resolution_in_inches_and_centimetres() {
        assert_eq!(read_dpi(&jfif(1, 200)), Some(Dpi { x: 200.0, y: 200.0 }));
        let metric = read_dpi(&jfif(2, 118)).unwrap();
        assert!((metric.x - 299.7).abs() < 0.1);
        assert_eq!(read_dpi(&jfif(0, 1)), None);
    }

    #[test]
    fn nonsense_resolutions_are_ignored() {
        assert_eq!(read_dpi(&jfif(1, 1)), None);
    }

    #[test]
    fn reads_tiff_resolution() {
        // little endian tiff: header, then a directory of two entries at 8,
        // then the fraction 600/2 at 40
        let mut bytes = b"II*\0".to_vec();
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        for (tag, kind, value) in [(282u16, 5u16, 40u32), (296, 3, 2)] {
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&kind.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&[0; 4]);
        bytes.resize(40, 0);
        bytes.extend_from_slice(&600u32.to_le_bytes());
        bytes.extend_from_slice(&2u32.to_le_bytes());
        assert_eq!(read_dpi(&bytes), Some(Dpi { x: 300.0, y: 300.0 }));
    }

    #[test]
    fn reads_the_jpeg_frame_header() {
        let mut bytes = vec![0xFF, 0xD8];
        // adobe header, then a baseline frame: 8 bit, 20 high, 30 wide, 4 parts
        bytes.extend_from_slice(&[0xFF, 0xEE, 0x00, 0x07]);
        bytes.extend_from_slice(b"Adobe");
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x08, 8, 0, 20, 0, 30, 4]);
        bytes.extend_from_slice(&[0xFF, 0xDA]);
        let layout = jpeg_layout(&bytes).unwrap();
        assert_eq!(
            (layout.width, layout.height, layout.components),
            (30, 20, 4)
        );
        assert!(layout.adobe_inverted_cmyk);
        assert_eq!(layout.exif_orientation, 1);
    }
}
