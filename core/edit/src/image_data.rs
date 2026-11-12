//! turning png and jpeg files into pdf image objects, and back.
//!
//! jpeg files are stored exactly as they are (pdf readers decode them
//! with /DCTDecode), so a saved-out jpeg is the very file that went in.
//! png files are decoded and stored compressed, with transparency kept
//! as a soft mask. the reading half is adapted from letters-annotate.

use std::io::Cursor;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::draft::Draft;
use crate::error::EditError;
use crate::pdf_values::{dict_number, name, number_array, resolve};

/// the largest picture accepted, in pixels: 50 megapixels is far
/// sharper than any page needs, and larger ones could use gigabytes.
const MAX_PIXELS: u64 = 50_000_000;

/// a decoded picture's size, for placing it.
pub(crate) struct AddedImage {
    pub object: ObjectId,
    pub width: u32,
    pub height: u32,
}

/// adds png or jpeg bytes to the file as an image object.
pub(crate) fn add_image_object(draft: &mut Draft, bytes: &[u8]) -> Result<AddedImage, EditError> {
    let (mut image, mask, width, height) = if bytes.starts_with(&[0xff, 0xd8]) {
        jpeg_object(bytes)?
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png_object(bytes)?
    } else {
        return Err(EditError::UnsupportedImage(
            "Use a PNG or JPEG picture.".into(),
        ));
    };
    if let Some(mask) = mask {
        let mask_id = draft.add(Object::Stream(mask));
        image.dict.set("SMask", Object::Reference(mask_id));
    }
    Ok(AddedImage {
        object: draft.add(Object::Stream(image)),
        width,
        height,
    })
}

fn too_large(width: u32, height: u32) -> bool {
    u64::from(width) * u64::from(height) > MAX_PIXELS || width == 0 || height == 0
}

fn too_large_error() -> EditError {
    EditError::UnsupportedImage("It is too large. Use a smaller picture.".into())
}

fn image_dictionary(width: u32, height: u32, color_space: &str) -> Dictionary {
    let mut image = Dictionary::new();
    image.set("Type", name("XObject"));
    image.set("Subtype", name("Image"));
    image.set("Width", Object::Integer(i64::from(width)));
    image.set("Height", Object::Integer(i64::from(height)));
    image.set("ColorSpace", name(color_space));
    image.set("BitsPerComponent", Object::Integer(8));
    image
}

type ImageParts = (Stream, Option<Stream>, u32, u32);

fn jpeg_object(bytes: &[u8]) -> Result<ImageParts, EditError> {
    let header = read_jpeg_header(bytes)
        .ok_or_else(|| EditError::UnsupportedImage("The JPEG file is damaged.".into()))?;
    if too_large(header.width, header.height) {
        return Err(too_large_error());
    }
    let color_space = match header.components {
        1 => "DeviceGray",
        3 => "DeviceRGB",
        4 => "DeviceCMYK",
        _ => {
            return Err(EditError::UnsupportedImage(
                "Its colours aren’t supported.".into(),
            ))
        }
    };
    let mut dict = image_dictionary(header.width, header.height, color_space);
    dict.set("Filter", name("DCTDecode"));
    // photoshop writes cmyk jpegs with inverted values, marked "Adobe"
    if header.components == 4 && header.adobe {
        dict.set(
            "Decode",
            number_array(&[1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]),
        );
    }
    Ok((
        Stream::new(dict, bytes.to_vec()).with_compression(false),
        None,
        header.width,
        header.height,
    ))
}

struct JpegHeader {
    width: u32,
    height: u32,
    components: u8,
    adobe: bool,
}

/// walks a jpeg's segments up to the frame header that holds its size.
fn read_jpeg_header(bytes: &[u8]) -> Option<JpegHeader> {
    let mut position = 2;
    let mut adobe = false;
    loop {
        while *bytes.get(position)? == 0xff && *bytes.get(position + 1)? == 0xff {
            position += 1;
        }
        if *bytes.get(position)? != 0xff {
            return None;
        }
        let marker = *bytes.get(position + 1)?;
        let length = usize::from(u16::from_be_bytes([
            *bytes.get(position + 2)?,
            *bytes.get(position + 3)?,
        ]));
        let segment = bytes.get(position + 4..position + 2 + length)?;
        match marker {
            0xee if segment.starts_with(b"Adobe") => adobe = true,
            // every frame header except DHT (c4), JPG (c8) and DAC (cc)
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                return Some(JpegHeader {
                    height: u32::from(u16::from_be_bytes([*segment.get(1)?, *segment.get(2)?])),
                    width: u32::from(u16::from_be_bytes([*segment.get(3)?, *segment.get(4)?])),
                    components: *segment.get(5)?,
                    adobe,
                });
            }
            0xda | 0xd9 => return None,
            _ => {}
        }
        position += 2 + length;
    }
}

