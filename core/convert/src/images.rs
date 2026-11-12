//! pdf pages to pictures: png, jpeg or tiff, at a chosen resolution.
//!
//! one page is drawn, encoded and written before the next is drawn, so
//! memory holds a single page however long the document is. every file
//! records its resolution, so other programs print it at the right size.

use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use image::codecs::jpeg::{JpegEncoder, PixelDensity};
use image::ExtendedColorType;
use letters_ocr::{CancelFlag, Progress};
use serde::{Deserialize, Serialize};
use tiff::encoder::{colortype, Rational, TiffEncoder};
use tiff::tags::ResolutionUnit;

use crate::error::write_failed;
use crate::naming::{create_unique_file, page_file_name, safe_base_name};
use crate::render::{check_size, render_at_dpi, PageRenderer, RgbaImage};
use crate::source::{PageSelection, PdfSource};
use crate::ConvertError;

/// the picture format, with the settings that belong to it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ImageFormat {
    Png,
    /// `quality` from 1 (smallest) to 100 (best)
    Jpeg {
        quality: u8,
    },
    Tiff {
        compression: TiffCompression,
        /// all pages in one file instead of one file per page
        #[serde(rename = "multiPage")]
        multi_page: bool,
    },
}

impl ImageFormat {
    fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg { .. } => "jpg",
            ImageFormat::Tiff { .. } => "tiff",
        }
    }
}

