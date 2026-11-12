//! page margins, page size and the five page boxes.
//!
//! moving content to new margins, or onto a new paper size, is one matrix
//! wrapped around the whole page content ("q <matrix> cm" before it, "Q"
//! after), and the same matrix applied to every annotation, so links,
//! form fields and comments stay on what they mark. adding or removing
//! white space on one side only changes the boxes: content stays where it
//! is in pdf space. core/pages' crop (which hides part of the page with
//! /CropBox from a box seen on screen) is not duplicated here.

use lopdf::{Dictionary, Document, Object, Stream};
use serde::{Deserialize, Serialize};

use crate::draft::Draft;
use crate::error::EditError;
use crate::geometry::{Matrix, Point, Rect};
use crate::interpret;
use crate::layout::{self, PageModel};
use crate::model::PageObjects;
use crate::moving::{self, matrix_numbers};
use crate::page_space::{inherited, PageSpace};
use crate::pdf_values::{number_array, rect_from_object};
use crate::structure;
use crate::summary::{ChangesSummary, EditConsent};

/// a path covering more than this share of the page is a background or a
/// frame, not content whose edge is a margin.
const BACKGROUND_SHARE: f32 = 0.5;
/// paper sizes are whole points; boxes this close count as the same.
const BOX_TOLERANCE: f32 = 0.01;

/// the four margins of a page, in points, as seen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// where a page's content sits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMargins {
    pub page: usize,
    pub width: f32,
    pub height: f32,
    /// the box around all text, pictures, drawings and annotations; none
    /// for an empty page
    pub content: Option<Rect>,
    /// the space between the content and each edge (all zero when empty)
    pub margins: Margins,
}

/// how content moves to new margins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarginFit {
    /// scale the content (keeping its proportions) to fit inside the new
    /// margins, its top left corner on the top and left margins
    #[default]
    Scale,
    /// keep the content's size; move its top left corner to the margins
    Shift,
}

/// how content goes onto a new paper size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PaperFit {
    /// scale the page down or up to fit the new paper, centred
    #[default]
    Fit,
    /// scale to cover the new paper, centred; the overflow is cut off
    Fill,
    /// keep the size, centred
    KeepSize,
}

/// one side of the page as seen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

/// the page boxes, as stored: [left, bottom, right, top] in pdf points
/// before the page's rotation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PageBoxes {
    pub media: Option<[f32; 4]>,
    pub crop: Option<[f32; 4]>,
    pub bleed: Option<[f32; 4]>,
    pub trim: Option<[f32; 4]>,
    pub art: Option<[f32; 4]>,
}

/// what a page-level change did.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageChangeResult {
    pub objects: PageObjects,
    pub margins: PageMargins,
    pub summary: ChangesSummary,
}

/// the box around a page's content on screen and its margins.
pub fn page_margins(doc: &Document, page: usize) -> Result<PageMargins, EditError> {
    let model = layout::build(doc, page)?;
    let objects = model.objects();
    let annotations = moving::annotations(&Draft::new(doc), &model);
    let annotation_rects: Vec<Rect> = annotations
        .iter()
        .map(|annotation| annotation.rect)
        .collect();
    Ok(margins_of(&objects, &annotation_rects))
}

fn margins_of(objects: &PageObjects, annotations: &[Rect]) -> PageMargins {
    let page_area = objects.width * objects.height;
    let rects = objects
        .text_blocks
        .iter()
        .map(|block| block.rect)
        .chain(
            objects
                .images
                .iter()
                .map(|image| image.crop_rect.unwrap_or(image.rect)),
        )
        .chain(
            objects
                .paths
                .iter()
                .map(|path| path.rect)
                .filter(|rect| rect.width * rect.height < BACKGROUND_SHARE * page_area),
        )
        .chain(annotations.iter().copied());
    let content = rects.reduce(|all, rect| all.union(&rect));
    let margins = content.map_or(Margins::default(), |content| Margins {
        top: content.y,
        right: objects.width - content.right(),
        bottom: objects.height - content.bottom(),
        left: content.x,
    });
    PageMargins {
        page: objects.page,
        width: objects.width,
        height: objects.height,
        content,
        margins,
    }
}

fn check_margins(margins: &Margins, width: f32, height: f32) -> Result<(), EditError> {
    let values = [margins.top, margins.right, margins.bottom, margins.left];
    let usable = margins.left + margins.right < width && margins.top + margins.bottom < height;
    if values
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
        || !usable
    {
        return Err(EditError::InvalidValue(
            "Margins must be zero or more and leave some room on the page.".into(),
        ));
    }
    Ok(())
}

/// moves (and with `MarginFit::Scale` scales) all of a page's content so
/// it sits inside new margins.
pub fn set_page_margins(
    doc: &mut Document,
    page: usize,
    margins: &Margins,
    fit: MarginFit,
    consent: &EditConsent,
) -> Result<PageChangeResult, EditError> {
    structure::guard(doc, consent)?;
    let model = layout::build(doc, page)?;
    let current = page_margins(doc, page)?;
    check_margins(margins, current.width, current.height)?;
    let content = current
        .content
        .ok_or_else(|| EditError::InvalidValue("This page has no content to move.".into()))?;
    let visible = margin_matrix(&content, margins, current.width, current.height, fit);
    let change = moving::page_change(&model, &visible);
    transform_page(doc, &model, &change, None)
}

