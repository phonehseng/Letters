//! exports: the deck as pdf (a slide per page, notes pages, or handouts
//! with 2, 3, 4 or 6 slides per page) and a pdf per slide for pictures
//! (the app's pdf renderer turns those into images).

use serde::{Deserialize, Serialize};

use crate::error::SlidesError;
use crate::fonts::FontLibrary;
use crate::inherit::Chain;
use crate::model::{Object, Presentation, Shape, TextBody, EMU_PER_INCH};
use crate::render::scene::{
    multiply, rectangle, scale, translate, Item, Matrix, Paint, Scene, StrokeStyle,
};
use crate::render::{points, slide_scene, RenderContext, RenderReport};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "perPage", rename_all = "camelCase")]
pub enum PdfLayout {
    Slides,
    NotesPages,
    /// 2, 3, 4 or 6 slides on a letter-sized page
    Handouts(u8),
}

/// a letter page in points, the size powerpoint prints handouts on here
const LETTER: (f64, f64) = (612.0, 792.0);
const HALF_INCH: f64 = 36.0;
/// powerpoint's notes text size (its notes master's 12 pt)
const NOTES_TEXT_SIZE: i32 = 1_200;

pub fn export_pdf(
    presentation: &Presentation,
    layout: PdfLayout,
    fonts: &mut FontLibrary,
) -> Result<(Vec<u8>, RenderReport), SlidesError> {
    let mut report = RenderReport::default();
    let slides: Vec<Scene> = (0..presentation.slides.len())
        .map(|index| slide_scene(presentation, index, fonts, &mut report))
        .collect();
    let pages = match layout {
        PdfLayout::Slides => slides,
        PdfLayout::NotesPages => notes_pages(presentation, &slides, fonts, &mut report),
        PdfLayout::Handouts(per_page) => handouts(&slides, per_page)?,
    };
    let bytes = crate::render::pdf::write_pdf(&pages, fonts, &presentation.media)?;
    Ok((bytes, report))
}

/// one pdf per slide, for thumbnails and picture export.
pub fn slide_pdfs(
    presentation: &Presentation,
    fonts: &mut FontLibrary,
) -> Result<Vec<Vec<u8>>, SlidesError> {
    let mut report = RenderReport::default();
    (0..presentation.slides.len())
        .map(|index| {
            let scene = slide_scene(presentation, index, fonts, &mut report);
            crate::render::pdf::write_pdf(&[scene], fonts, &presentation.media)
        })
        .collect()
}

/// a scene's items drawn under `matrix` (onto another page).
fn placed(scene: &Scene, matrix: &Matrix) -> Vec<Item> {
    scene
        .items
        .iter()
        .cloned()
        .map(|mut item| {
            match &mut item {
                Item::Fill { matrix: m, .. }
                | Item::Stroke { matrix: m, .. }
                | Item::Text { matrix: m, .. } => *m = multiply(matrix, m),
                Item::Image {
                    matrix: m, clip, ..
                } => {
                    *m = multiply(matrix, m);
                    if let Some((clip_matrix, _)) = clip {
                        *clip_matrix = multiply(matrix, clip_matrix);
                    }
                }
            }
            item
        })
        .collect()
}

/// the slide fitted into a box, with a thin frame around it.
fn framed(slide: &Scene, left: f64, top: f64, width: f64, height: f64) -> Vec<Item> {
    let factor = (width / slide.width).min(height / slide.height);
    let (w, h) = (slide.width * factor, slide.height * factor);
    let (x, y) = (left + (width - w) / 2.0, top + (height - h) / 2.0);
    let mut items = placed(slide, &multiply(&translate(x, y), &scale(factor, factor)));
    items.push(Item::Stroke {
        matrix: translate(x, y),
        paths: rectangle(0.0, 0.0, w, h),
        style: StrokeStyle {
            color: crate::color::Rgba::BLACK,
            width: 0.75,
            dashes: Vec::new(),
            cap: 0,
            join: 0,
        },
    });
    items
}