/// how a tiff's pixels are packed. both kinds lose nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TiffCompression {
    #[default]
    Lzw,
    Deflate,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ColorMode {
    #[default]
    Color,
    Grayscale,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageExportOptions {
    pub format: ImageFormat,
    /// dots per inch; 150 suits screens, 300 printing
    pub dpi: f32,
    pub pages: PageSelection,
    pub color: ColorMode,
}

impl Default for ImageExportOptions {
    fn default() -> Self {
        Self {
            format: ImageFormat::Png,
            dpi: 150.0,
            pages: PageSelection::All,
            color: ColorMode::Color,
        }
    }
}

/// draws the chosen pages and saves them as pictures in `folder`, named
/// after `base_name` ("Essay - page 1.png", or "Essay.tiff" for a tiff
/// with every page). files already in the folder are never replaced: a
/// taken name gets " (2)". returns the files made, in page order.
///
/// if anything fails or the conversion is cancelled, the files made so
/// far are deleted again, so a stopped export leaves nothing half done.
pub fn pdf_to_images(
    source: &PdfSource,
    renderer: &mut dyn PageRenderer,
    options: &ImageExportOptions,
    folder: &Path,
    base_name: &str,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<Vec<PathBuf>, ConvertError> {
    check_options(options)?;
    let pages = options.pages.resolve(source.page_count())?;
    for page in &pages {
        let space = source.page_space(*page)?;
        check_size(space.width, space.height, options.dpi)?;
    }
    let mut made = Vec::new();
    let result = export_pages(
        source, renderer, options, &pages, folder, base_name, &mut made, progress, cancel,
    );
    if result.is_err() {
        for file in &made {
            let _ = std::fs::remove_file(file);
        }
    }
    result.map(|_| made)
}

fn check_options(options: &ImageExportOptions) -> Result<(), ConvertError> {
    if !(options.dpi.is_finite() && (10.0..=2400.0).contains(&options.dpi)) {
        return Err(ConvertError::InvalidSetting(
            "Choose a resolution between 10 and 2400 dpi.".into(),
        ));
    }
    if let ImageFormat::Jpeg { quality } = options.format {
        if !(1..=100).contains(&quality) {
            return Err(ConvertError::InvalidSetting(
                "Choose a JPEG quality between 1 and 100.".into(),
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn export_pages(
    source: &PdfSource,
    renderer: &mut dyn PageRenderer,
    options: &ImageExportOptions,
    pages: &[usize],
    folder: &Path,
    base_name: &str,
    made: &mut Vec<PathBuf>,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<(), ConvertError> {
    let base = safe_base_name(base_name);
    let last_page = source.page_count() - 1;
    let mut draw = |page: usize| -> Result<RgbaImage, ConvertError> {
        if cancel.is_cancelled() {
            return Err(ConvertError::Cancelled);
        }
        let space = source.page_space(page)?;
        render_at_dpi(
            renderer,
            source.path(),
            page,
            (space.width, space.height),
            options.dpi,
        )
    };
    let mut report = |index: usize, page: usize| {
        progress(Progress {
            page,
            done: index + 1,
            total: pages.len(),
        })
    };

    if let ImageFormat::Tiff {
        compression,
        multi_page: true,
    } = options.format
    {
        let (file, path) = create_unique_file(folder, &format!("{base}.tiff"))?;
        made.push(path);
        let mut tiff = TiffEncoder::new(BufWriter::new(file))
            .map_err(tiff_failed)?
            .with_compression(tiff_compression(compression));
        for (index, page) in pages.iter().enumerate() {
            let image = draw(*page)?;
            write_tiff_page(&mut tiff, &image, options)?;
            report(index, *page);
        }
        return Ok(());
    }

    for (index, page) in pages.iter().enumerate() {
        let image = draw(*page)?;
        let name = page_file_name(&base, *page, last_page, options.format.extension());
        let (file, path) = create_unique_file(folder, &name)?;
        made.push(path);
        let mut writer = BufWriter::new(file);
        match options.format {
            ImageFormat::Png => write_png(&mut writer, &image, options)?,
            ImageFormat::Jpeg { quality } => write_jpeg(&mut writer, &image, options, quality)?,
            ImageFormat::Tiff { compression, .. } => {
                let mut tiff = TiffEncoder::new(&mut writer)
                    .map_err(tiff_failed)?
                    .with_compression(tiff_compression(compression));
                write_tiff_page(&mut tiff, &image, options)?;
            }
        }
        writer.flush().map_err(write_failed)?;
        report(index, *page);
    }
    Ok(())
}

/// the pixels in the chosen colour mode, and whether they are gray.
fn pixels(image: &RgbaImage, options: &ImageExportOptions) -> (Vec<u8>, bool) {
    match options.color {
        ColorMode::Color => (image.to_rgb(), false),
        ColorMode::Grayscale => (image.to_gray(), true),
    }
}

/// png keeps resolution as pixels per metre in its pHYs chunk.
pub(crate) fn write_png(
    writer: &mut impl Write,
    image: &RgbaImage,
    options: &ImageExportOptions,
) -> Result<(), ConvertError> {
    let (data, gray) = pixels(image, options);
    encode_png(writer, image.width, image.height, &data, gray, options.dpi)
}

pub(crate) fn encode_png(
    writer: &mut impl Write,
    width: u32,
    height: u32,
    data: &[u8],
    gray: bool,
    dpi: f32,
) -> Result<(), ConvertError> {
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(if gray {
        png::ColorType::Grayscale
    } else {
        png::ColorType::Rgb
    });
    encoder.set_depth(png::BitDepth::Eight);
    let per_metre = (dpi / 0.0254).round() as u32;
    encoder.set_pixel_dims(Some(png::PixelDimensions {
        xppu: per_metre,
        yppu: per_metre,
        unit: png::Unit::Meter,
    }));
    let failed = |error: png::EncodingError| ConvertError::WriteFailed(format!("({error})"));
    let mut png = encoder.write_header().map_err(failed)?;
    png.write_image_data(data).map_err(failed)?;
    png.finish().map_err(failed)
}

/// jpeg keeps resolution in its jfif header, in whole dots per inch.
pub(crate) fn write_jpeg(
    writer: &mut impl Write,
    image: &RgbaImage,
    options: &ImageExportOptions,
    quality: u8,
) -> Result<(), ConvertError> {
    let (data, gray) = pixels(image, options);
    encode_jpeg(
        writer,
        image.width,
        image.height,
        &data,
        gray,
        options.dpi,
        quality,
    )
}

pub(crate) fn encode_jpeg(
    writer: &mut impl Write,
    width: u32,
    height: u32,
    data: &[u8],
    gray: bool,
    dpi: f32,
    quality: u8,
) -> Result<(), ConvertError> {
    let mut encoder = JpegEncoder::new_with_quality(writer, quality);
    encoder.set_pixel_density(PixelDensity::dpi(dpi.round().clamp(1.0, 65535.0) as u16));
    let color = if gray {
        ExtendedColorType::L8
    } else {
        ExtendedColorType::Rgb8
    };
    encoder
        .encode(data, width, height, color)
        .map_err(|error| ConvertError::WriteFailed(format!("({error})")))
}

fn tiff_compression(compression: TiffCompression) -> tiff::encoder::Compression {
    match compression {
        TiffCompression::Lzw => tiff::encoder::Compression::Lzw,
        TiffCompression::Deflate => tiff::encoder::Compression::Deflate(Default::default()),
        TiffCompression::None => tiff::encoder::Compression::Uncompressed,
    }
}

/// adds one page to a tiff, with its resolution in dots per inch.
fn write_tiff_page<W: Write + std::io::Seek>(
    tiff: &mut TiffEncoder<W>,
    image: &RgbaImage,
    options: &ImageExportOptions,
) -> Result<(), ConvertError> {
    let (data, gray) = pixels(image, options);
    // a whole number over 100 keeps resolutions like 72.5 dpi exact enough
    let resolution = Rational {
        n: (options.dpi * 100.0).round() as u32,
        d: 100,
    };
    if gray {
        let mut page = tiff
            .new_image::<colortype::Gray8>(image.width, image.height)
            .map_err(tiff_failed)?;
        page.resolution(ResolutionUnit::Inch, resolution);
        page.write_data(&data).map_err(tiff_failed)
    } else {
        let mut page = tiff
            .new_image::<colortype::RGB8>(image.width, image.height)
            .map_err(tiff_failed)?;
        page.resolution(ResolutionUnit::Inch, resolution);
        page.write_data(&data).map_err(tiff_failed)
    }
}

fn tiff_failed(error: tiff::TiffError) -> ConvertError {
    ConvertError::WriteFailed(format!("({error})"))
}
