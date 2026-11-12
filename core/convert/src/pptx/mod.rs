//! pdf to slides (pptx), written by hand as zip and xml.
//!
//! visual fidelity first: one slide per page, the slide the size of the
//! page, and everything placed exactly:
//! - text becomes real text boxes, one per line, with the font family,
//!   size, colour, bold and italic of the pdf. text is never drawn into
//!   a picture.
//! - pictures become pictures at their place
//! - simple shapes (boxes, straight lines, shapes with straight sides,
//!   in plain colours) become office shapes
//! - everything else (curves, gradients, patterns, clipped or see-through
//!   drawing, annotations) is drawn into a background picture, from a
//!   copy of the page with the text and the rebuilt parts taken out, so
//!   the slide always looks like the page and nothing shows twice
//!
//! the report says, slide by slide, what became real objects and what is
//! in the background picture.

mod parts;
mod slide;

use std::path::Path;

use letters_ocr::{CancelFlag, Progress};
use serde::{Deserialize, Serialize};

use crate::content::strip::{remaining_paint, write_background_copy, Removals, TempPdf};
use crate::content::{read_page_drawing, OpRef, PageDrawing, PaintedPath};
use crate::geometry::Rect;
use crate::images::{encode_jpeg, encode_png};
use crate::package::PackageFile;
use crate::render::{render_at_dpi, PageRenderer};
use crate::source::{PageSelection, PdfSource};
use crate::structure::{page_number_map, read_page};
use crate::ConvertError;
use slide::{Place, SlideWriter, EMU_PER_POINT};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PptxExportOptions {
    pub pages: PageSelection,
    /// the resolution of background pictures
    pub background_dpi: f32,
}

impl Default for PptxExportOptions {
    fn default() -> Self {
        Self {
            pages: PageSelection::All,
            background_dpi: 200.0,
        }
    }
}

/// what became of one page.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideReport {
    pub page: usize,
    pub text_boxes: usize,
    pub pictures: usize,
    pub shapes: usize,
    /// whether a background picture holds what couldn't be rebuilt
    pub background: bool,
    /// what is in the background picture, a sentence each
    pub in_background: Vec<String>,
    /// the page has a different size from the slide, so it was shrunk to
    /// fit and centred
    pub scaled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PptxReport {
    /// the slide size, in points
    pub slide_width: f32,
    pub slide_height: f32,
    pub slides: Vec<SlideReport>,
}

/// powerpoint's limits on a slide's sides: 1 to 56 inches
const MIN_SIDE: f32 = 72.0;
const MAX_SIDE: f32 = 56.0 * 72.0;
/// more shapes than this on one page are drawn into the background; a
/// slide with thousands of shapes is slow to open and useless to edit
const MAX_SHAPES: usize = 1500;

