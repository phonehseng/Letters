//! pictures in a pdf, made ready for the docx (recipe 10, "pictures").
//!
//! a jpeg with no mask in DeviceRGB or DeviceGray keeps its original bytes,
//! including the ascii85-wrapped ones reportlab writes, so the picture is
//! not re-encoded. everything else is decoded to pixels and written as png,
//! with its soft mask as alpha.

use std::io::Cursor;

use image::{DynamicImage, GrayImage, ImageFormat, RgbaImage};
use lopdf::{Document, Object, Stream};

use super::lexer::Operand;
use super::objects::{filter_names, get, get_number, numbers, resolve, stream_bytes};

/// a picture ready to be stored in the docx.
#[derive(Clone, Debug, PartialEq)]
pub struct PictureData {
    pub bytes: Vec<u8>,
    /// "jpeg" or "png"
    pub extension: &'static str,
    pub width: u32,
    pub height: u32,
}

/// how the samples of an image are coloured.
#[derive(Clone, Debug)]
enum Colors {
    Gray,
    Rgb,
    Cmyk,
    /// a palette of base-space colours, already as rgb
    Indexed(Vec<[u8; 3]>),
    /// one tint channel (separation), drawn darker as it grows
    Tint,
}

/// a decoded image description, independent of where it came from.
struct ImageSpec {
    width: u32,
    height: u32,
    bits: u32,
    colors: Colors,
    stencil: bool,
}

/// reads an image xobject.
pub fn decode_image(document: &Document, stream: &Stream) -> Result<PictureData, String> {
    let dict = &stream.dict;
    let width = get_number(document, dict, b"Width").unwrap_or(0.0) as u32;
    let height = get_number(document, dict, b"Height").unwrap_or(0.0) as u32;
    if width == 0 || height == 0 {
        return Err("it has no size".into());
    }
    let has_mask = dict.has(b"SMask") || dict.has(b"Mask");
    let filters = filter_names(document, stream);
    let space_name = match get(document, dict, b"ColorSpace") {
        Some(Object::Name(name)) => Some(name.clone()),
        _ => None,
    };
    if let Some(jpeg) = original_jpeg(&filters, stream)? {
        let plain_space = matches!(
            space_name.as_deref(),
            Some(b"DeviceRGB") | Some(b"DeviceGray")
        );
        if !has_mask && plain_space {
            return Ok(PictureData {
                bytes: jpeg,
                extension: "jpeg",
                width,
                height,
            });
        }
        let decoded = image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg)
            .map_err(|e| format!("its jpeg data can't be read ({e})"))?;
        let mut rgba = decoded.to_rgba8();
        apply_soft_mask(document, stream, &mut rgba);
        return png_picture(rgba);
    }
    if filters
        .iter()
        .any(|f| f == b"JPXDecode" || f == b"JBIG2Decode" || f == b"CCITTFaxDecode")
    {
        return Err("its compression (jpeg 2000, jbig2 or fax) isn't supported".into());
    }
    let samples = stream_bytes(stream)?;
    let stencil = matches!(
        get(document, dict, b"ImageMask"),
        Some(Object::Boolean(true))
    );
    let spec = ImageSpec {
        width,
        height,
        bits: if stencil {
            1
        } else {
            get_number(document, dict, b"BitsPerComponent").unwrap_or(8.0) as u32
        },
        colors: colors_of(document, get(document, dict, b"ColorSpace")),
        stencil,
    };
    let mut rgba = to_rgba(&spec, &samples)?;
    apply_soft_mask(document, stream, &mut rgba);
    png_picture(rgba)
}

