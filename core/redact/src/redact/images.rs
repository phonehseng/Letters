//! images: finding which pixels lie under a redaction area and replacing
//! them with the fill colour.
//!
//! an image is drawn into the unit square of its own space, stretched onto
//! the page by the current matrix. its pixels run left to right and top to
//! bottom, so pixel (column i, row j) covers the square from i/w to
//! (i+1)/w across and from 1-(j+1)/h to 1-j/h up. any pixel that overlaps
//! an area even a little is replaced.
//!
//! the image is decoded (flate, lzw, run length, ascii filters with any
//! predictor, and jpeg), the pixels are changed, and it is written back
//! with flate, which loses nothing. images that cannot be decoded here
//! (jpeg 2000, jbig2, ccitt fax, cmyk jpeg) are not kept at all: the
//! caller removes them whole and says so, because leaving the original
//! pixels under a box would only hide them.

use lopdf::{Dictionary, Document, Object, Stream};

use crate::geometry::{Matrix, Point, Quad, Rect};
use crate::objects::{as_dict, dict_get, number, numbers, resolve, MAX_DECODED_BYTES};

/// which pixels of a `width` by `height` image lie under the areas.
pub struct Coverage {
    pub width: usize,
    pub height: usize,
    pub mask: Vec<bool>,
    pub covered: usize,
}

impl Coverage {
    pub fn is_full(&self) -> bool {
        self.covered == self.width * self.height
    }

    fn get(&self, x: usize, y: usize) -> bool {
        self.mask[y * self.width + x]
    }
}

/// works out the covered pixels, or none when the matrix squashes the
/// image flat (then it is invisible, and the caller removes it).
pub fn coverage(width: usize, height: usize, ctm: &Matrix, areas: &[Rect]) -> Option<Coverage> {
    let to_image = ctm.inverse()?;
    let mut result = Coverage {
        width,
        height,
        mask: vec![false; width * height],
        covered: 0,
    };
    // from the image's unit square to pixel units, with y pointing down
    let to_pixels = Matrix::new(width as f64, 0.0, 0.0, -(height as f64), 0.0, height as f64);
    for area in areas {
        let shape = Quad(
            area.transform(&to_image)
                .0
                .map(|corner| to_pixels.apply(corner)),
        );
        mark_pixels(&mut result, &shape);
    }
    Some(result)
}

fn mark_pixels(coverage: &mut Coverage, shape: &Quad) {
    let bounds = shape.bounds();
    let columns = pixel_range(bounds.x0, bounds.x1, coverage.width);
    let rows = pixel_range(bounds.y0, bounds.y1, coverage.height);
    let axis_aligned = is_axis_aligned(shape);
    for y in rows.clone() {
        for x in columns.clone() {
            let index = y * coverage.width + x;
            if coverage.mask[index] {
                continue;
            }
            let pixel = Rect::from_corners(x as f64, y as f64, x as f64 + 1.0, y as f64 + 1.0);
            // for a straight area the bounds are the shape, so every pixel
            // in the range overlaps it
            if axis_aligned || shape.overlaps(&pixel) {
                coverage.mask[index] = true;
                coverage.covered += 1;
            }
        }
    }
}

fn is_axis_aligned(shape: &Quad) -> bool {
    let [a, b, c, d] = shape.0;
    let flat = |p: Point, q: Point| (p.x - q.x).abs() < 1e-9 || (p.y - q.y).abs() < 1e-9;
    flat(a, b) && flat(b, c) && flat(c, d) && flat(d, a)
}

/// the pixel indices a span from `low` to `high` overlaps by more than a
/// hair, limited to the image.
fn pixel_range(low: f64, high: f64, count: usize) -> std::ops::Range<usize> {
    let epsilon = 1e-6;
    let start = (low + epsilon).floor().max(0.0);
    let end = (high - epsilon).ceil().min(count as f64);
    if end <= start {
        return 0..0;
    }
    start as usize..end as usize
}

/// the samples of a decoded image, packed in rows the way pdf stores them.
pub struct Samples {
    pub width: usize,
    pub height: usize,
    pub components: usize,
    pub bits: usize,
    pub data: Vec<u8>,
}

