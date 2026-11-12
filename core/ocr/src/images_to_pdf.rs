//! making a pdf from pictures: scans, phone photos, screenshots.
//!
//! each picture becomes one page. a jpeg is stored exactly as it came (pdf
//! can show jpeg data directly), so a photo loses nothing and the file is
//! no bigger than the photos. other pictures are stored losslessly,
//! compressed with deflate. optionally each page is read as it is made,
//! and the words added as an invisible text layer.

use std::path::PathBuf;

use image::DynamicImage;
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::cancel::{CancelFlag, Progress};
use crate::engine::Engine;
use crate::geometry::{Point, Rect};
use crate::metadata::{jpeg_layout, read_dpi, JpegLayout};
use crate::page_image::{decode_upright, PageImage};
use crate::recognize::{recognize, OcrOptions};
use crate::result::{Baseline, OcrPage};
use crate::text_layer::{add_text_layer, compressed_stream};
use crate::OcrError;

/// where a picture comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum ImageSource {
    File(PathBuf),
    Bytes(Vec<u8>),
}

/// how big each page is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PaperSize {
    /// exactly the picture's size, from the resolution stored in the file
    /// (or `assumed_dpi` when it has none). right for scans.
    FromImage,
    /// a4 paper, the picture shrunk or grown to fit inside the margins and
    /// centred. the page turns sideways for a picture wider than tall.
    A4,
    /// us letter paper, the same way
    Letter,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImagesToPdfOptions {
    pub paper: PaperSize,
    /// the resolution a picture is taken to have when its file doesn't say
    pub assumed_dpi: f32,
    /// space left around the picture on a4 and letter pages, in points
    pub margin: f32,
}

impl Default for ImagesToPdfOptions {
    fn default() -> Self {
        Self {
            paper: PaperSize::FromImage,
            assumed_dpi: 300.0,
            margin: 0.0,
        }
    }
}

const A4_POINTS: (f32, f32) = (595.28, 841.89);
const LETTER_POINTS: (f32, f32) = (612.0, 792.0);

/// builds a pdf with one page per picture, in the order given.
///
/// pass `read_text` to also recognise each page and add its invisible
/// text layer. pictures are loaded one at a time, so memory holds a single
/// picture however many there are. `progress` hears after every page and
/// `cancel` is checked between pages.
pub fn images_to_pdf(
    images: &[ImageSource],
    options: &ImagesToPdfOptions,
    read_text: Option<(&Engine, &OcrOptions)>,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<Document, OcrError> {
    if images.is_empty() {
        return Err(OcrError::NoImages);
    }
    if let Some((engine, ocr_options)) = read_text {
        engine.check_languages(&ocr_options.languages)?;
    }
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);
    let mut kids = Vec::new();

    for (index, source) in images.iter().enumerate() {
        if cancel.is_cancelled() {
            return Err(OcrError::Cancelled);
        }
        let bytes = match source {
            ImageSource::File(path) => std::fs::read(path)
                .map_err(|error| OcrError::UnreadableImage(format!("({error})")))?,
            ImageSource::Bytes(bytes) => bytes.clone(),
        };
        let (page_id, placement, upright) = add_page(&mut doc, pages_id, &bytes, options)?;
        kids.push(page_id);
        write_page_tree(&mut doc, pages_id, &kids);

        if let Some((engine, ocr_options)) = read_text {
            let page_image = upright_page_image(&bytes, upright, &placement)?;
            let words = recognize(engine, &page_image, ocr_options, cancel)?;
            let on_page = placed_on_page(&words, &placement);
            add_text_layer(&mut doc, index as u32, &on_page)?;
        }
        progress(Progress {
            page: index,
            done: index + 1,
            total: images.len(),
        });
    }
    Ok(doc)
}

/// where a picture sits on its page, in points from the page's top left.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    page_width: f32,
    page_height: f32,
    picture: Rect,
}

/// the decoded picture, kept only when the page will be read.
type Upright = Option<DynamicImage>;

