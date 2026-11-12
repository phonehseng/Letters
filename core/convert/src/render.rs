//! drawing pages as pictures. the drawing itself is done by whoever calls
//! this crate (the app passes its pdf engine), so this crate never loads
//! a pdf library of its own.

use std::path::Path;

use crate::ConvertError;

/// draws pages of a pdf file as rgba pixels.
///
/// the app implements this with its render worker; the tests do the same
/// with letters-pdf. `scale` is pixels per point (1/72 inch), so 300 dpi
/// is a scale of 300 / 72. the page should be drawn onto white, the way
/// it looks on paper. the file is named by path because some conversions
/// draw a changed copy of the document (with the text taken out, say).
pub trait PageRenderer {
    fn render(&mut self, pdf: &Path, page: usize, scale: f32) -> Result<RgbaImage, ConvertError>;
}

/// a picture as four bytes per pixel (red, green, blue, alpha), row by
/// row from the top.
#[derive(Clone, Debug, PartialEq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// checks that the pixel count matches the size.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, ConvertError> {
        if pixels.len() != width as usize * height as usize * 4 || width == 0 || height == 0 {
            return Err(ConvertError::RenderFailed(format!(
                "the drawn page has {} bytes for {width} by {height} pixels",
                pixels.len()
            )));
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// trims the picture to the size the page should have at its
    /// resolution. renderers round sizes up (and some store the scale
    /// rounded), so a page can come back a pixel or two larger than
    /// width × dpi / 72; trimming that sliver keeps the stated resolution
    /// true. a picture that is already small enough is left as it is.
    pub(crate) fn trimmed_to(self, width: u32, height: u32) -> RgbaImage {
        if width >= self.width && height >= self.height {
            return self;
        }
        let (width, height) = (width.min(self.width), height.min(self.height));
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
        for row in 0..height as usize {
            let start = row * self.width as usize * 4;
            pixels.extend_from_slice(&self.pixels[start..start + width as usize * 4]);
        }
        RgbaImage {
            width,
            height,
            pixels,
        }
    }

    /// red, green, blue per pixel, any transparency laid over white.
    pub fn to_rgb(&self) -> Vec<u8> {
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|pixel| {
                let [r, g, b] = over_white(pixel);
                [r, g, b]
            })
            .collect()
    }

    /// one gray level per pixel, using the usual weights for how bright
    /// each colour looks (itu-r bt.601).
    pub fn to_gray(&self) -> Vec<u8> {
        self.pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| {
                let [r, g, b] = over_white(pixel);
                let gray = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
                gray.round().clamp(0.0, 255.0) as u8
            })
            .collect()
    }
}

/// mixes a pixel with white by its alpha, so a page drawn with a
/// transparent background still looks like paper.
fn over_white(pixel: &[u8]) -> [u8; 3] {
    let alpha = u32::from(pixel[3]);
    let mix = |channel: u8| ((u32::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
    [mix(pixel[0]), mix(pixel[1]), mix(pixel[2])]
}

/// pixels per point for a resolution in dots per inch.
pub fn scale_for_dpi(dpi: f32) -> f32 {
    dpi / 72.0
}

/// how many pixels a page of this many points should be at `dpi`.
pub(crate) fn pixels_at(points: f32, dpi: f32) -> u32 {
    (points * dpi / 72.0).round().max(1.0) as u32
}

/// draws a page at `dpi` and trims it to its exact size.
pub(crate) fn render_at_dpi(
    renderer: &mut dyn PageRenderer,
    pdf: &Path,
    page: usize,
    size_pt: (f32, f32),
    dpi: f32,
) -> Result<RgbaImage, ConvertError> {
    let image = renderer.render(pdf, page, scale_for_dpi(dpi))?;
    Ok(image.trimmed_to(pixels_at(size_pt.0, dpi), pixels_at(size_pt.1, dpi)))
}

/// the largest picture any single page may become: 20 000 pixels a side
/// and 200 megapixels, so a huge page at a high resolution is refused
/// with a message instead of running out of memory.
pub const MAX_SIDE_PIXELS: f32 = 20_000.0;
pub const MAX_MEGAPIXELS: f32 = 200.0;

/// checks that a page drawn at `dpi` stays within the limits above.
pub(crate) fn check_size(width_pt: f32, height_pt: f32, dpi: f32) -> Result<(), ConvertError> {
    let scale = scale_for_dpi(dpi);
    let (width, height) = (width_pt * scale, height_pt * scale);
    if width > MAX_SIDE_PIXELS || height > MAX_SIDE_PIXELS || width * height / 1e6 > MAX_MEGAPIXELS
    {
        return Err(ConvertError::InvalidSetting(format!(
            "At {dpi} dpi a page would be {} by {} pixels, which is too large. Choose a lower resolution.",
            width.round(),
            height.round()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transparent_pixels_become_white() {
        let image = RgbaImage::new(2, 1, vec![0, 0, 0, 0, 255, 0, 0, 255]).unwrap();
        assert_eq!(image.to_rgb(), vec![255, 255, 255, 255, 0, 0]);
        assert_eq!(image.to_gray(), vec![255, 76]);
    }

    #[test]
    fn wrong_sizes_are_refused() {
        assert!(RgbaImage::new(2, 2, vec![0; 4]).is_err());
    }

    #[test]
    fn huge_pictures_are_refused() {
        assert!(check_size(612.0, 792.0, 300.0).is_ok());
        assert!(check_size(612.0, 792.0, 3000.0).is_err());
    }
}
