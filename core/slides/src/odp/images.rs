//! picture sizes, for turning opendocument crops into drawingml's.
//! odf crops a picture by lengths of its natural size (`fo:clip`);
//! drawingml crops by shares of it, so the natural size is read from the
//! picture: its pixels at its stated resolution, 96 dpi when it states
//! none (what libreoffice assumes).

use super::read_shapes::ShapeContext;
use super::units::length;
use crate::model::{Emu, EMU_PER_INCH};

const DEFAULT_DPI: f64 = 96.0;

/// width and height in emu of a png or jpeg.
pub fn natural_size(bytes: &[u8]) -> Option<(Emu, Emu)> {
    if bytes.starts_with(b"\x89PNG") {
        return png_size(bytes);
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        return jpeg_size(bytes);
    }
    None
}

fn to_emu(pixels: u32, dpi: f64) -> Emu {
    (f64::from(pixels) / dpi * EMU_PER_INCH as f64).round() as Emu
}

fn png_size(bytes: &[u8]) -> Option<(Emu, Emu)> {
    let width = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?);
    let height = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?);
    let (mut dpi_x, mut dpi_y) = (DEFAULT_DPI, DEFAULT_DPI);
    // pHYs gives pixels per metre when its unit byte is 1
    if let Some(at) = bytes.windows(4).position(|w| w == b"pHYs") {
        let read = |offset: usize| {
            bytes
                .get(at + offset..at + offset + 4)
                .and_then(|b| b.try_into().ok())
                .map(u32::from_be_bytes)
        };
        if let (Some(x), Some(y), Some(1)) = (read(4), read(8), bytes.get(at + 12)) {
            dpi_x = f64::from(x) * 0.0254;
            dpi_y = f64::from(y) * 0.0254;
        }
    }
    Some((
        to_emu(width, dpi_x.max(1.0)),
        to_emu(height, dpi_y.max(1.0)),
    ))
}

fn jpeg_size(bytes: &[u8]) -> Option<(Emu, Emu)> {
    let mut index = 2;
    let mut dpi = DEFAULT_DPI;
    while index + 9 < bytes.len() {
        if bytes[index] != 0xFF {
            return None;
        }
        let marker = bytes[index + 1];
        let length = usize::from(u16::from_be_bytes([bytes[index + 2], bytes[index + 3]]));
        // jfif with density in dots per inch
        if marker == 0xE0
            && bytes.get(index + 4..index + 9) == Some(b"JFIF\0")
            && bytes.get(index + 11) == Some(&1)
        {
            let density = u16::from_be_bytes([*bytes.get(index + 12)?, *bytes.get(index + 13)?]);
            if density > 0 {
                dpi = f64::from(density);
            }
        }
        if (0xC0..=0xC3).contains(&marker) {
            let height = u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]);
            let width = u16::from_be_bytes([*bytes.get(index + 7)?, *bytes.get(index + 8)?]);
            return Some((
                to_emu(u32::from(width), dpi),
                to_emu(u32::from(height), dpi),
            ));
        }
        index += 2 + length;
    }
    None
}

/// `fo:clip="rect(top, right, bottom, left)"` as drawingml's crop
/// fractions (left, top, right, bottom in thousandths of a percent).
pub fn crop_fractions(clip: &str, href: &str, context: &ShapeContext<'_>) -> Option<[i32; 4]> {
    let inner = clip.trim().strip_prefix("rect(")?.strip_suffix(')')?;
    let sides: Vec<Emu> = inner
        .split([',', ' '])
        .filter(|s| !s.is_empty())
        .map(|s| length(s).unwrap_or(0))
        .collect();
    let [top, right, bottom, left] = <[Emu; 4]>::try_from(sides).ok()?;
    let (width, height) = *context.sizes.get(href)?;
    if width <= 0 || height <= 0 {
        return None;
    }
    let share = |part: Emu, whole: Emu| (part as f64 / whole as f64 * 100_000.0).round() as i32;
    let crop = [
        share(left, width),
        share(top, height),
        share(right, width),
        share(bottom, height),
    ];
    (crop != [0; 4]).then_some(crop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_sizes_use_96_dpi_without_a_resolution() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&400u32.to_be_bytes());
        png.extend_from_slice(&300u32.to_be_bytes());
        assert_eq!(natural_size(&png), Some((3_810_000, 2_857_500)));
    }
}