/// reads an inline image (`BI ... ID ... EI`).
pub fn inline_image(dictionary: &Operand, data: &[u8]) -> Result<PictureData, String> {
    let entry = |long: &[u8], short: &[u8]| dictionary.get(long).or_else(|| dictionary.get(short));
    let width = entry(b"Width", b"W")
        .and_then(Operand::number)
        .unwrap_or(0.0) as u32;
    let height = entry(b"Height", b"H")
        .and_then(Operand::number)
        .unwrap_or(0.0) as u32;
    if width == 0 || height == 0 {
        return Err("it has no size".into());
    }
    let filters: Vec<Vec<u8>> = match entry(b"Filter", b"F") {
        Some(Operand::Name(name)) => vec![name.clone()],
        Some(Operand::Array(items)) => items
            .iter()
            .filter_map(|i| i.name().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    };
    let mut bytes = data.to_vec();
    for filter in &filters {
        bytes = match filter.as_slice() {
            b"FlateDecode" | b"Fl" => inflate(&bytes)?,
            b"ASCII85Decode" | b"A85" => ascii85(&bytes)?,
            b"DCTDecode" | b"DCT" => {
                let decoded = image::load_from_memory_with_format(&bytes, ImageFormat::Jpeg)
                    .map_err(|e| format!("its jpeg data can't be read ({e})"))?;
                return png_picture(decoded.to_rgba8());
            }
            other => {
                return Err(format!(
                    "its {} compression isn't supported",
                    String::from_utf8_lossy(other)
                ))
            }
        };
    }
    let stencil = matches!(entry(b"ImageMask", b"IM"), Some(Operand::Boolean(true)));
    let colors = match entry(b"ColorSpace", b"CS").and_then(Operand::name) {
        Some(b"DeviceRGB") | Some(b"RGB") => Colors::Rgb,
        Some(b"DeviceCMYK") | Some(b"CMYK") => Colors::Cmyk,
        _ => Colors::Gray,
    };
    let spec = ImageSpec {
        width,
        height,
        bits: if stencil {
            1
        } else {
            entry(b"BitsPerComponent", b"BPC")
                .and_then(Operand::number)
                .unwrap_or(8.0) as u32
        },
        colors,
        stencil,
    };
    png_picture(to_rgba(&spec, &bytes)?)
}

/// the jpeg file inside an image stream: plain DCTDecode, or wrapped in
/// ascii85 the way reportlab writes it. `None` when it is not a jpeg.
fn original_jpeg(filters: &[Vec<u8>], stream: &Stream) -> Result<Option<Vec<u8>>, String> {
    match filters {
        [only] if only == b"DCTDecode" => Ok(Some(stream.content.clone())),
        [first, second] if first == b"ASCII85Decode" && second == b"DCTDecode" => {
            ascii85(&stream.content).map(Some)
        }
        [first, second] if first == b"FlateDecode" && second == b"DCTDecode" => {
            inflate(&stream.content).map(Some)
        }
        _ => Ok(None),
    }
}

fn inflate(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut stream = Stream::new(lopdf::Dictionary::new(), bytes.to_vec());
    stream
        .dict
        .set("Filter", Object::Name(b"FlateDecode".to_vec()));
    stream.decompressed_content().map_err(|e| e.to_string())
}

fn ascii85(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut stream = Stream::new(lopdf::Dictionary::new(), bytes.to_vec());
    stream
        .dict
        .set("Filter", Object::Name(b"ASCII85Decode".to_vec()));
    stream.decompressed_content().map_err(|e| e.to_string())
}

fn colors_of(document: &Document, space: Option<&Object>) -> Colors {
    match space {
        Some(Object::Name(name)) => match name.as_slice() {
            b"DeviceRGB" | b"CalRGB" => Colors::Rgb,
            b"DeviceCMYK" => Colors::Cmyk,
            _ => Colors::Gray,
        },
        Some(Object::Array(items)) => {
            let family = items.first().map(|i| resolve(document, i));
            match family {
                Some(Object::Name(n)) if n == b"ICCBased" => {
                    let components = items
                        .get(1)
                        .and_then(|s| match resolve(document, s) {
                            Object::Stream(s) => get_number(document, &s.dict, b"N"),
                            _ => None,
                        })
                        .unwrap_or(3.0) as i64;
                    match components {
                        1 => Colors::Gray,
                        4 => Colors::Cmyk,
                        _ => Colors::Rgb,
                    }
                }
                Some(Object::Name(n)) if n == b"CalRGB" || n == b"Lab" => Colors::Rgb,
                Some(Object::Name(n)) if n == b"CalGray" => Colors::Gray,
                Some(Object::Name(n)) if n == b"Indexed" || n == b"I" => {
                    indexed_palette(document, items)
                }
                Some(Object::Name(n)) if n == b"Separation" || n == b"DeviceN" => Colors::Tint,
                _ => Colors::Rgb,
            }
        }
        _ => Colors::Gray,
    }
}

fn indexed_palette(document: &Document, items: &[Object]) -> Colors {
    let base = colors_of(document, items.get(1).map(|i| resolve(document, i)));
    let lookup: Vec<u8> = match items.get(3).map(|i| resolve(document, i)) {
        Some(Object::String(bytes, _)) => bytes.clone(),
        Some(Object::Stream(stream)) => stream_bytes(stream).unwrap_or_default(),
        _ => Vec::new(),
    };
    let components = match base {
        Colors::Rgb => 3,
        Colors::Cmyk => 4,
        _ => 1,
    };
    let palette = lookup
        .chunks(components)
        .map(|entry| match entry {
            [r, g, b] => [*r, *g, *b],
            [c, m, y, k] => cmyk_to_rgb(*c, *m, *y, *k),
            [gray] => [*gray; 3],
            _ => [0; 3],
        })
        .collect();
    Colors::Indexed(palette)
}

fn cmyk_to_rgb(c: u8, m: u8, y: u8, k: u8) -> [u8; 3] {
    let mix = |v: u8| ((255 - u32::from(v)) * (255 - u32::from(k)) / 255) as u8;
    [mix(c), mix(m), mix(y)]
}

/// reads `count` samples of `bits` each from a row.
fn row_samples(row: &[u8], bits: u32, count: usize) -> Vec<u32> {
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let value = match bits {
            8 => u32::from(row.get(index).copied().unwrap_or(0)),
            16 => u32::from(row.get(2 * index).copied().unwrap_or(0)),
            1 | 2 | 4 => {
                let bit = index * bits as usize;
                let byte = row.get(bit / 8).copied().unwrap_or(0);
                let shift = 8 - bits as usize - bit % 8;
                u32::from(byte >> shift) & ((1 << bits) - 1)
            }
            _ => 0,
        };
        samples.push(value);
    }
    samples
}

