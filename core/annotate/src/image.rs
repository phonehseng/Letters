//! pictures placed on a page as stamp annotations (/Stamp).
//!
//! signatures and pasted pictures are kept as annotations rather than
//! page content, so they can still be moved or removed until the file
//! is flattened. jpeg files are stored exactly as they are (pdf can show
//! them directly); png files are decoded and stored compressed, with any
//! transparency kept as a soft mask.

use std::io::Cursor;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::annots;
use crate::appearance::{self, Appearance, Canvas};
use crate::common::{self, Basics};
use crate::error::AnnotateError;
use crate::geometry::Rect;
use crate::page_space::PageSpace;
use crate::pdf_values::{dict_dict, dict_name, dict_rect, name, number_array, resolve};
use crate::read::{self, AnnotationInfo};

/// the largest picture accepted, in pixels. a 50 megapixel photo is
/// already far sharper than any page needs, and decoding a larger one
/// could use gigabytes of memory.
const MAX_PIXELS: u64 = 50_000_000;

/// the /Name Letters gives picture stamps, so they can be told apart
/// from rubber stamps such as "Approved".
const IMAGE_STAMP_NAME: &str = "Image";

/// what the app sends to place a picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageParams {
    /// where the picture goes, in visible page points. it is stretched
    /// to fill the rectangle, so keep its proportions in the ui
    pub rect: Rect,
    /// the bytes of a png or jpeg file
    pub image: Vec<u8>,
    #[serde(default = "crate::full_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub contents: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

/// places a png or jpeg picture on a page.
pub fn add_image(
    doc: &mut Document,
    page: usize,
    params: &ImageParams,
) -> Result<AnnotationInfo, AnnotateError> {
    let page = annots::page(doc, page)?;
    common::check_rect(params.rect)?;
    let image = image_object(&params.image)?;
    let basics = Basics {
        color: None,
        opacity: params.opacity,
        contents: params.contents.as_deref(),
        author: params.author.as_deref(),
    };
    let mut annotation = common::new_dictionary("Stamp", &page, &basics);
    annotation.set("Name", name(IMAGE_STAMP_NAME));
    annotation.set("Rect", number_array(&page.space.user_rect(params.rect)));

    let image_id = add_image_object(doc, image);
    let annotation_id = doc.add_object(annotation);
    let drawing = draw_picture(image_id, params.rect, params.opacity);
    appearance::install(doc, annotation_id, drawing, &page.space)?;
    annots::append(doc, page.id, annotation_id)?;
    read::info(doc, &page, annotation_id)
}

/// a decoded picture, ready to be stored: the image and its optional mask.
struct ImageObject {
    image: Stream,
    mask: Option<Stream>,
}

fn add_image_object(doc: &mut Document, object: ImageObject) -> ObjectId {
    let mut image = object.image;
    if let Some(mask) = object.mask {
        let mask_id = doc.add_object(Object::Stream(mask));
        image.dict.set("SMask", Object::Reference(mask_id));
    }
    doc.add_object(Object::Stream(image))
}

/// redraws a picture stamp, reusing the picture its appearance already
/// shows. stamps from other apps that aren't a single picture can't be
/// redrawn, only moved and resized (which needs no redrawing).
pub(crate) fn draw(
    doc: &Document,
    annotation: &Dictionary,
    space: &PageSpace,
) -> Result<Appearance, AnnotateError> {
    let frame = dict_rect(doc, annotation, b"Rect")
        .map(|rect| space.visible_rect(rect))
        .filter(|rect| !rect.is_empty())
        .ok_or(AnnotateError::EmptyArea)?;
    let picture = shown_picture(doc, annotation).ok_or(AnnotateError::CannotChange)?;
    Ok(draw_picture(
        picture,
        frame,
        common::opacity(doc, annotation),
    ))
}

/// true for stamps Letters made from a picture.
pub(crate) fn is_picture_stamp(doc: &Document, annotation: &Dictionary) -> bool {
    dict_name(doc, annotation, b"Name") == Some(IMAGE_STAMP_NAME.as_bytes())
        && shown_picture(doc, annotation).is_some()
}

/// the image object a stamp's normal appearance draws, if it draws one.
fn shown_picture(doc: &Document, annotation: &Dictionary) -> Option<ObjectId> {
    let normal = dict_dict(doc, annotation, b"AP")?
        .get(b"N")
        .ok()?
        .as_reference()
        .ok()?;
    let form = doc.get_object(normal).ok()?.as_stream().ok()?;
    let resources = dict_dict(doc, &form.dict, b"Resources")?;
    let images = dict_dict(doc, resources, b"XObject")?;
    images.iter().find_map(|(_, object)| {
        let id = object.as_reference().ok()?;
        let stream = resolve(doc, object).as_stream().ok()?;
        (dict_name(doc, &stream.dict, b"Subtype") == Some(b"Image")).then_some(id)
    })
}

fn draw_picture(image: ObjectId, frame: Rect, opacity: f32) -> Appearance {
    let mut canvas = Canvas::new(frame);
    canvas.opacity(opacity, false);
    canvas.image(image, frame);
    canvas.finish()
}

/// reads png or jpeg bytes into an image object.
fn image_object(bytes: &[u8]) -> Result<ImageObject, AnnotateError> {
    if bytes.starts_with(&[0xff, 0xd8]) {
        jpeg_object(bytes)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png_object(bytes)
    } else {
        Err(AnnotateError::UnsupportedImage(
            "Use a PNG or JPEG picture.".into(),
        ))
    }
}