/// writes the chosen pages as slides to `path` (replacing it: the save
/// panel already asked).
pub fn pdf_to_pptx(
    source: &PdfSource,
    renderer: &mut dyn PageRenderer,
    options: &PptxExportOptions,
    path: &Path,
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<PptxReport, ConvertError> {
    if !(50.0..=600.0).contains(&options.background_dpi) {
        return Err(ConvertError::InvalidSetting(
            "Choose a background resolution between 50 and 600 dpi.".into(),
        ));
    }
    let pages = options.pages.resolve(source.page_count())?;
    let first = source.page_space(pages[0])?;
    let (slide_width, slide_height) = slide_size(first.width, first.height);

    // first look at every page: what can be rebuilt, and what is left
    // for the background picture
    let mut plans = Vec::new();
    for page in &pages {
        check_cancel(cancel)?;
        plans.push(plan_page(source, *page)?);
    }
    let needing: Vec<_> = plans
        .iter()
        .filter(|plan| plan.needs_background())
        .map(|plan| {
            source
                .page_id(plan.page)
                .map(|id| (id, plan.removals.clone()))
        })
        .collect::<Result<_, _>>()?;
    let background = if needing.is_empty() {
        None
    } else {
        Some(write_background_copy(source.document(), &needing)?)
    };

    let mut package = PackageFile::create(path)?;
    let result = write_slides(
        source,
        renderer,
        options,
        &mut package,
        &plans,
        background.as_ref(),
        (slide_width, slide_height),
        progress,
        cancel,
    );
    match result {
        Ok(slides) => {
            package.finish()?;
            Ok(PptxReport {
                slide_width,
                slide_height,
                slides,
            })
        }
        Err(error) => {
            package.abandon();
            Err(error)
        }
    }
}

fn check_cancel(cancel: &CancelFlag) -> Result<(), ConvertError> {
    if cancel.is_cancelled() {
        Err(ConvertError::Cancelled)
    } else {
        Ok(())
    }
}

/// the first page's size, kept within powerpoint's limits (scaled as a
/// whole, so the shape stays the same).
fn slide_size(width: f32, height: f32) -> (f32, f32) {
    let grow = (MIN_SIDE / width.min(height)).max(1.0);
    let shrink = (MAX_SIDE / width.max(height)).min(1.0);
    let factor = if grow > 1.0 { grow } else { shrink };
    (width * factor, height * factor)
}

/// where a page goes on the slide.
fn place_for(width: f32, height: f32, slide: (f32, f32)) -> Place {
    let scale = (slide.0 / width).min(slide.1 / height);
    Place {
        scale,
        dx: (slide.0 - width * scale) / 2.0,
        dy: (slide.1 - height * scale) / 2.0,
    }
}

/// a picture of the page that can be copied out and placed again.
struct PlannedPicture {
    origin: OpRef,
    rect: Rect,
    image_id: String,
}

struct PagePlan {
    page: usize,
    drawing: PageDrawing,
    /// paths rebuilt as shapes, by their index in the drawing
    shapes: Vec<usize>,
    pictures: Vec<PlannedPicture>,
    removals: Removals,
    annotations: usize,
    /// pictures letters-edit couldn't copy out
    pictures_left: usize,
}

impl PagePlan {
    fn needs_background(&self) -> bool {
        remaining_paint(&self.drawing, &self.removals) > 0 || self.annotations > 0
    }
}

fn plan_page(source: &PdfSource, page: usize) -> Result<PagePlan, ConvertError> {
    let page_id = source.page_id(page)?;
    let drawing = read_page_drawing(source.document(), page_id)?;
    let objects = letters_edit::page_objects(source.document(), page)?;
    let mut removals = Removals::default();

    let mut shapes = Vec::new();
    for (index, path) in drawing.paths.iter().enumerate() {
        if shapes.len() >= MAX_SHAPES {
            break;
        }
        if let (Some(origin), true) = (path.origin, is_simple_shape(path)) {
            shapes.push(index);
            removals.paths.insert(origin);
        }
    }

    // pictures: matched with letters-edit's by place, then copied out
    let mut pictures = Vec::new();
    let mut taken = vec![false; objects.images.len()];
    let mut pictures_left = 0;
    for image in &drawing.images {
        let Some(origin) = image.origin.filter(|_| image.upright && image.plain) else {
            continue;
        };
        let found = objects.images.iter().enumerate().position(|(i, object)| {
            !taken[i] && !object.inline && same_place(&Rect::from_edit(object.rect), &image.rect)
        });
        let Some(index) = found else {
            continue;
        };
        taken[index] = true;
        let object = &objects.images[index];
        // a picture in a format that can't be copied out stays in the background
        if letters_edit::extract_image(source.document(), page, &object.id).is_err() {
            pictures_left += 1;
            continue;
        }
        removals.images.insert(origin);
        pictures.push(PlannedPicture {
            origin,
            rect: image.rect,
            image_id: object.id.clone(),
        });
    }
    let annotations = drawn_annotations(source, page_id);
    Ok(PagePlan {
        page,
        drawing,
        shapes,
        pictures,
        removals,
        annotations,
        pictures_left,
    })
}

fn same_place(a: &Rect, b: &Rect) -> bool {
    (a.x - b.x).abs() < 1.0
        && (a.y - b.y).abs() < 1.0
        && (a.width - b.width).abs() < 1.0
        && (a.height - b.height).abs() < 1.0
}

/// a path an office shape can copy exactly: plain colours, no curves,
/// one outline, drawn on the page itself.
fn is_simple_shape(path: &PaintedPath) -> bool {
    path.plain
        && !path.has_curves
        && path.subpaths.len() == 1
        && path.subpaths[0].points.len() >= 2
        && path.subpaths[0].points.len() <= 200
        && (path.fill.is_some() || path.stroke.is_some())
}

/// annotations other than links are drawn by the renderer, so they end
/// up in the background picture.
fn drawn_annotations(source: &PdfSource, page_id: lopdf::ObjectId) -> usize {
    let doc = source.document();
    let Some(annotations) = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|p| p.get(b"Annots").ok())
        .and_then(|a| crate::source::resolve(doc, a).as_array().ok())
    else {
        return 0;
    };
    annotations
        .iter()
        .filter(|a| {
            let subtype = crate::source::resolve(doc, a)
                .as_dict()
                .ok()
                .and_then(|d| d.get(b"Subtype").ok())
                .and_then(|s| s.as_name().ok());
            subtype != Some(b"Link")
        })
        .count()
}

