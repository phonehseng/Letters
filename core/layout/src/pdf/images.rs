//! pictures for the pdf: jpeg files go in as they are (the pdf reader
//! decodes them); png files are decoded and stored deflated, with their
//! alpha channel as a soft mask.

use lopdf::{Dictionary, Object, ObjectId, Stream};

fn name(text: &str) -> Object {
    Object::Name(text.as_bytes().to_vec())
}

/// adds a picture as an image xobject; `None` for formats this writer
/// cannot place (emf, wmf, svg...).
pub fn add_image(
    document: &mut lopdf::Document,
    bytes: &[u8],
    content_type: &str,
) -> Option<ObjectId> {
    if content_type.contains("jpeg") || bytes.starts_with(&[0xff, 0xd8]) {
        return add_jpeg(document, bytes);
    }
    if content_type.contains("png") || bytes.starts_with(b"\x89PNG") {
        return add_png(document, bytes);
    }
    None
}

fn add_jpeg(document: &mut lopdf::Document, bytes: &[u8]) -> Option<ObjectId> {
    let (width, height, components) = jpeg_size(bytes)?;
    let mut dict = Dictionary::new();
    dict.set("Type", name("XObject"));
    dict.set("Subtype", name("Image"));
    dict.set("Width", Object::Integer(i64::from(width)));
    dict.set("Height", Object::Integer(i64::from(height)));
    dict.set(
        "ColorSpace",
        name(match components {
            1 => "DeviceGray",
            4 => "DeviceCMYK",
            _ => "DeviceRGB",
        }),
    );
    dict.set("BitsPerComponent", Object::Integer(8));
    dict.set("Filter", name("DCTDecode"));
    Some(document.add_object(Stream::new(dict, bytes.to_vec())))
}

/// width, height and colour components from a jpeg's start-of-frame.
fn jpeg_size(bytes: &[u8]) -> Option<(u16, u16, u8)> {
    let mut at = 2;
    while at + 9 < bytes.len() {
        if bytes[at] != 0xff {
            at += 1;
            continue;
        }
        let marker = bytes[at + 1];
        let length = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
        // sof0..sof15, except dht (c4), jpg (c8) and dac (cc)
        if (0xc0..=0xcf).contains(&marker) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            let height = u16::from_be_bytes([bytes[at + 5], bytes[at + 6]]);
            let width = u16::from_be_bytes([bytes[at + 7], bytes[at + 8]]);
            return Some((width, height, bytes[at + 9]));
        }
        at += 2 + length;
    }
    None
}

fn add_png(document: &mut lopdf::Document, bytes: &[u8]) -> Option<ObjectId> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let pixels = &buffer[..info.buffer_size()];
    let (color, alpha): (Vec<u8>, Option<Vec<u8>>) = match info.color_type {
        png::ColorType::Rgb => (pixels.to_vec(), None),
        png::ColorType::Rgba => (
            pixels.chunks(4).flat_map(|p| [p[0], p[1], p[2]]).collect(),
            Some(pixels.chunks(4).map(|p| p[3]).collect()),
        ),
        png::ColorType::Grayscale => (pixels.iter().flat_map(|g| [*g, *g, *g]).collect(), None),
        png::ColorType::GrayscaleAlpha => (
            pixels.chunks(2).flat_map(|p| [p[0], p[0], p[0]]).collect(),
            Some(pixels.chunks(2).map(|p| p[1]).collect()),
        ),
        png::ColorType::Indexed => return None,
    };
    let image = |data: Vec<u8>, space: &str| {
        let mut dict = Dictionary::new();
        dict.set("Type", name("XObject"));
        dict.set("Subtype", name("Image"));
        dict.set("Width", Object::Integer(i64::from(info.width)));
        dict.set("Height", Object::Integer(i64::from(info.height)));
        dict.set("ColorSpace", name(space));
        dict.set("BitsPerComponent", Object::Integer(8));
        let mut stream = Stream::new(dict, data);
        let _ = stream.compress();
        stream
    };
    let mut main = image(color, "DeviceRGB");
    if let Some(alpha) = alpha {
        let mask = document.add_object(image(alpha, "DeviceGray"));
        main.dict.set("SMask", Object::Reference(mask));
    }
    Some(document.add_object(main))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jpeg_size_is_read_from_the_frame_header() {
        // soi, app0 (length 4), sof0 with 8 bit, 30 high, 40 wide, 3 components
        let bytes = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00,
            0x1e, 0x00, 0x28, 0x03, 0, 0, 0, 0,
        ];
        assert_eq!(jpeg_size(&bytes), Some((40, 30, 3)));
    }
}