impl Samples {
    fn row_bytes(&self) -> usize {
        (self.width * self.components * self.bits).div_ceil(8)
    }

    fn max_value(&self) -> u16 {
        ((1u32 << self.bits) - 1) as u16
    }

    /// writes one sample, whatever the bits per sample.
    fn set(&mut self, x: usize, y: usize, component: usize, value: u16) {
        let index = x * self.components + component;
        let row_start = y * self.row_bytes();
        match self.bits {
            8 => self.data[row_start + index] = value as u8,
            16 => {
                let at = row_start + index * 2;
                self.data[at..at + 2].copy_from_slice(&value.to_be_bytes());
            }
            1 | 2 | 4 => {
                let bit = index * self.bits;
                let byte = row_start + bit / 8;
                let shift = 8 - self.bits - (bit % 8);
                let mask = (self.max_value() as u8) << shift;
                self.data[byte] = (self.data[byte] & !mask) | (((value as u8) << shift) & mask);
            }
            _ => {}
        }
    }

    fn paint(&mut self, coverage: &Coverage, values: &[u16]) {
        for y in 0..self.height.min(coverage.height) {
            for x in 0..self.width.min(coverage.width) {
                if coverage.get(x, y) {
                    for (component, value) in values.iter().enumerate().take(self.components) {
                        self.set(x, y, component, *value);
                    }
                }
            }
        }
    }
}

/// what kind of colour an image's samples hold, as far as filling goes.
#[derive(Clone, Debug)]
pub enum Space {
    Gray,
    Rgb,
    Cmyk,
    /// an index into a palette of colours in `base`
    Indexed {
        base: Box<Space>,
        palette: Vec<u8>,
    },
    /// lab, separations, devicen: there is no sure way to write a given
    /// colour, so covered pixels are set to zero. the fill box drawn on
    /// top shows the right colour either way.
    Other(usize),
}

impl Space {
    pub fn components(&self) -> usize {
        match self {
            Space::Gray => 1,
            Space::Rgb => 3,
            Space::Cmyk => 4,
            Space::Indexed { .. } => 1,
            Space::Other(count) => *count,
        }
    }
}

/// reads an image colour space. `named` looks up colour spaces given by
/// name in the page resources (inline images may do that).
pub fn read_space(
    doc: &Document,
    object: &Object,
    named: &dyn Fn(&[u8]) -> Option<Object>,
) -> Option<Space> {
    match resolve(doc, object) {
        Object::Name(name) => match name.as_slice() {
            b"DeviceGray" | b"G" | b"CalGray" => Some(Space::Gray),
            b"DeviceRGB" | b"RGB" | b"CalRGB" => Some(Space::Rgb),
            b"DeviceCMYK" | b"CMYK" => Some(Space::Cmyk),
            other => read_space(doc, &named(other)?, &|_| None),
        },
        Object::Array(items) => read_space_array(doc, items, named),
        _ => None,
    }
}

fn read_space_array(
    doc: &Document,
    items: &[Object],
    named: &dyn Fn(&[u8]) -> Option<Object>,
) -> Option<Space> {
    let family = resolve(doc, items.first()?).as_name().ok()?;
    match family {
        b"CalGray" | b"DeviceGray" | b"G" => Some(Space::Gray),
        b"CalRGB" | b"DeviceRGB" | b"RGB" => Some(Space::Rgb),
        b"DeviceCMYK" | b"CMYK" => Some(Space::Cmyk),
        b"Lab" => Some(Space::Other(3)),
        b"ICCBased" => {
            let stream = resolve(doc, items.get(1)?).as_stream().ok()?;
            match stream.dict.get(b"N").ok().and_then(|n| number(doc, n))? as usize {
                1 => Some(Space::Gray),
                3 => Some(Space::Rgb),
                4 => Some(Space::Cmyk),
                count => Some(Space::Other(count)),
            }
        }
        b"Indexed" | b"I" => {
            let base = read_space(doc, items.get(1)?, named)?;
            let palette = match resolve(doc, items.get(3)?) {
                Object::String(bytes, _) => bytes.clone(),
                Object::Stream(stream) => crate::objects::decoded_content(stream).ok()?,
                _ => return None,
            };
            Some(Space::Indexed {
                base: Box::new(base),
                palette,
            })
        }
        b"Separation" => Some(Space::Other(1)),
        b"DeviceN" => Some(Space::Other(
            resolve(doc, items.get(1)?).as_array().ok()?.len(),
        )),
        _ => None,
    }
}