fn to_rgba(spec: &ImageSpec, samples: &[u8]) -> Result<RgbaImage, String> {
    let components = match spec.colors {
        Colors::Rgb => 3,
        Colors::Cmyk => 4,
        _ => 1,
    };
    let row_bytes = (spec.width as usize * components * spec.bits as usize).div_ceil(8);
    if samples.len() < row_bytes * spec.height as usize {
        return Err("its data is shorter than its size says".into());
    }
    let scale = |value: u32| -> u8 {
        match spec.bits {
            8 | 16 => value as u8,
            bits => (value * 255 / ((1 << bits) - 1)) as u8,
        }
    };
    let mut image = RgbaImage::new(spec.width, spec.height);
    for y in 0..spec.height {
        let row = &samples[y as usize * row_bytes..(y as usize + 1) * row_bytes];
        let values = row_samples(row, spec.bits, spec.width as usize * components);
        for x in 0..spec.width {
            let pixel = &values[x as usize * components..(x as usize + 1) * components];
            let rgba = if spec.stencil {
                // sample 0 paints (decode [0 1]); the paint colour is black here
                if pixel[0] == 0 {
                    [0, 0, 0, 255]
                } else {
                    [0, 0, 0, 0]
                }
            } else {
                let [r, g, b] = match &spec.colors {
                    Colors::Gray => [scale(pixel[0]); 3],
                    Colors::Rgb => [scale(pixel[0]), scale(pixel[1]), scale(pixel[2])],
                    Colors::Cmyk => cmyk_to_rgb(
                        scale(pixel[0]),
                        scale(pixel[1]),
                        scale(pixel[2]),
                        scale(pixel[3]),
                    ),
                    Colors::Indexed(palette) => {
                        palette.get(pixel[0] as usize).copied().unwrap_or([0; 3])
                    }
                    Colors::Tint => [255 - scale(pixel[0]); 3],
                };
                [r, g, b, 255]
            };
            image.put_pixel(x, y, image::Rgba(rgba));
        }
    }
    Ok(image)
}