/// the on-screen matrix that takes the content box to its new place.
fn margin_matrix(
    content: &Rect,
    margins: &Margins,
    width: f32,
    height: f32,
    fit: MarginFit,
) -> Matrix {
    let room_width = width - margins.left - margins.right;
    let room_height = height - margins.top - margins.bottom;
    let scale = match fit {
        MarginFit::Scale => {
            (room_width / content.width.max(1e-6)).min(room_height / content.height.max(1e-6))
        }
        MarginFit::Shift => 1.0,
    };
    Matrix::translate(-content.x, -content.y)
        .then(&Matrix::new(scale, 0.0, 0.0, scale, 0.0, 0.0))
        .then(&Matrix::translate(margins.left, margins.top))
}

/// wraps the page's content in one matrix, applies it to annotations,
/// and (for a resize) sets the new boxes. commits only if all worked.
fn transform_page(
    doc: &mut Document,
    model: &PageModel,
    change: &Matrix,
    new_media: Option<[f32; 4]>,
) -> Result<PageChangeResult, EditError> {
    let changes = {
        let mut draft = Draft::new(doc);
        let annotations = moving::annotations(&draft, model);
        moving::move_annotations(&mut draft, model, &annotations, change)?;
        let mut page = draft
            .dictionary(model.run.page_id)
            .cloned()
            .ok_or_else(|| EditError::Damaged("the page is missing".into()))?;
        wrap_contents(&mut draft, &mut page, change);
        if let Some(media) = new_media {
            set_paper(&mut page, media);
        }
        draft.set(model.run.page_id, Object::Dictionary(page));
        draft.finish()
    };
    changes.commit(doc);
    let mut summary = ChangesSummary::default();
    summary.page(model.page);
    // everything on the page moved with the one matrix
    let after = layout::build(doc, model.page)?;
    let objects = after.objects();
    summary.text_lines_moved = objects
        .text_blocks
        .iter()
        .map(|block| block.lines.len())
        .sum();
    summary.images_moved = objects.images.len();
    summary.paths_moved = objects.paths.len();
    summary.annotations_moved = moving::annotations(&Draft::new(doc), &after).len();
    Ok(PageChangeResult {
        margins: page_margins(doc, model.page)?,
        objects,
        summary,
    })
}

fn plain_stream(content: String) -> Object {
    let mut stream = Stream::new(Dictionary::new(), content.into_bytes());
    let _ = stream.compress();
    Object::Stream(stream)
}

/// puts "q <matrix> cm" before the page's content and "Q" after it.
fn wrap_contents(draft: &mut Draft, page: &mut Dictionary, change: &Matrix) {
    let mut contents: Vec<Object> = interpret::page_content_streams(draft.doc, page)
        .into_iter()
        .map(Object::Reference)
        .collect();
    let opening = draft.add(plain_stream(format!("q {} cm\n", matrix_numbers(change))));
    let closing = draft.add(plain_stream("\nQ\n".to_owned()));
    contents.insert(0, Object::Reference(opening));
    contents.push(Object::Reference(closing));
    page.set("Contents", Object::Array(contents));
}

/// a new paper size: the media box, and no other box (they described the
/// old paper and would cut the new one).
fn set_paper(page: &mut Dictionary, media: [f32; 4]) {
    page.set("MediaBox", number_array(&media));
    for key in [b"CropBox".as_slice(), b"BleedBox", b"TrimBox", b"ArtBox"] {
        page.remove(key);
    }
}

/// gives the page new paper (`width` by `height` points as seen) and puts
/// its content on it.
pub fn resize_page(
    doc: &mut Document,
    page: usize,
    width: f32,
    height: f32,
    fit: PaperFit,
    consent: &EditConsent,
) -> Result<PageChangeResult, EditError> {
    if !(width.is_finite() && height.is_finite() && width >= 1.0 && height >= 1.0) {
        return Err(EditError::InvalidValue(
            "The paper must be at least one point each way.".into(),
        ));
    }
    structure::guard(doc, consent)?;
    let model = layout::build(doc, page)?;
    let old = &model.run.space;
    // the page turns the paper as it did; the stored box is the unturned paper
    let (stored_width, stored_height) = if old.rotation % 180 == 0 {
        (width, height)
    } else {
        (height, width)
    };
    let media = [0.0, 0.0, stored_width, stored_height];
    let new_space = PageSpace::new(media, old.rotation);
    let (scale_x, scale_y) = (width / old.width, height / old.height);
    let scale = match fit {
        PaperFit::Fit => scale_x.min(scale_y),
        PaperFit::Fill => scale_x.max(scale_y),
        PaperFit::KeepSize => 1.0,
    };
    let offset = Point::new(
        (width - old.width * scale) / 2.0,
        (height - old.height * scale) / 2.0,
    );
    let visible = Matrix::new(scale, 0.0, 0.0, scale, offset.x, offset.y);
    let change = old
        .to_visible_matrix()
        .then(&visible)
        .then(&new_space.to_user_matrix());
    transform_page(doc, &model, &change, Some(media))
}