fn png_object(bytes: &[u8]) -> Result<ImageParts, EditError> {
    let damaged = |_| EditError::UnsupportedImage("The PNG file is damaged.".into());
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let header = decoder.read_header_info().map_err(damaged)?;
    if too_large(header.width, header.height) {
        return Err(too_large_error());
    }
    let mut reader = decoder.read_info().map_err(damaged)?;
    let size = reader.output_buffer_size().ok_or_else(too_large_error)?;
    let mut pixels = vec![0; size];
    let frame = reader.next_frame(&mut pixels).map_err(damaged)?;
    pixels.truncate(frame.buffer_size());
    let (channels, color_space, has_alpha) = match frame.color_type {
        png::ColorType::Grayscale => (1, "DeviceGray", false),
        png::ColorType::GrayscaleAlpha => (2, "DeviceGray", true),
        png::ColorType::Rgb => (3, "DeviceRGB", false),
        png::ColorType::Rgba => (4, "DeviceRGB", true),
        png::ColorType::Indexed => {
            return Err(EditError::UnsupportedImage(
                "Its colours aren’t supported.".into(),
            ))
        }
    };
    let mut color = Vec::with_capacity(pixels.len());
    let mut alpha = Vec::new();
    for row in pixels.chunks_exact(frame.line_size) {
        for pixel in row[..frame.width as usize * channels].chunks_exact(channels) {
            if has_alpha {
                color.extend_from_slice(&pixel[..channels - 1]);
                alpha.push(pixel[channels - 1]);
            } else {
                color.extend_from_slice(pixel);
            }
        }
    }
    let image = compressed(
        image_dictionary(frame.width, frame.height, color_space),
        color,
    );
    let mask = (has_alpha && alpha.iter().any(|&value| value < 255)).then(|| {
        compressed(
            image_dictionary(frame.width, frame.height, "DeviceGray"),
            alpha,
        )
    });
    Ok((image, mask, frame.width, frame.height))
}

fn compressed(dict: Dictionary, data: Vec<u8>) -> Stream {
    let mut stream = Stream::new(dict, data);
    let _ = stream.compress();
    stream
}

/// a picture taken out of the file: png, or the original jpeg bytes.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedImage {
    /// "png" or "jpeg", for the file name
    pub format: String,
    pub bytes: Vec<u8>,
}

fn cannot(reason: &str) -> EditError {
    EditError::CannotExtract(reason.into())
}

/// the image object's picture as a file.
pub(crate) fn extract(doc: &Document, object: ObjectId) -> Result<ExtractedImage, EditError> {
    let stream = doc
        .get_object(object)
        .and_then(Object::as_stream)
        .map_err(|_| EditError::UnknownObject)?;
    let filters = filters(doc, &stream.dict);
    if filters == [b"DCTDecode".to_vec()] {
        return Ok(ExtractedImage {
            format: "jpeg".into(),
            bytes: stream.content.clone(),
        });
    }
    if filters.iter().any(|filter| {
        !matches!(
            filter.as_slice(),
            b"FlateDecode"
                | b"LZWDecode"
                | b"ASCIIHexDecode"
                | b"ASCII85Decode"
                | b"RunLengthDecode"
        )
    }) {
        return Err(cannot(
            "It is stored in a format Letters can’t convert yet.",
        ));
    }
    let data = if filters.is_empty() {
        stream.content.clone()
    } else {
        stream
            .decompressed_content()
            .map_err(|_| cannot("Its data is damaged."))?
    };
    let width = dict_number(doc, &stream.dict, b"Width").unwrap_or(0.0) as u32;
    let height = dict_number(doc, &stream.dict, b"Height").unwrap_or(0.0) as u32;
    let bits = dict_number(doc, &stream.dict, b"BitsPerComponent").unwrap_or(8.0) as u32;
    if too_large(width, height) {
        return Err(cannot("Its size is damaged or too large."));
    }
    let rgb = to_rgb(doc, &stream.dict, &data, width, height, bits)?;
    let alpha = stream
        .dict
        .get(b"SMask")
        .ok()
        .and_then(|mask| resolve(doc, mask).as_stream().ok())
        .and_then(|mask| {
            let pixels = mask
                .decompressed_content()
                .ok()
                .or_else(|| (!mask.dict.has(b"Filter")).then(|| mask.content.clone()))?;
            let same_size = dict_number(doc, &mask.dict, b"Width") == Some(width as f32)
                && dict_number(doc, &mask.dict, b"Height") == Some(height as f32);
            (same_size && pixels.len() >= (width * height) as usize).then_some(pixels)
        });
    encode_png(&rgb, alpha.as_deref(), width, height)
}

fn filters(doc: &Document, dict: &Dictionary) -> Vec<Vec<u8>> {
    match dict.get(b"Filter").map(|filter| resolve(doc, filter)) {
        Ok(Object::Name(name)) => vec![name.clone()],
        Ok(Object::Array(items)) => items
            .iter()
            .filter_map(|item| item.as_name().ok())
            .map(<[u8]>::to_vec)
            .collect(),
        _ => Vec::new(),
    }
}