fn notes_pages(
    presentation: &Presentation,
    slides: &[Scene],
    fonts: &mut FontLibrary,
    report: &mut RenderReport,
) -> Vec<Scene> {
    let (width, height) = presentation
        .notes_size
        .as_ref()
        .map_or((7.5 * 72.0, 10.0 * 72.0), |size| {
            (points(size.width), points(size.height))
        });
    let image_width = width - 2.0 * (EMU_PER_INCH as f64 * 0.75 / 12_700.0);
    let mut pages = Vec::new();
    for (index, slide) in slides.iter().enumerate() {
        let image_height = image_width * slide.height / slide.width;
        let margin = (width - image_width) / 2.0;
        let mut page = Scene {
            width,
            height,
            items: vec![Item::Fill {
                matrix: translate(0.0, 0.0),
                paths: rectangle(0.0, 0.0, width, height),
                paint: Paint::Solid(crate::color::Rgba::WHITE),
            }],
        };
        page.items.extend(framed(
            slide,
            margin,
            HALF_INCH * 2.0,
            image_width,
            image_height,
        ));
        let text_top = HALF_INCH * 2.0 + image_height + HALF_INCH;
        let rect = [
            margin,
            text_top,
            margin + image_width,
            height - HALF_INCH * 1.5,
        ];
        page.items
            .extend(notes_text(presentation, index, rect, fonts, report));
        pages.push(page);
    }
    pages
}

/// a slide's notes laid out in `rect` (left, top, right, bottom points).
fn notes_text(
    presentation: &Presentation,
    index: usize,
    rect: [f64; 4],
    fonts: &mut FontLibrary,
    report: &mut RenderReport,
) -> Vec<Item> {
    let slide = &presentation.slides[index];
    let Some(notes) = &slide.notes else {
        return Vec::new();
    };
    let mut body: TextBody = notes.text.clone();
    for paragraph in &mut body.paragraphs {
        for run in &mut paragraph.runs {
            run.props_mut().size.get_or_insert(NOTES_TEXT_SIZE);
        }
    }
    let holder = Object::Shape(Shape {
        text: Some(body.clone()),
        ..Default::default()
    });
    let chain = Chain::of_slide(presentation, slide);
    let mut context = RenderContext {
        presentation,
        fonts,
        scheme: chain.theme.map(|t| t.colors.clone()).unwrap_or_default(),
        map: chain
            .master
            .map(|m| m.color_map.clone())
            .unwrap_or_default(),
        slide_number: index + 1,
        report,
    };
    let target = crate::render::text::TextTarget {
        object: &holder,
        body: &body,
        rect,
        matrix: translate(0.0, 0.0),
        color: None,
    };
    let mut items = Vec::new();
    crate::render::text::draw_text(&mut context, &chain, &target, &mut items);
    items
}

fn handouts(slides: &[Scene], per_page: u8) -> Result<Vec<Scene>, SlidesError> {
    let (columns, rows) = match per_page {
        2 => (1, 2),
        3 => (1, 3),
        4 => (2, 2),
        6 => (2, 3),
        other => {
            return Err(SlidesError::InvalidEdit(format!(
                "handouts hold 2, 3, 4 or 6 slides a page, not {other}"
            )))
        }
    };
    let (width, height) = LETTER;
    let gap = HALF_INCH / 2.0;
    let cell_width = (width - 2.0 * HALF_INCH - gap * f64::from(columns - 1)) / f64::from(columns);
    let cell_height = (height - 2.0 * HALF_INCH - gap * f64::from(rows - 1)) / f64::from(rows);
    let mut pages = Vec::new();
    for chunk in slides.chunks(usize::from(per_page)) {
        let mut page = Scene {
            width,
            height,
            items: Vec::new(),
        };
        for (position, slide) in chunk.iter().enumerate() {
            let (column, row) = (position as u32 % columns, position as u32 / columns);
            // three a page sit on the left half, leaving room for writing
            let box_width = if per_page == 3 {
                cell_width / 2.0
            } else {
                cell_width
            };
            let left = HALF_INCH + f64::from(column) * (cell_width + gap);
            let top = HALF_INCH + f64::from(row) * (cell_height + gap);
            page.items
                .extend(framed(slide, left, top, box_width, cell_height));
        }
        pages.push(page);
    }
    Ok(pages)
}