/// the fill colour as colour values from 0 to 1 in a colour space.
fn fill_in_space(space: &Space, fill: [f64; 3]) -> Vec<f64> {
    let [r, g, b] = fill;
    match space {
        Space::Gray => vec![0.299 * r + 0.587 * g + 0.114 * b],
        Space::Rgb => vec![r, g, b],
        Space::Cmyk => {
            let k = 1.0 - r.max(g).max(b);
            if k >= 1.0 {
                vec![0.0, 0.0, 0.0, 1.0]
            } else {
                let scale = 1.0 - k;
                vec![
                    (1.0 - r - k) / scale,
                    (1.0 - g - k) / scale,
                    (1.0 - b - k) / scale,
                    k,
                ]
            }
        }
        Space::Indexed { .. } | Space::Other(_) => vec![0.0; space.components()],
    }
}

/// the raw sample values that draw the fill colour, taking the image's
/// /Decode array into account.
fn fill_samples(space: &Space, fill: [f64; 3], bits: usize, decode: &[f64]) -> Vec<u16> {
    let max = ((1u32 << bits) - 1) as f64;
    if let Space::Indexed { base, palette } = space {
        return vec![nearest_palette_entry(base, palette, fill, max as usize) as u16];
    }
    fill_in_space(space, fill)
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let (low, high) = match decode.get(index * 2..index * 2 + 2) {
                Some([low, high]) => (*low, *high),
                _ => (0.0, 1.0),
            };
            let raw = if (high - low).abs() < 1e-12 {
                0.0
            } else {
                (value - low) / (high - low) * max
            };
            raw.round().clamp(0.0, max) as u16
        })
        .collect()
}

/// the palette entry closest to the fill colour.
fn nearest_palette_entry(base: &Space, palette: &[u8], fill: [f64; 3], highest: usize) -> usize {
    let target: Vec<f64> = fill_in_space(base, fill)
        .iter()
        .map(|v| v * 255.0)
        .collect();
    let size = base.components().max(1);
    let mut best = (0, f64::INFINITY);
    for (index, entry) in palette.chunks(size).enumerate().take(highest + 1) {
        let distance: f64 = entry
            .iter()
            .zip(&target)
            .map(|(have, want)| (f64::from(*have) - want).powi(2))
            .sum();
        if distance < best.1 {
            best = (index, distance);
        }
    }
    best.0
}

/// an image ready to be edited.
pub struct DecodedImage {
    pub samples: Samples,
    /// true when the data was jpeg, so the samples are 8 bit and the
    /// image's /Decode array does not apply the usual way
    pub was_jpeg: bool,
}

/// undoes an image's filters and returns its samples, or explains why
/// that is not possible here.
pub fn decode_image(
    doc: &Document,
    dict: &Dictionary,
    data: &[u8],
    components: usize,
) -> Result<DecodedImage, String> {
    let width = dict_usize(doc, dict, b"Width").ok_or("the image has no width")?;
    let height = dict_usize(doc, dict, b"Height").ok_or("the image has no height")?;
    if width == 0 || height == 0 || width.saturating_mul(height) > 100_000_000 {
        return Err("the image size is not usable".into());
    }
    let is_mask = matches!(
        dict_get(doc, dict, b"ImageMask"),
        Some(Object::Boolean(true))
    );
    let mut bits = if is_mask {
        1
    } else {
        dict_usize(doc, dict, b"BitsPerComponent").unwrap_or(8)
    };
    let (filters, params) = filter_list(doc, dict);
    let mut bytes = data.to_vec();
    let mut was_jpeg = false;
    let mut components = components;
    for (index, filter) in filters.iter().enumerate() {
        match filter.as_slice() {
            b"DCTDecode" | b"DCT" => {
                let (pixels, jpeg_components, jpeg_bits) = decode_jpeg(&bytes)?;
                bytes = pixels;
                components = jpeg_components;
                bits = jpeg_bits;
                was_jpeg = true;
            }
            b"JPXDecode" => return Err("it is stored as JPEG 2000".into()),
            b"JBIG2Decode" => return Err("it is stored as JBIG2".into()),
            b"CCITTFaxDecode" | b"CCF" => return Err("it is stored as CCITT fax".into()),
            other => bytes = undo_filter(other, params.get(index).cloned().flatten(), &bytes)?,
        }
    }
    if ![1, 2, 4, 8, 16].contains(&bits) {
        return Err("its bits per sample are not usable".into());
    }
    let mut samples = Samples {
        width,
        height,
        components,
        bits,
        data: bytes,
    };
    // a short image is drawn as if padded, so pad it the same way
    let needed = samples.row_bytes() * height;
    samples.data.resize(needed, 0);
    Ok(DecodedImage { samples, was_jpeg })
}

