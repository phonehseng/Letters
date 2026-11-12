//! the picture of one page that recognition starts from.
//!
//! pictures come from two places: pages of a pdf drawn by our renderer
//! (raw rgba pixels or png bytes, at a resolution the caller chose), and
//! image files a person picked (png, jpeg or tiff). either way only the
//! gray levels are kept, which is all text recognition needs and a quarter
//! of the memory of colour.

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, GrayImage, ImageDecoder, ImageReader, Luma};

use crate::metadata::read_dpi;
use crate::OcrError;

/// the resolution pages are drawn at for recognition unless told otherwise.
/// 300 dots per inch is what tesseract is tuned for.
pub const DEFAULT_DPI: f32 = 300.0;

/// the largest picture accepted, in megapixels. a letter page at 600 dpi
/// is about 34 megapixels; anything far beyond is a mistake or an attack.
pub const MAX_MEGAPIXELS: u32 = 150;

/// one page as gray pixels, and how many pixels make an inch.
#[derive(Clone, Debug, PartialEq)]
pub struct PageImage {
    pub pixels: GrayImage,
    pub dpi: f32,
}

impl PageImage {
    pub fn from_gray(pixels: GrayImage, dpi: f32) -> Self {
        Self { pixels, dpi }
    }

    /// raw rgba pixels, four bytes each, rows from the top, as our renderer
    /// gives them. see-through pixels are treated as lying on white paper.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8], dpi: f32) -> Result<Self, OcrError> {
        check_size(width, height)?;
        if rgba.len() != width as usize * height as usize * 4 {
            return Err(OcrError::UnreadableImage(
                "The picture’s size doesn’t match its pixels.".into(),
            ));
        }
        let mut pixels = GrayImage::new(width, height);
        let (quads, _) = rgba.as_chunks::<4>();
        for (pixel, rgba) in pixels.pixels_mut().zip(quads) {
            *pixel = Luma([gray_on_white(rgba)]);
        }
        Ok(Self { pixels, dpi })
    }

    /// an encoded picture (png, jpeg or tiff). the resolution comes from
    /// `dpi` when given, else from the file, else `DEFAULT_DPI`. a photo
    /// is turned upright the way the camera recorded.
    pub fn from_encoded(bytes: &[u8], dpi: Option<f32>) -> Result<Self, OcrError> {
        let dpi = dpi
            .or_else(|| read_dpi(bytes).map(|dpi| dpi.x))
            .unwrap_or(DEFAULT_DPI);
        let picture = decode_upright(bytes)?;
        check_size(picture.width(), picture.height())?;
        Ok(Self {
            pixels: to_gray_on_white(picture),
            dpi,
        })
    }

    /// reads an image file from disk.
    pub fn open(path: &Path) -> Result<Self, OcrError> {
        let bytes =
            std::fs::read(path).map_err(|error| OcrError::UnreadableImage(format!("({error})")))?;
        Self::from_encoded(&bytes, None)
    }

    /// width of the page in points (1/72 inch).
    pub fn width_points(&self) -> f32 {
        self.pixels.width() as f32 * 72.0 / self.dpi
    }

    pub fn height_points(&self) -> f32 {
        self.pixels.height() as f32 * 72.0 / self.dpi
    }
}

/// decodes a picture and applies the camera's orientation, so a phone
/// photo taken sideways reads the right way up.
pub(crate) fn decode_upright(bytes: &[u8]) -> Result<DynamicImage, OcrError> {
    let unreadable = |error: image::ImageError| OcrError::UnreadableImage(format!("({error})"));
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| OcrError::UnreadableImage(format!("({error})")))?;
    let mut decoder = reader.into_decoder().map_err(unreadable)?;
    let (width, height) = decoder.dimensions();
    check_size(width, height)?;
    let orientation = decoder.orientation().map_err(unreadable)?;
    let mut picture = DynamicImage::from_decoder(decoder).map_err(unreadable)?;
    picture.apply_orientation(orientation);
    Ok(picture)
}

fn check_size(width: u32, height: u32) -> Result<(), OcrError> {
    if width == 0 || height == 0 {
        return Err(OcrError::UnreadableImage("The picture is empty.".into()));
    }
    let megapixels = u64::from(width) * u64::from(height) / 1_000_000;
    if megapixels > u64::from(MAX_MEGAPIXELS) {
        return Err(OcrError::ImageTooLarge(MAX_MEGAPIXELS));
    }
    Ok(())
}

fn to_gray_on_white(picture: DynamicImage) -> GrayImage {
    if !picture.color().has_alpha() {
        return picture.into_luma8();
    }
    let rgba = picture.into_rgba8();
    let mut gray = GrayImage::new(rgba.width(), rgba.height());
    for (pixel, rgba) in gray.pixels_mut().zip(rgba.pixels()) {
        *pixel = Luma([gray_on_white(&rgba.0)]);
    }
    gray
}

/// the brightness of a pixel as it would look on white paper.
///
/// uses the rec. 709 weights, the same ones the `image` crate uses, so a
/// picture reads the same whichever way it arrived.
fn gray_on_white(rgba: &[u8]) -> u8 {
    let luma =
        0.2126 * f32::from(rgba[0]) + 0.7152 * f32::from(rgba[1]) + 0.0722 * f32::from(rgba[2]);
    let alpha = f32::from(rgba[3]) / 255.0;
    (luma * alpha + 255.0 * (1.0 - alpha))
        .round()
        .clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_becomes_gray_on_white_paper() {
        // opaque black, see-through black, opaque white
        let rgba = [0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255];
        let page = PageImage::from_rgba(3, 1, &rgba, 300.0).unwrap();
        let values: Vec<u8> = page.pixels.pixels().map(|pixel| pixel.0[0]).collect();
        assert_eq!(values, [0, 255, 255]);
    }

    #[test]
    fn rgba_of_the_wrong_length_is_refused() {
        assert!(PageImage::from_rgba(2, 2, &[0; 12], 300.0).is_err());
    }

    #[test]
    fn page_size_in_points_follows_the_resolution() {
        let page = PageImage::from_gray(GrayImage::new(2550, 3300), 300.0);
        assert_eq!((page.width_points(), page.height_points()), (612.0, 792.0));
    }

    #[test]
    fn png_bytes_keep_their_resolution() {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, 4, 4);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: 7874,
            yppu: 7874,
            unit: png::Unit::Meter,
        }));
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[128; 16])
            .unwrap();
        let page = PageImage::from_encoded(&bytes, None).unwrap();
        assert!((page.dpi - 200.0).abs() < 0.1);
        assert_eq!(page.pixels.get_pixel(1, 1).0[0], 128);
        // an explicit resolution wins
        assert_eq!(
            PageImage::from_encoded(&bytes, Some(72.0)).unwrap().dpi,
            72.0
        );
    }

    #[test]
    fn garbage_is_unreadable() {
        assert!(matches!(
            PageImage::from_encoded(b"not a picture", None),
            Err(OcrError::UnreadableImage(_))
        ));
    }
}