fn too_large(width: u32, height: u32) -> bool {
    u64::from(width) * u64::from(height) > MAX_PIXELS || width == 0 || height == 0
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

/// what a jpeg's header says about the picture.
struct JpegHeader {
    width: u32,
    height: u32,
    components: u8,
    /// photoshop writes cmyk jpegs with inverted values and marks them
    /// with an "Adobe" segment
    adobe: bool,
}

/// stores a jpeg as it is. only its header is read, to learn its size
/// and colours; pdf readers decode the picture themselves (/DCTDecode).
fn jpeg_object(bytes: &[u8]) -> Result<ImageObject, AnnotateError> {
    let header = read_jpeg_header(bytes)
        .ok_or_else(|| AnnotateError::UnsupportedImage("The JPEG file is damaged.".into()))?;
    if too_large(header.width, header.height) {
        return Err(AnnotateError::UnsupportedImage(
            "It is too large. Use a smaller picture.".into(),
        ));
    }
    let color_space = match header.components {
        1 => "DeviceGray",
        3 => "DeviceRGB",
        4 => "DeviceCMYK",
        _ => {
            return Err(AnnotateError::UnsupportedImage(
                "Its colours aren’t supported.".into(),
            ))
        }
    };
    let mut dict = image_dictionary(header.width, header.height, color_space);
    dict.set("Filter", name("DCTDecode"));
    if header.components == 4 && header.adobe {
        dict.set(
            "Decode",
            number_array(&[1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]),
        );
    }
    Ok(ImageObject {
        image: Stream::new(dict, bytes.to_vec()).with_compression(false),
        mask: None,
    })
}

/// walks a jpeg's segments up to the frame header (SOFn) that holds the size.
fn read_jpeg_header(bytes: &[u8]) -> Option<JpegHeader> {
    let mut position = 2;
    let mut adobe = false;
    loop {
        // segments start with 0xff; extra 0xff bytes are padding
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
            // start of scan or end of image before any frame header
            0xda | 0xd9 => return None,
            _ => {}
        }
        position += 2 + length;
    }
}

/// decodes a png into 8 bit grey or rgb pixels plus an optional alpha mask.
fn png_object(bytes: &[u8]) -> Result<ImageObject, AnnotateError> {
    let damaged = |_| AnnotateError::UnsupportedImage("The PNG file is damaged.".into());
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    // palettes become rgb, 16 bit becomes 8 bit, and low bit depths are widened
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let header = decoder.read_header_info().map_err(damaged)?;
    if too_large(header.width, header.height) {
        return Err(AnnotateError::UnsupportedImage(
            "It is too large. Use a smaller picture.".into(),
        ));
    }
    let mut reader = decoder.read_info().map_err(damaged)?;
    let size = reader.output_buffer_size().ok_or_else(|| {
        AnnotateError::UnsupportedImage("It is too large. Use a smaller picture.".into())
    })?;
    let mut pixels = vec![0; size];
    let frame = reader.next_frame(&mut pixels).map_err(damaged)?;
    pixels.truncate(frame.buffer_size());

    let (channels, color_space, has_alpha) = match frame.color_type {
        png::ColorType::Grayscale => (1, "DeviceGray", false),
        png::ColorType::GrayscaleAlpha => (2, "DeviceGray", true),
        png::ColorType::Rgb => (3, "DeviceRGB", false),
        png::ColorType::Rgba => (4, "DeviceRGB", true),
        // normalize_to_color8 expands palettes, so this is not expected
        png::ColorType::Indexed => {
            return Err(AnnotateError::UnsupportedImage(
                "Its colours aren’t supported.".into(),
            ))
        }
    };
    let row_bytes = frame.line_size;
    let mut color = Vec::with_capacity(pixels.len());
    let mut alpha = Vec::new();
    for row in pixels.chunks_exact(row_bytes) {
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
    // a picture whose pixels are all solid needs no mask
    let mask = (has_alpha && alpha.iter().any(|&value| value < 255)).then(|| {
        compressed(
            image_dictionary(frame.width, frame.height, "DeviceGray"),
            alpha,
        )
    });
    Ok(ImageObject { image, mask })
}

fn compressed(dict: Dictionary, data: Vec<u8>) -> Stream {
    let mut stream = Stream::new(dict, data);
    // an uncompressed image is still valid, just larger
    let _ = stream.compress();
    stream
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a tiny jpeg header: SOI, an APP0 segment, then a baseline frame
    /// header for a 3 by 2 rgb picture. enough for the header reader.
    fn jpeg_header_bytes() -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8];
        bytes.extend_from_slice(&[0xff, 0xe0, 0x00, 0x04, 0x00, 0x00]);
        bytes.extend_from_slice(&[0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x02, 0x00, 0x03, 0x03]);
        bytes.extend_from_slice(&[0; 9]);
        bytes
    }

    #[test]
    fn reads_the_size_from_a_jpeg_header() {
        let header = read_jpeg_header(&jpeg_header_bytes()).unwrap();
        assert_eq!((header.width, header.height, header.components), (3, 2, 3));
        assert!(!header.adobe);
    }

    #[test]
    fn refuses_files_that_are_not_pictures() {
        assert!(matches!(
            image_object(b"GIF89a"),
            Err(AnnotateError::UnsupportedImage(_))
        ));
        assert!(matches!(
            image_object(&[0xff, 0xd8, 0x00]),
            Err(AnnotateError::UnsupportedImage(_))
        ));
    }
}