/// the stored box edge that a side seen on screen is: 0 left, 1 bottom,
/// 2 right, 3 top of [left, bottom, right, top].
fn edge_of(space: &PageSpace, side: Side) -> usize {
    let outward = match side {
        Side::Top => Point::new(0.0, -1.0),
        Side::Right => Point::new(1.0, 0.0),
        Side::Bottom => Point::new(0.0, 1.0),
        Side::Left => Point::new(-1.0, 0.0),
    };
    let user = space.user_vector(outward);
    match (user.x.round() as i32, user.y.round() as i32) {
        (-1, _) => 0,
        (_, -1) => 1,
        (1, _) => 2,
        _ => 3,
    }
}

/// adds white space to one side of the page (or, for a negative amount,
/// takes it away), as for a binding margin. content stays where it is.
pub fn add_page_space(
    doc: &mut Document,
    page: usize,
    side: Side,
    amount: f32,
    consent: &EditConsent,
) -> Result<PageBoxes, EditError> {
    if !amount.is_finite() {
        return Err(EditError::InvalidValue(
            "That amount isn’t a number of points.".into(),
        ));
    }
    structure::guard(doc, consent)?;
    let page_id = crate::pages::page_id(doc, page)?;
    let space = PageSpace::of_page(doc, page_id)?;
    let edge = edge_of(&space, side);
    // left and bottom grow outwards by going down; right and top by going up
    let delta = if edge < 2 { -amount } else { amount };
    let mut boxes = page_boxes(doc, page)?;
    for stored in [&mut boxes.media, &mut boxes.crop].into_iter().flatten() {
        stored[edge] += delta;
        if stored[2] - stored[0] < 1.0 || stored[3] - stored[1] < 1.0 {
            return Err(EditError::InvalidValue("That would leave no page.".into()));
        }
    }
    set_page_boxes(doc, page, &boxes, consent)?;
    page_boxes(doc, page)
}

/// the page's boxes as they apply to it (inherited ones included). the
/// crop box is the media box when the page sets none.
pub fn page_boxes(doc: &Document, page: usize) -> Result<PageBoxes, EditError> {
    let page_id = crate::pages::page_id(doc, page)?;
    let dict = doc
        .get_dictionary(page_id)
        .map_err(|_| EditError::Damaged("the page is missing".into()))?;
    let read =
        |key: &[u8]| inherited(doc, dict, key).and_then(|value| rect_from_object(doc, value));
    Ok(PageBoxes {
        media: read(b"MediaBox"),
        crop: read(b"CropBox"),
        bleed: read(b"BleedBox"),
        trim: read(b"TrimBox"),
        art: read(b"ArtBox"),
    })
}

fn valid_box(value: &[f32; 4]) -> bool {
    value.iter().all(|number| number.is_finite()) && value[2] > value[0] && value[3] > value[1]
}

fn inside(inner: &[f32; 4], outer: &[f32; 4]) -> bool {
    inner[0] >= outer[0] - BOX_TOLERANCE
        && inner[1] >= outer[1] - BOX_TOLERANCE
        && inner[2] <= outer[2] + BOX_TOLERANCE
        && inner[3] <= outer[3] + BOX_TOLERANCE
}

/// sets the page's boxes exactly (those given; the others stay). every
/// box must lie inside the media box, as the pdf standard requires.
pub fn set_page_boxes(
    doc: &mut Document,
    page: usize,
    boxes: &PageBoxes,
    consent: &EditConsent,
) -> Result<(), EditError> {
    structure::guard(doc, consent)?;
    let page_id = crate::pages::page_id(doc, page)?;
    let current = page_boxes(doc, page)?;
    let media = boxes
        .media
        .or(current.media)
        .unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let named = [
        ("MediaBox", boxes.media),
        ("CropBox", boxes.crop),
        ("BleedBox", boxes.bleed),
        ("TrimBox", boxes.trim),
        ("ArtBox", boxes.art),
    ];
    for (key, value) in named.iter() {
        let Some(value) = value else { continue };
        if !valid_box(value) {
            return Err(EditError::InvalidValue(format!(
                "The {key} needs a left below its right and a bottom below its top."
            )));
        }
        if *key != "MediaBox" && !inside(value, &media) {
            return Err(EditError::InvalidValue(format!(
                "The {key} must lie inside the MediaBox."
            )));
        }
    }
    let mut dict = doc
        .get_dictionary(page_id)
        .map_err(|_| EditError::Damaged("the page is missing".into()))?
        .clone();
    for (key, value) in named {
        if let Some(value) = value {
            dict.set(key, number_array(&value));
        }
    }
    doc.objects.insert(page_id, Object::Dictionary(dict));
    Ok(())
}