fn dict_usize(doc: &Document, dict: &Dictionary, key: &[u8]) -> Option<usize> {
    let value = dict.get(key).ok().and_then(|o| number(doc, o))?;
    (value >= 0.0).then_some(value as usize)
}

/// the image's filters with the parameters of each.
fn filter_list(doc: &Document, dict: &Dictionary) -> (Vec<Vec<u8>>, Vec<Option<Dictionary>>) {
    let filters: Vec<Vec<u8>> = match dict_get(doc, dict, b"Filter") {
        Some(Object::Name(name)) => vec![name.clone()],
        Some(Object::Array(items)) => items
            .iter()
            .filter_map(|item| resolve(doc, item).as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    };
    let params = match dict_get(doc, dict, b"DecodeParms") {
        Some(Object::Dictionary(params)) => vec![Some(params.clone())],
        Some(Object::Array(items)) => items
            .iter()
            .map(|item| as_dict(resolve(doc, item)).cloned())
            .collect(),
        _ => Vec::new(),
    };
    (filters, params)
}

/// undoes one general purpose filter with lopdf's decoders, which also
/// handle png and tiff predictors.
fn undo_filter(filter: &[u8], params: Option<Dictionary>, data: &[u8]) -> Result<Vec<u8>, String> {
    let full_name: &[u8] = match filter {
        b"Fl" => b"FlateDecode",
        b"LZW" => b"LZWDecode",
        b"AHx" => b"ASCIIHexDecode",
        b"A85" => b"ASCII85Decode",
        b"RL" => b"RunLengthDecode",
        other => other,
    };
    let mut dict = Dictionary::new();
    dict.set("Filter", Object::Name(full_name.to_vec()));
    if let Some(params) = params {
        dict.set("DecodeParms", Object::Dictionary(params));
    }
    Stream::new(dict, data.to_vec())
        .decompressed_content_with_limit(MAX_DECODED_BYTES)
        .map_err(|error| format!("its data can't be read ({error})"))
}

/// decodes a jpeg into samples. cmyk jpegs are refused: they are often
/// stored with inverted values that only some readers undo, and getting
/// that wrong would change how the rest of the image looks.
fn decode_jpeg(bytes: &[u8]) -> Result<(Vec<u8>, usize, usize), String> {
    let mut decoder = jpeg_decoder::Decoder::new(bytes);
    let pixels = decoder
        .decode()
        .map_err(|error| format!("its JPEG data can't be read ({error})"))?;
    let info = decoder.info().ok_or("its JPEG data can't be read")?;
    match info.pixel_format {
        jpeg_decoder::PixelFormat::L8 => Ok((pixels, 1, 8)),
        jpeg_decoder::PixelFormat::L16 => Ok((pixels, 1, 16)),
        jpeg_decoder::PixelFormat::RGB24 => Ok((pixels, 3, 8)),
        jpeg_decoder::PixelFormat::CMYK32 => Err("it is a CMYK JPEG".into()),
    }
}

/// what painting the covered pixels should write.
pub enum FillWith {
    /// the fill colour in the image's colour space
    Colour { space: Space, fill: [f64; 3] },
    /// for stencil masks: the value that paints nothing
    NothingPainted,
    /// for soft masks: fully opaque, so the fill colour shows
    Opaque,
    /// for /Mask stencils of an image: the value that lets the image show
    Shown,
}

/// replaces the covered pixels. `decode` is the image's /Decode array.
pub fn paint_covered(
    image: &mut DecodedImage,
    coverage: &Coverage,
    with: &FillWith,
    decode: &[f64],
) {
    let samples = &mut image.samples;
    let max = samples.max_value();
    let inverted = decode.len() >= 2 && decode[0] > decode[1];
    let values = match with {
        FillWith::Colour { space, fill } => fill_samples(space, *fill, samples.bits, decode),
        // with the usual decode [0 1] a stencil sample of 1 paints nothing
        FillWith::NothingPainted => vec![if inverted { 0 } else { max }],
        FillWith::Opaque => vec![if inverted { 0 } else { max }; samples.components.max(1)],
        FillWith::Shown => vec![if inverted { max } else { 0 }],
    };
    samples.paint(coverage, &values);
}

/// the numbers of a /Decode array.
pub fn decode_array(doc: &Document, dict: &Dictionary) -> Vec<f64> {
    match dict_get(doc, dict, b"Decode") {
        Some(Object::Array(items)) => numbers(doc, items),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_pixels_follow_the_image_placement() {
        // a 10 by 10 image drawn as a 100 point square at (100, 100)
        let ctm = Matrix::new(100.0, 0.0, 0.0, 100.0, 100.0, 100.0);
        // the top left quarter of the image on the page
        let area = Rect::from_corners(100.0, 150.0, 150.0, 200.0);
        let coverage = coverage(10, 10, &ctm, &[area]).unwrap();
        assert_eq!(coverage.covered, 25);
        assert!(coverage.get(0, 0));
        assert!(coverage.get(4, 4));
        assert!(!coverage.get(5, 0));
        assert!(!coverage.get(0, 5));
    }

    #[test]
    fn a_turned_image_marks_the_right_pixels() {
        // turned a quarter: image x runs up the page, image y runs left
        let ctm = Matrix::new(0.0, 100.0, -100.0, 0.0, 200.0, 100.0);
        let area = Rect::from_corners(100.0, 100.0, 110.0, 110.0);
        let coverage = coverage(10, 10, &ctm, &[area]).unwrap();
        assert_eq!(coverage.covered, 1);
    }

    #[test]
    fn samples_can_be_set_at_any_bit_depth() {
        for bits in [1usize, 2, 4, 8, 16] {
            let blank = vec![0; (3 * bits).div_ceil(8) * 2];
            let mut samples = Samples {
                width: 3,
                height: 2,
                components: 1,
                bits,
                data: blank.clone(),
            };
            let max = samples.max_value();
            samples.set(1, 1, 0, max);
            assert_ne!(samples.data, blank, "bits {bits}");
            samples.set(1, 1, 0, 0);
            assert_eq!(samples.data, blank, "bits {bits}");
        }
    }

    #[test]
    fn the_fill_colour_respects_decode_arrays() {
        assert_eq!(fill_samples(&Space::Gray, [0.0; 3], 8, &[]), vec![0]);
        assert_eq!(
            fill_samples(&Space::Gray, [0.0; 3], 8, &[1.0, 0.0]),
            vec![255]
        );
        assert_eq!(
            fill_samples(&Space::Rgb, [1.0, 0.0, 0.0], 8, &[]),
            vec![255, 0, 0]
        );
        assert_eq!(
            fill_samples(&Space::Cmyk, [0.0; 3], 8, &[]),
            vec![0, 0, 0, 255]
        );
    }

    #[test]
    fn indexed_images_use_the_nearest_palette_colour() {
        let space = Space::Indexed {
            base: Box::new(Space::Rgb),
            palette: vec![255, 255, 255, 10, 10, 10, 255, 0, 0],
        };
        assert_eq!(fill_samples(&space, [0.0; 3], 8, &[]), vec![1]);
    }
}