#[allow(clippy::too_many_arguments)]
fn write_slides(
    source: &PdfSource,
    renderer: &mut dyn PageRenderer,
    options: &PptxExportOptions,
    package: &mut PackageFile,
    plans: &[PagePlan],
    background: Option<&TempPdf>,
    slide_size: (f32, f32),
    progress: &mut dyn FnMut(Progress),
    cancel: &CancelFlag,
) -> Result<Vec<SlideReport>, ConvertError> {
    let count = plans.len();
    let zip = &mut package.zip;
    zip.add(
        "[Content_Types].xml",
        parts::content_types(count).as_bytes(),
    )?;
    zip.add("_rels/.rels", parts::root_relationships().as_bytes())?;
    zip.add(
        "docProps/core.xml",
        parts::core_properties(source.title().as_deref()).as_bytes(),
    )?;
    zip.add("docProps/app.xml", parts::app_properties(count).as_bytes())?;
    let (width_emu, height_emu) = (
        (slide_size.0 * EMU_PER_POINT).round() as i64,
        (slide_size.1 * EMU_PER_POINT).round() as i64,
    );
    zip.add(
        "ppt/presentation.xml",
        parts::presentation(count, width_emu, height_emu).as_bytes(),
    )?;
    zip.add(
        "ppt/_rels/presentation.xml.rels",
        parts::presentation_relationships(count).as_bytes(),
    )?;
    zip.add("ppt/presProps.xml", parts::pres_props().as_bytes())?;
    zip.add("ppt/viewProps.xml", parts::view_props().as_bytes())?;
    zip.add("ppt/tableStyles.xml", parts::table_styles().as_bytes())?;
    zip.add(
        "ppt/slideMasters/slideMaster1.xml",
        parts::slide_master().as_bytes(),
    )?;
    zip.add(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        parts::slide_master_relationships().as_bytes(),
    )?;
    zip.add(
        "ppt/slideLayouts/slideLayout1.xml",
        parts::slide_layout().as_bytes(),
    )?;
    zip.add(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        parts::slide_layout_relationships().as_bytes(),
    )?;
    zip.add("ppt/theme/theme1.xml", parts::theme().as_bytes())?;

    let page_numbers = page_number_map(source);
    let mut reports = Vec::new();
    for (index, plan) in plans.iter().enumerate() {
        check_cancel(cancel)?;
        let number = index + 1;
        let read = read_page(source, plan.page, &page_numbers)?;
        let place = place_for(read.width, read.height, slide_size);
        let mut writer = SlideWriter::new(place);
        let mut report = SlideReport {
            page: plan.page,
            scaled: (place.scale - 1.0).abs() > 0.001
                || place.dx.abs() > 0.5
                || place.dy.abs() > 0.5,
            ..Default::default()
        };
        let page_rect = Rect::new(0.0, 0.0, read.width, read.height);

        if let (Some(copy), true) = (background, plan.needs_background()) {
            let media = background_media(
                renderer,
                copy,
                plan,
                (read.width, read.height),
                options.background_dpi,
                number,
            )?;
            package
                .zip
                .add(&format!("ppt/media/{}", media.0), &media.1)?;
            writer.set_background(&media.0, &page_rect);
            report.background = true;
            report.in_background = background_sentences(plan);
        }

        // shapes and pictures in the order the page paints them
        let mut drawn: Vec<(OpRef, Drawn)> = plan
            .shapes
            .iter()
            .filter_map(|i| plan.drawing.paths[*i].origin.map(|o| (o, Drawn::Shape(*i))))
            .collect();
        drawn.extend(
            plan.pictures
                .iter()
                .enumerate()
                .map(|(i, p)| (p.origin, Drawn::Picture(i))),
        );
        drawn.sort_by_key(|(origin, _)| *origin);
        for (_, item) in drawn {
            match item {
                Drawn::Shape(i) => {
                    add_shape(&mut writer, &plan.drawing.paths[i]);
                    report.shapes += 1;
                }
                Drawn::Picture(i) => {
                    let picture = &plan.pictures[i];
                    let image = letters_edit::extract_image(
                        source.document(),
                        plan.page,
                        &picture.image_id,
                    )?;
                    let extension = if image.format == "jpeg" {
                        "jpeg"
                    } else {
                        "png"
                    };
                    let name = format!("slide{number}-picture{}.{extension}", i + 1);
                    package
                        .zip
                        .add(&format!("ppt/media/{name}"), &image.bytes)?;
                    writer.add_picture(&name, &picture.rect);
                    report.pictures += 1;
                }
            }
        }
        for fragment in &read.fragments {
            if writer.add_text(fragment) {
                report.text_boxes += 1;
            }
        }
        let (slide_xml, relationships) = writer.finish();
        package.zip.add(
            &format!("ppt/slides/slide{number}.xml"),
            slide_xml.as_bytes(),
        )?;
        package.zip.add(
            &format!("ppt/slides/_rels/slide{number}.xml.rels"),
            relationships.as_bytes(),
        )?;
        reports.push(report);
        progress(Progress {
            page: plan.page,
            done: number,
            total: count,
        });
    }
    Ok(reports)
}