fn add_page(
    doc: &mut Document,
    pages_id: ObjectId,
    bytes: &[u8],
    options: &ImagesToPdfOptions,
) -> Result<(ObjectId, Placement, Upright), OcrError> {
    let dpi = read_dpi(bytes);
    let dpi = (
        dpi.map_or(options.assumed_dpi, |dpi| dpi.x),
        dpi.map_or(options.assumed_dpi, |dpi| dpi.y),
    );
    let (image, upright_size, turns, decoded) = match jpeg_layout(bytes) {
        Some(layout) if matches!(layout.components, 1 | 3 | 4) => {
            let turns = quarter_turns(layout.exif_orientation);
            let size = if turns % 2 == 1 {
                (layout.height, layout.width)
            } else {
                (layout.width, layout.height)
            };
            (jpeg_image(bytes, &layout), size, turns, None)
        }
        _ => {
            let picture = decode_upright(bytes)?;
            let size = (picture.width(), picture.height());
            (lossless_image(doc, &picture), size, 0, Some(picture))
        }
    };
    let placement = place(upright_size, dpi, options);
    let image_id = doc.add_object(image);
    let content = draw_picture(&placement, turns);
    let content_id = doc.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![
            0.into(), 0.into(),
            placement.page_width.into(), placement.page_height.into(),
        ],
        "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
        "Contents" => content_id,
    });
    Ok((page_id, placement, decoded))
}

fn write_page_tree(doc: &mut Document, pages_id: ObjectId, kids: &[ObjectId]) {
    let kids_list: Vec<Object> = kids.iter().map(|&id| Object::Reference(id)).collect();
    doc.objects.insert(
        pages_id,
        dictionary! {
            "Type" => "Pages",
            "Kids" => kids_list,
            "Count" => kids.len() as i64,
        }
        .into(),
    );
}

/// the clockwise quarter turns an exif orientation asks for. the mirrored
/// orientations (2, 4, 5, 7) come from selfie cameras and scanners almost
/// never; they are shown unmirrored with the turn they include.
fn quarter_turns(exif_orientation: u8) -> u32 {
    match exif_orientation {
        3 | 4 => 2,
        5 | 6 => 1,
        7 | 8 => 3,
        _ => 0,
    }
}

/// a jpeg stored as it is: pdf's DCTDecode filter is jpeg.
fn jpeg_image(bytes: &[u8], layout: &JpegLayout) -> Stream {
    let colour_space = match layout.components {
        1 => "DeviceGray",
        4 => "DeviceCMYK",
        _ => "DeviceRGB",
    };
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => i64::from(layout.width),
        "Height" => i64::from(layout.height),
        "ColorSpace" => colour_space,
        "BitsPerComponent" => 8,
        "Filter" => "DCTDecode",
    };
    if layout.adobe_inverted_cmyk {
        let flipped: Vec<Object> = (0..4).flat_map(|_| [1.into(), 0.into()]).collect();
        dict.set("Decode", flipped);
    }
    Stream::new(dict, bytes.to_vec()).with_compression(false)
}

/// any other picture, as raw samples compressed losslessly. see-through
/// pixels keep their transparency as a soft mask.
fn lossless_image(doc: &mut Document, picture: &DynamicImage) -> Stream {
    let colour = picture.color();
    let gray = !colour.has_color();
    let (width, height) = (i64::from(picture.width()), i64::from(picture.height()));
    let samples = if gray {
        picture.to_luma8().into_raw()
    } else {
        picture.to_rgb8().into_raw()
    };
    let mut dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => width,
        "Height" => height,
        "ColorSpace" => if gray { "DeviceGray" } else { "DeviceRGB" },
        "BitsPerComponent" => 8,
    };
    if colour.has_alpha() {
        let alpha: Vec<u8> = picture
            .to_rgba8()
            .pixels()
            .map(|pixel| pixel.0[3])
            .collect();
        // a fully opaque picture needs no mask
        if alpha.iter().any(|&value| value < 255) {
            let mask = compressed_stream(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => width,
                    "Height" => height,
                    "ColorSpace" => "DeviceGray",
                    "BitsPerComponent" => 8,
                },
                &alpha,
            );
            let mask_id = doc.add_object(mask);
            dict.set("SMask", mask_id);
        }
    }
    compressed_stream(dict, &samples)
}

