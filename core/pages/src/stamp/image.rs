//! turning png or jpeg bytes into a pdf image.
//!
//! jpeg files can be stored in a pdf exactly as they are, because pdf
//! readers decode jpeg themselves; only the size and colour layout have to
//! be read from the file's header. png files are decoded to plain pixels
//! and compressed again, with any transparency kept as a separate mask
//! image, which is how pdf stores transparency.

use std::io::Cursor;

use lopdf::{Dictionary, Object, Stream};

use crate::PagesError;

/// a picture ready to be added to a document.
pub(crate) struct PdfImage {
    pub width: u32,
    pub height: u32,
    /// the image itself, without its mask
    pub stream: Stream,
    /// the transparency, when the picture has any
    pub mask: Option<Stream>,
}

/// reads png or jpeg bytes, telling the two apart by their first bytes.
pub(crate) fn decode(bytes: &[u8]) -> Result<PdfImage, PagesError> {
    if bytes.starts_with(b"\x89PNG") {
        from_png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        from_jpeg(bytes)
    } else {
        Err(unsupported("The file isn’t a PNG or JPEG picture."))
    }
}

fn unsupported(reason: &str) -> PagesError {
    PagesError::UnsupportedImage(reason.to_owned())
}

/// the dictionary every image needs: its size and how its colours are stored.
fn image_dictionary(width: u32, height: u32, colour_space: &str) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"XObject".to_vec()));
    dict.set("Subtype", Object::Name(b"Image".to_vec()));
    dict.set("Width", Object::Integer(i64::from(width)));
    dict.set("Height", Object::Integer(i64::from(height)));
    dict.set("ColorSpace", Object::Name(colour_space.as_bytes().to_vec()));
    dict.set("BitsPerComponent", Object::Integer(8));
    dict
}

/// a flate-compressed image stream. compression is what keeps a decoded
/// png from taking width x height x 3 bytes in the file.
fn compressed_image(dict: Dictionary, pixels: Vec<u8>) -> Stream {
    let mut stream = Stream::new(dict, pixels);
    // if compressing fails the pixels are simply stored as they are
    let _ = stream.compress();
    stream
}

fn from_png(bytes: &[u8]) -> Result<PdfImage, PagesError> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    // palette and 16 bit pictures are turned into plain 8 bit colours,
    // so only four layouts are left to handle below
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|error| unsupported(&error.to_string()))?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| unsupported("The picture is too big."))?;
    let mut pixels = vec![0; size];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|error| unsupported(&error.to_string()))?;
    pixels.truncate(info.buffer_size());

    let (colour_space, channels, has_alpha) = match info.color_type {
        png::ColorType::Grayscale => ("DeviceGray", 1, false),
        png::ColorType::GrayscaleAlpha => ("DeviceGray", 1, true),
        png::ColorType::Rgb => ("DeviceRGB", 3, false),
        png::ColorType::Rgba => ("DeviceRGB", 3, true),
        png::ColorType::Indexed => return Err(unsupported("The picture’s colours can’t be read.")),
    };
    let (colour, alpha) = if has_alpha {
        split_alpha(&pixels, channels)
    } else {
        (pixels, Vec::new())
    };
    let (width, height) = (info.width, info.height);
    let mask =
        has_alpha.then(|| compressed_image(image_dictionary(width, height, "DeviceGray"), alpha));
    Ok(PdfImage {
        width,
        height,
        stream: compressed_image(image_dictionary(width, height, colour_space), colour),
        mask,
    })
}

/// separates interleaved colour and alpha bytes into two planes.
fn split_alpha(pixels: &[u8], colour_channels: usize) -> (Vec<u8>, Vec<u8>) {
    let pixel_size = colour_channels + 1;
    let count = pixels.len() / pixel_size;
    let mut colour = Vec::with_capacity(count * colour_channels);
    let mut alpha = Vec::with_capacity(count);
    for pixel in pixels.chunks_exact(pixel_size) {
        colour.extend_from_slice(&pixel[..colour_channels]);
        alpha.push(pixel[colour_channels]);
    }
    (colour, alpha)
}

fn from_jpeg(bytes: &[u8]) -> Result<PdfImage, PagesError> {
    let header = jpeg_header(bytes).ok_or_else(|| unsupported("The JPEG file is damaged."))?;
    let colour_space = match header.components {
        1 => "DeviceGray",
        3 => "DeviceRGB",
        _ => {
            return Err(unsupported(
                "Only colour and greyscale JPEG pictures can be used.",
            ))
        }
    };
    let mut dict = image_dictionary(header.width, header.height, colour_space);
    dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    // already compressed, so it must not be compressed a second time
    let stream = Stream::new(dict, bytes.to_vec()).with_compression(false);
    Ok(PdfImage {
        width: header.width,
        height: header.height,
        stream,
        mask: None,
    })
}

struct JpegHeader {
    width: u32,
    height: u32,
    components: u8,
}

/// finds the "start of frame" block of a jpeg, which holds its size.
///
/// a jpeg is a list of blocks, each starting with 0xFF, a marker byte and
/// (for most markers) a two byte length. the walk skips blocks until it
/// reaches a frame header, and gives up at the start of the image data.
fn jpeg_header(bytes: &[u8]) -> Option<JpegHeader> {
    let mut position = 2;
    while position + 4 <= bytes.len() {
        if bytes[position] != 0xFF {
            return None;
        }
        let marker = bytes[position + 1];
        // padding between blocks
        if marker == 0xFF {
            position += 1;
            continue;
        }
        let length = usize::from(u16::from_be_bytes([
            bytes[position + 2],
            bytes[position + 3],
        ]));
        let is_frame = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_frame {
            let frame = bytes.get(position + 4..position + 10)?;
            return Some(JpegHeader {
                height: u32::from(u16::from_be_bytes([frame[1], frame[2]])),
                width: u32::from(u16::from_be_bytes([frame[3], frame[4]])),
                components: frame[5],
            })
            .filter(|header| header.width > 0 && header.height > 0);
        }
        // start of scan: the pixels begin and no frame header was found
        if marker == 0xDA {
            return None;
        }
        position += 2 + length;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// the smallest jpeg header that names a size: start of image, then a
    /// baseline frame header for a 3 component, 40 by 30 picture.
    fn tiny_jpeg_header() -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8];
        // an app block to skip over first
        bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00]);
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 30, 0x00, 40, 3]);
        bytes.extend_from_slice(&[0; 9]);
        bytes
    }

    #[test]
    fn reads_the_size_from_a_jpeg_header() {
        let image = decode(&tiny_jpeg_header()).unwrap();
        assert_eq!((image.width, image.height), (40, 30));
        assert!(image.mask.is_none());
    }

    #[test]
    fn refuses_files_that_are_not_pictures() {
        assert!(matches!(
            decode(b"%PDF-1.7"),
            Err(PagesError::UnsupportedImage(_))
        ));
        assert!(matches!(
            decode(&[0xFF, 0xD8, 0xFF, 0xDA, 0, 2]),
            Err(PagesError::UnsupportedImage(_))
        ));
    }

    #[test]
    fn splits_colour_from_transparency() {
        let (colour, alpha) = split_alpha(&[1, 2, 3, 9, 4, 5, 6, 8], 3);
        assert_eq!(colour, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(alpha, vec![9, 8]);
    }
}
