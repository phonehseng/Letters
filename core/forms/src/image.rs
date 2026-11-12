//! turning a png picture into a pdf image, keeping its see-through parts.
//!
//! pdf images have no alpha channel of their own. the colours go in one
//! image and the transparency in a second, grey "soft mask" image that the
//! first one points to (/SMask). both are compressed with flate.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::error::{FormError, FormResult};
use crate::objects::name;

/// pictures bigger than this many pixels are refused, so a huge file
/// can't use up the computer's memory. 4000 by 4000 is plenty for a signature.
const MAX_PIXELS: u64 = 16_000_000;

/// a decoded picture: 8 bit rgb, and alpha when the picture has any.
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
    pub alpha: Option<Vec<u8>>,
}

/// reads a png of any colour type into 8 bit rgb plus alpha.
pub fn decode_png(bytes: &[u8]) -> FormResult<DecodedImage> {
    let bad = |error: png::DecodingError| FormError::BadImage(error.to_string());
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // palettes and low bit depths become plain 8 bit colours
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(bad)?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width == 0 || height == 0 || width as u64 * height as u64 > MAX_PIXELS {
        return Err(FormError::BadImage(
            "the picture is empty or too big".into(),
        ));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| FormError::BadImage("the picture is too big".into()))?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(bad)?;
    buffer.truncate(frame.buffer_size());
    let pixels = (width * height) as usize;
    let (rgb, alpha) = match frame.color_type {
        png::ColorType::Rgba => {
            split_channels(&buffer, 4, pixels, |px| [px[0], px[1], px[2]], |px| px[3])
        }
        png::ColorType::Rgb => (buffer, None),
        png::ColorType::GrayscaleAlpha => {
            split_channels(&buffer, 2, pixels, |px| [px[0], px[0], px[0]], |px| px[1])
        }
        png::ColorType::Grayscale => (
            buffer.iter().flat_map(|&grey| [grey, grey, grey]).collect(),
            None,
        ),
        png::ColorType::Indexed => {
            return Err(FormError::BadImage(
                "the picture's colours could not be read".into(),
            ))
        }
    };
    Ok(DecodedImage {
        width,
        height,
        rgb,
        alpha,
    })
}

fn split_channels(
    buffer: &[u8],
    channels: usize,
    pixels: usize,
    colour: impl Fn(&[u8]) -> [u8; 3],
    opacity: impl Fn(&[u8]) -> u8,
) -> (Vec<u8>, Option<Vec<u8>>) {
    let mut rgb = Vec::with_capacity(pixels * 3);
    let mut alpha = Vec::with_capacity(pixels);
    for pixel in buffer.chunks_exact(channels).take(pixels) {
        rgb.extend_from_slice(&colour(pixel));
        alpha.push(opacity(pixel));
    }
    // a picture that is opaque everywhere needs no mask
    let alpha = alpha.iter().any(|&value| value != 255).then_some(alpha);
    (rgb, alpha)
}

/// adds a png to the document as an image xobject (with a soft mask for
/// its transparency) and returns the image's object id.
pub fn add_png_image(doc: &mut Document, png: &[u8]) -> FormResult<ObjectId> {
    let image = decode_png(png)?;
    let mask = image.alpha.as_ref().map(|alpha| {
        doc.add_object(image_stream(
            image.width,
            image.height,
            "DeviceGray",
            alpha.clone(),
            None,
        ))
    });
    Ok(doc.add_object(image_stream(
        image.width,
        image.height,
        "DeviceRGB",
        image.rgb,
        mask,
    )))
}

fn image_stream(
    width: u32,
    height: u32,
    colour_space: &str,
    samples: Vec<u8>,
    mask: Option<ObjectId>,
) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", name("XObject"));
    dict.set("Subtype", name("Image"));
    dict.set("Width", Object::Integer(width as i64));
    dict.set("Height", Object::Integer(height as i64));
    dict.set("ColorSpace", name(colour_space));
    dict.set("BitsPerComponent", Object::Integer(8));
    if let Some(mask) = mask {
        dict.set("SMask", Object::Reference(mask));
    }
    let mut stream = Stream::new(dict, samples);
    // pictures compress well, and a failed compression just stays raw
    let _ = stream.compress();
    stream
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(width: u32, height: u32, color: png::ColorType, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
        writer.finish().unwrap();
        out
    }

    #[test]
    fn keeps_transparency_apart() {
        let png = encode(2, 1, png::ColorType::Rgba, &[255, 0, 0, 255, 0, 0, 255, 0]);
        let image = decode_png(&png).unwrap();
        assert_eq!(image.rgb, [255, 0, 0, 0, 0, 255]);
        assert_eq!(image.alpha.unwrap(), [255, 0]);
    }

    #[test]
    fn opaque_and_grey_pictures_need_no_mask() {
        let png = encode(2, 1, png::ColorType::Grayscale, &[0, 200]);
        let image = decode_png(&png).unwrap();
        assert_eq!(image.rgb, [0, 0, 0, 200, 200, 200]);
        assert!(image.alpha.is_none());
    }

    #[test]
    fn refuses_what_is_not_a_png() {
        assert!(matches!(decode_png(b"GIF89a"), Err(FormError::BadImage(_))));
    }
}