/// the picture's pixels as 8 bit rgb, for the colour spaces most files use.
fn to_rgb(
    doc: &Document,
    dict: &Dictionary,
    data: &[u8],
    width: u32,
    height: u32,
    bits: u32,
) -> Result<Vec<u8>, EditError> {
    let pixels = (width * height) as usize;
    let space = dict
        .get(b"ColorSpace")
        .map(|space| resolve(doc, space).clone())
        .unwrap_or(Object::Name(b"DeviceGray".to_vec()));
    let components = |space: &Object| -> Option<usize> {
        match space {
            Object::Name(name) => match name.as_slice() {
                b"DeviceGray" | b"CalGray" | b"G" => Some(1),
                b"DeviceRGB" | b"CalRGB" | b"RGB" => Some(3),
                b"DeviceCMYK" | b"CMYK" => Some(4),
                _ => None,
            },
            Object::Array(items) => match items.first().and_then(|item| item.as_name().ok()) {
                Some(b"ICCBased") => items
                    .get(1)
                    .and_then(|profile| resolve(doc, profile).as_stream().ok())
                    .and_then(|profile| dict_number(doc, &profile.dict, b"N"))
                    .map(|n| n as usize),
                Some(b"CalRGB") => Some(3),
                Some(b"CalGray") => Some(1),
                _ => None,
            },
            _ => None,
        }
    };
    if let Object::Array(items) = &space {
        if items.first().and_then(|item| item.as_name().ok()) == Some(b"Indexed") && bits == 8 {
            return indexed_to_rgb(doc, items, data, pixels);
        }
    }
    let count = components(&space)
        .ok_or_else(|| cannot("Its colours are stored in a way Letters can’t convert yet."))?;
    if bits == 1 && count == 1 {
        let row = (width as usize).div_ceil(8);
        let mut rgb = Vec::with_capacity(pixels * 3);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let byte = data.get(y * row + x / 8).copied().unwrap_or(0);
                let value = if byte & (0x80 >> (x % 8)) != 0 {
                    255
                } else {
                    0
                };
                rgb.extend_from_slice(&[value; 3]);
            }
        }
        return Ok(rgb);
    }
    if bits != 8 || data.len() < pixels * count {
        return Err(cannot(
            "Its colours are stored in a way Letters can’t convert yet.",
        ));
    }
    Ok(data[..pixels * count]
        .chunks_exact(count)
        .flat_map(|pixel| match pixel {
            [grey] => [*grey; 3],
            [r, g, b] => [*r, *g, *b],
            [c, m, y, k] => {
                let channel = |value: u8| 255 - (u16::from(value) + u16::from(*k)).min(255) as u8;
                [channel(*c), channel(*m), channel(*y)]
            }
            _ => [0; 3],
        })
        .collect())
}

fn indexed_to_rgb(
    doc: &Document,
    items: &[Object],
    data: &[u8],
    pixels: usize,
) -> Result<Vec<u8>, EditError> {
    let base_is_rgb = items
        .get(1)
        .map(|base| resolve(doc, base))
        .and_then(|base| base.as_name().ok())
        == Some(b"DeviceRGB");
    let table = match items.get(3).map(|table| resolve(doc, table)) {
        Some(Object::String(bytes, _)) => bytes.clone(),
        Some(Object::Stream(stream)) => stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone()),
        _ => Vec::new(),
    };
    if !base_is_rgb || data.len() < pixels {
        return Err(cannot(
            "Its colours are stored in a way Letters can’t convert yet.",
        ));
    }
    Ok(data[..pixels]
        .iter()
        .flat_map(|index| {
            let at = usize::from(*index) * 3;
            [
                table.get(at).copied().unwrap_or(0),
                table.get(at + 1).copied().unwrap_or(0),
                table.get(at + 2).copied().unwrap_or(0),
            ]
        })
        .collect())
}

fn encode_png(
    rgb: &[u8],
    alpha: Option<&[u8]>,
    width: u32,
    height: u32,
) -> Result<ExtractedImage, EditError> {
    let failed = |_| cannot("The picture couldn’t be written as a PNG.");
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_depth(png::BitDepth::Eight);
        let pixels: Vec<u8> = match alpha {
            Some(alpha) => {
                encoder.set_color(png::ColorType::Rgba);
                rgb.as_chunks::<3>()
                    .0
                    .iter()
                    .zip(alpha)
                    .flat_map(|(pixel, a)| [pixel[0], pixel[1], pixel[2], *a])
                    .collect()
            }
            None => {
                encoder.set_color(png::ColorType::Rgb);
                rgb.to_vec()
            }
        };
        let mut writer = encoder.write_header().map_err(failed)?;
        writer.write_image_data(&pixels).map_err(failed)?;
        writer.finish().map_err(failed)?;
    }
    Ok(ExtractedImage {
        format: "png".into(),
        bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_size_from_a_jpeg_header() {
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00];
        bytes.extend_from_slice(&[0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x02, 0x00, 0x03, 0x03]);
        bytes.extend_from_slice(&[0; 9]);
        let header = read_jpeg_header(&bytes).unwrap();
        assert_eq!((header.width, header.height, header.components), (3, 2, 3));
    }
}