/// works out the page size and where the picture goes on it.
fn place(upright_size: (u32, u32), dpi: (f32, f32), options: &ImagesToPdfOptions) -> Placement {
    let natural = (
        upright_size.0 as f32 * 72.0 / dpi.0,
        upright_size.1 as f32 * 72.0 / dpi.1,
    );
    let paper = match options.paper {
        PaperSize::FromImage => {
            return Placement {
                page_width: natural.0,
                page_height: natural.1,
                picture: Rect::new(0.0, 0.0, natural.0, natural.1),
            };
        }
        PaperSize::A4 => A4_POINTS,
        PaperSize::Letter => LETTER_POINTS,
    };
    // turn the paper to match the picture
    let (page_width, page_height) = if natural.0 > natural.1 {
        (paper.1, paper.0)
    } else {
        paper
    };
    let margin = options.margin.clamp(0.0, page_width.min(page_height) / 4.0);
    let room = (page_width - 2.0 * margin, page_height - 2.0 * margin);
    let scale = (room.0 / natural.0).min(room.1 / natural.1);
    let (width, height) = (natural.0 * scale, natural.1 * scale);
    Placement {
        page_width,
        page_height,
        picture: Rect::new(
            (page_width - width) / 2.0,
            (page_height - height) / 2.0,
            width,
            height,
        ),
    }
}

/// the drawing instructions that put the picture in its place.
///
/// a pdf picture fills the square from (0, 0) to (1, 1); the matrix
/// stretches that square over the picture's spot. for a photo the camera
/// stored sideways, the matrix also turns it, so the page shows it upright.
fn draw_picture(placement: &Placement, turns: u32) -> String {
    let Rect {
        x,
        width: w,
        height: h,
        ..
    } = placement.picture;
    // pdf measures from the bottom, the placement from the top
    let y = placement.page_height - placement.picture.bottom();
    let [a, b, c, d, e, f] = match turns {
        // stored top left ends up at the shown top right
        1 => [0.0, -h, w, 0.0, x, y + h],
        2 => [-w, 0.0, 0.0, -h, x + w, y + h],
        // stored top left ends up at the shown bottom left
        3 => [0.0, h, -w, 0.0, x + w, y],
        _ => [w, 0.0, 0.0, h, x, y],
    };
    format!("q {a} {b} {c} {d} {e} {f} cm /Im0 Do Q")
}

/// the picture as recognition needs it: upright gray pixels, with the
/// resolution it has on the page.
fn upright_page_image(
    bytes: &[u8],
    decoded: Upright,
    placement: &Placement,
) -> Result<PageImage, OcrError> {
    let picture = match decoded {
        Some(picture) => picture,
        None => decode_upright(bytes)?,
    };
    let dpi = picture.width() as f32 * 72.0 / placement.picture.width;
    Ok(PageImage::from_gray(picture.into_luma8(), dpi))
}