enum Drawn {
    Shape(usize),
    Picture(usize),
}

fn add_shape(writer: &mut SlideWriter, path: &PaintedPath) {
    if let Some(rect) = path.as_box() {
        writer.add_box(&rect, path.fill, path.stroke, path.line_width);
        return;
    }
    let subpath = &path.subpaths[0];
    match (subpath.points.as_slice(), path.stroke) {
        ([from, to], Some(color)) if path.fill.is_none() => {
            writer.add_line(*from, *to, color, path.line_width)
        }
        _ => writer.add_polygon(subpath, path.fill, path.stroke, path.line_width),
    }
}

/// draws the page without what was rebuilt, as a png (or a jpeg when
/// photos are left in it, which keeps the file small).
fn background_media(
    renderer: &mut dyn PageRenderer,
    copy: &TempPdf,
    plan: &PagePlan,
    size: (f32, f32),
    dpi: f32,
    number: usize,
) -> Result<(String, Vec<u8>), ConvertError> {
    let image = render_at_dpi(renderer, copy.path(), plan.page, size, dpi)?;
    let rgb = image.to_rgb();
    let mut bytes = Vec::new();
    let photos_left = plan.drawing.images.len() > plan.pictures.len();
    if photos_left {
        encode_jpeg(&mut bytes, image.width, image.height, &rgb, false, dpi, 88)?;
        Ok((format!("slide{number}-background.jpeg"), bytes))
    } else {
        encode_png(&mut bytes, image.width, image.height, &rgb, false, dpi)?;
        Ok((format!("slide{number}-background.png"), bytes))
    }
}

/// says in words what the background picture holds.
fn background_sentences(plan: &PagePlan) -> Vec<String> {
    let mut sentences = Vec::new();
    let paths = plan
        .drawing
        .paths
        .iter()
        .filter(|p| p.origin.is_none_or(|o| !plan.removals.paths.contains(&o)))
        .count();
    let pictures = plan
        .drawing
        .images
        .iter()
        .filter(|i| i.origin.is_none_or(|o| !plan.removals.images.contains(&o)))
        .count();
    let plural = |n: usize, one: &str, many: &str| {
        if n == 1 {
            one.to_string()
        } else {
            format!("{n} {many}")
        }
    };
    if paths > 0 {
        sentences.push(format!(
            "{} (curves, see-through or clipped drawing) {} part of the background picture.",
            plural(paths, "One drawn shape", "drawn shapes"),
            if paths == 1 { "is" } else { "are" }
        ));
    }
    if pictures > 0 {
        sentences.push(format!(
            "{} {} part of the background picture{}.",
            plural(pictures, "One picture", "pictures"),
            if pictures == 1 { "is" } else { "are" },
            if plan.pictures_left > 0 {
                ", because they couldn’t be copied out"
            } else {
                ""
            }
        ));
    }
    if plan.drawing.other_paint > 0 {
        sentences.push(
            "Shading, inline pictures or drawings inside forms are part of the background picture."
                .into(),
        );
    }
    if plan.annotations > 0 {
        sentences
            .push("Comments, stamps and form fields are part of the background picture.".into());
    }
    sentences
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slides_keep_the_page_shape_within_limits() {
        assert_eq!(slide_size(612.0, 792.0), (612.0, 792.0));
        let (w, h) = slide_size(36.0, 72.0);
        assert_eq!((w, h), (72.0, 144.0));
        let (w, _) = slide_size(8000.0, 4000.0);
        assert_eq!(w, MAX_SIDE);
    }

    #[test]
    fn other_page_sizes_are_fitted_and_centred() {
        let place = place_for(612.0, 396.0, (612.0, 792.0));
        assert_eq!(place.scale, 1.0);
        assert_eq!((place.dx, place.dy), (0.0, 198.0));
    }
}