/// puts the image's soft mask (or stencil mask) into its alpha channel.
fn apply_soft_mask(document: &Document, stream: &Stream, rgba: &mut RgbaImage) {
    let (mask_stream, invert) = match get(document, &stream.dict, b"SMask") {
        Some(Object::Stream(mask)) => (mask, false),
        _ => match get(document, &stream.dict, b"Mask") {
            // an explicit mask: samples of 1 are masked out
            Some(Object::Stream(mask)) => (mask, true),
            _ => return,
        },
    };
    let Ok(mask) = decode_gray(document, mask_stream) else {
        return;
    };
    let mask = if mask.dimensions() != rgba.dimensions() {
        image::imageops::resize(
            &mask,
            rgba.width(),
            rgba.height(),
            image::imageops::FilterType::Nearest,
        )
    } else {
        mask
    };
    for (pixel, alpha) in rgba.pixels_mut().zip(mask.pixels()) {
        pixel.0[3] = if invert { 255 - alpha.0[0] } else { alpha.0[0] };
    }
}

fn decode_gray(document: &Document, stream: &Stream) -> Result<GrayImage, String> {
    let filters = filter_names(document, stream);
    if let Some(jpeg) = original_jpeg(&filters, stream)? {
        return image::load_from_memory_with_format(&jpeg, ImageFormat::Jpeg)
            .map(|d| d.to_luma8())
            .map_err(|e| e.to_string());
    }
    let dict = &stream.dict;
    let width = get_number(document, dict, b"Width").unwrap_or(0.0) as u32;
    let height = get_number(document, dict, b"Height").unwrap_or(0.0) as u32;
    let stencil = matches!(
        get(document, dict, b"ImageMask"),
        Some(Object::Boolean(true))
    );
    let bits = if stencil {
        1
    } else {
        get_number(document, dict, b"BitsPerComponent").unwrap_or(8.0) as u32
    };
    let samples = stream_bytes(stream)?;
    let row_bytes = (width as usize * bits as usize).div_ceil(8);
    if width == 0 || samples.len() < row_bytes * height as usize {
        return Err("mask data too short".into());
    }
    let decode_inverted = matches!(get(document, dict, b"Decode"), Some(Object::Array(d)) if numbers(document, d).first() == Some(&1.0));
    let mut gray = GrayImage::new(width, height);
    for y in 0..height {
        let row = &samples[y as usize * row_bytes..(y as usize + 1) * row_bytes];
        for (x, value) in row_samples(row, bits, width as usize)
            .into_iter()
            .enumerate()
        {
            let mut level = match bits {
                8 | 16 => value as u8,
                b => (value * 255 / ((1 << b) - 1)) as u8,
            };
            if decode_inverted {
                level = 255 - level;
            }
            gray.put_pixel(x as u32, y, image::Luma([level]));
        }
    }
    Ok(gray)
}

fn png_picture(rgba: RgbaImage) -> Result<PictureData, String> {
    let (width, height) = rgba.dimensions();
    let opaque = rgba.pixels().all(|p| p.0[3] == 255);
    let image = if opaque {
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(rgba).to_rgb8())
    } else {
        DynamicImage::ImageRgba8(rgba)
    };
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|e| format!("it couldn’t be written as png ({e})"))?;
    Ok(PictureData {
        bytes,
        extension: "png",
        width,
        height,
    })
}

/// encodes an rgba image as png (for rasterised vectors).
pub fn encode_png(rgba: RgbaImage) -> Result<Vec<u8>, String> {
    png_picture(rgba).map(|picture| picture.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_of_every_depth() {
        assert_eq!(row_samples(&[0b1010_0000], 1, 4), [1, 0, 1, 0]);
        assert_eq!(row_samples(&[0x1F], 4, 2), [1, 15]);
        assert_eq!(row_samples(&[7, 9], 8, 2), [7, 9]);
    }

    #[test]
    fn inline_gray_image_becomes_png() {
        let dictionary = Operand::Dictionary(vec![
            (b"W".to_vec(), Operand::Number(2.0)),
            (b"H".to_vec(), Operand::Number(1.0)),
            (b"BPC".to_vec(), Operand::Number(8.0)),
            (b"CS".to_vec(), Operand::Name(b"G".to_vec())),
        ]);
        let picture = inline_image(&dictionary, &[0, 255]).unwrap();
        assert_eq!(picture.extension, "png");
        assert_eq!((picture.width, picture.height), (2, 1));
        assert!(picture.bytes.starts_with(b"\x89PNG"));
    }
}