/// moves words found in the picture to where the picture is on the page.
fn placed_on_page(words: &OcrPage, placement: &Placement) -> OcrPage {
    let (dx, dy) = (placement.picture.x, placement.picture.y);
    let shift = |point: Point| Point::new(point.x + dx, point.y + dy);
    let mut moved = words.clone();
    moved.width = placement.page_width;
    moved.height = placement.page_height;
    for word in &mut moved.words {
        word.rect.x += dx;
        word.rect.y += dy;
        word.baseline = Baseline {
            start: shift(word.baseline.start),
            end: shift(word.baseline.end),
        };
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32, dpi: Option<u32>) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        if let Some(dpi) = dpi {
            let per_metre = (dpi as f32 / 0.0254).round() as u32;
            encoder.set_pixel_dims(Some(png::PixelDimensions {
                xppu: per_metre,
                yppu: per_metre,
                unit: png::Unit::Meter,
            }));
        }
        let samples = vec![200u8; (width * height) as usize];
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&samples)
            .unwrap();
        bytes
    }

    /// a tiny but real jpeg: 16 by 8 pixels, gray.
    fn jpeg(width: u32, height: u32) -> Vec<u8> {
        let picture = image::GrayImage::from_pixel(width, height, image::Luma([90]));
        let mut bytes = Vec::new();
        DynamicImage::ImageLuma8(picture)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        bytes
    }

    fn build(images: Vec<Vec<u8>>, options: ImagesToPdfOptions) -> Document {
        let sources: Vec<ImageSource> = images.into_iter().map(ImageSource::Bytes).collect();
        images_to_pdf(&sources, &options, None, &mut |_| {}, &CancelFlag::new()).unwrap()
    }

    fn media_box(doc: &Document, page: u32) -> Vec<f32> {
        let id = crate::pdf_page::page_id(doc, page).unwrap();
        doc.get_dictionary(id)
            .unwrap()
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_float().unwrap())
            .collect()
    }

    fn image_stream(doc: &Document, page: u32) -> Stream {
        let id = crate::pdf_page::page_id(doc, page).unwrap();
        let resources = doc
            .get_dictionary(id)
            .unwrap()
            .get(b"Resources")
            .unwrap()
            .as_dict()
            .unwrap();
        let image_id = resources
            .get(b"XObject")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Im0")
            .unwrap()
            .as_reference()
            .unwrap();
        doc.get_object(image_id)
            .unwrap()
            .as_stream()
            .unwrap()
            .clone()
    }

    #[test]
    fn page_size_follows_the_stored_resolution() {
        // 600 by 300 pixels at 300 dpi is 2 by 1 inches
        let doc = build(
            vec![png(600, 300, Some(300))],
            ImagesToPdfOptions::default(),
        );
        let media = media_box(&doc, 0);
        assert!(
            (media[2] - 144.0).abs() < 0.1 && (media[3] - 72.0).abs() < 0.1,
            "{media:?}"
        );
    }

    #[test]
    fn a_jpeg_is_kept_byte_for_byte() {
        let photo = jpeg(16, 8);
        let doc = build(vec![photo.clone()], ImagesToPdfOptions::default());
        let image = image_stream(&doc, 0);
        assert_eq!(image.content, photo);
        assert_eq!(
            image.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"DCTDecode"
        );
        assert_eq!(
            image.dict.get(b"ColorSpace").unwrap().as_name().unwrap(),
            b"DeviceGray"
        );
    }

    #[test]
    fn a_png_is_stored_losslessly() {
        let doc = build(vec![png(4, 4, None)], ImagesToPdfOptions::default());
        let image = image_stream(&doc, 0);
        assert_eq!(
            image.dict.get(b"Filter").unwrap().as_name().unwrap(),
            b"FlateDecode"
        );
        assert_eq!(image.decompressed_content().unwrap(), vec![200u8; 16]);
    }

    #[test]
    fn a_wide_picture_gets_a_sideways_a4_page_and_fits_inside_the_margin() {
        let options = ImagesToPdfOptions {
            paper: PaperSize::A4,
            margin: 36.0,
            ..ImagesToPdfOptions::default()
        };
        let placement = place((3000, 1000), (300.0, 300.0), &options);
        assert!(placement.page_width > placement.page_height);
        assert!(
            placement.picture.x >= 35.9 && placement.picture.right() <= placement.page_width - 35.9
        );
        assert!((placement.picture.width / placement.picture.height - 3.0).abs() < 1e-3);
    }

    #[test]
    fn every_picture_becomes_a_page_in_order() {
        let doc = build(
            vec![png(10, 10, None), jpeg(8, 8), png(20, 10, None)],
            ImagesToPdfOptions::default(),
        );
        assert_eq!(doc.get_pages().len(), 3);
        assert!(media_box(&doc, 2)[2] > media_box(&doc, 0)[2]);
    }

    #[test]
    fn no_pictures_is_an_error() {
        let result = images_to_pdf(
            &[],
            &ImagesToPdfOptions::default(),
            None,
            &mut |_| {},
            &CancelFlag::new(),
        );
        assert_eq!(result.unwrap_err(), OcrError::NoImages);
    }

    #[test]
    fn a_sideways_photo_is_turned_by_the_matrix() {
        let placement = Placement {
            page_width: 100.0,
            page_height: 200.0,
            picture: Rect::new(0.0, 0.0, 100.0, 200.0),
        };
        assert_eq!(
            draw_picture(&placement, 0),
            "q 100 0 0 200 0 0 cm /Im0 Do Q"
        );
        assert_eq!(
            draw_picture(&placement, 1),
            "q 0 -200 100 0 0 200 cm /Im0 Do Q"
        );
    }
}
