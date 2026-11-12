//! more image edits: cropping (by a clipping path that keeps every
//! pixel, or by cutting the pixels away), flipping and opacity. each one
//! changes only that drawing of the image, by wrapping its operator.
//!
//! an image is drawn into the unit square of the user space in force at
//! its Do operator, so a crop is a rectangle in that square: "q x y w h re
//! W n ... Q" around the operator.

use lopdf::{Dictionary, Document, Object};
use serde::{Deserialize, Serialize};

use crate::draft::Draft;
use crate::edit_run::{run_edit, Landing, Outcome};
use crate::error::EditError;
use crate::geometry::{Matrix, Point, Rect};
use crate::image_data;
use crate::layout::PageModel;
use crate::moving::{matrix_numbers, wrap};
use crate::pdf_values::{format_number, name, real};
use crate::rewrite::Plan;
use crate::summary::{ContentEditResult, EditConsent};
use crate::writer;

/// which way to flip a picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlipDirection {
    /// left becomes right
    Horizontal,
    /// top becomes bottom
    Vertical,
}

/// the part of the image's unit square under a box on screen, clamped
/// to the square: left, bottom, right, top.
fn unit_region(model: &PageModel, image: usize, rect: &Rect) -> Option<[f32; 4]> {
    let ctm = model.run.images[image].ctm;
    let inverse = ctm.invert()?;
    let corners: Vec<Point> = rect
        .corners()
        .iter()
        .map(|corner| inverse.apply(model.run.space.user_point(*corner)))
        .collect();
    let around = Rect::around(corners)?;
    let left = around.x.clamp(0.0, 1.0);
    let bottom = around.y.clamp(0.0, 1.0);
    let right = around.right().clamp(0.0, 1.0);
    let top = around.bottom().clamp(0.0, 1.0);
    (right > left && top > bottom).then_some([left, bottom, right, top])
}

fn no_overlap() -> EditError {
    EditError::InvalidValue("The crop must cover part of the picture.".into())
}

/// hides the parts of an image outside `rect` (points on screen) with a
/// clipping path. every pixel stays in the file, so the crop can be
/// undone or changed later.
pub fn crop_image(
    doc: &mut Document,
    page: usize,
    id: &str,
    rect: Rect,
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    run_edit(doc, page, consent, |model, _, plan| {
        let image = model.find_image(id)?;
        let [left, bottom, right, top] = unit_region(model, image, &rect).ok_or_else(no_overlap)?;
        let record = &model.run.images[image];
        let clip = format!(
            "q {} {} {} {} re W n",
            format_number(left),
            format_number(bottom),
            format_number(right - left),
            format_number(top - bottom)
        );
        wrap(plan, record.source, record.op, record.op, &clip, "Q");
        let mut outcome = Outcome::default();
        outcome
            .landings
            .push(Landing::Image(model.image_quad(image)));
        Ok(outcome)
    })
}

/// cuts an image down to the part inside `rect` (or to its current crop,
/// for none): the hidden pixels are removed from the file, and the new,
/// smaller picture is drawn exactly where that part was. the original
/// image object stays if anything else draws it.
pub fn crop_image_pixels(
    doc: &mut Document,
    page: usize,
    id: &str,
    rect: Option<Rect>,
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    run_edit(doc, page, consent, |model, draft, plan| {
        let image = model.find_image(id)?;
        let objects = model.objects();
        let current = &objects.images[image];
        let wanted = rect.or(current.crop_rect).ok_or_else(|| {
            EditError::InvalidValue("Choose the part of the picture to keep first.".into())
        })?;
        let region = unit_region(model, image, &wanted).ok_or_else(no_overlap)?;
        let landed = cut_pixels(draft, model, plan, image, region)?;
        let mut outcome = Outcome::default();
        outcome.landings.push(Landing::Image(landed));
        Ok(outcome)
    })
}

/// writes the cut-down picture in the image's place; returns where its
/// corners land on screen.
fn cut_pixels(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    image: usize,
    region: [f32; 4],
) -> Result<[Point; 4], EditError> {
    let record = &model.run.images[image];
    let object = record.object.ok_or_else(|| {
        EditError::CannotExtract(
            "It is written inside the page’s drawing instructions, which Letters can’t crop yet."
                .into(),
        )
    })?;
    let extracted = image_data::extract(draft.doc, object)?;
    let picture = image::load_from_memory(&extracted.bytes)
        .map_err(|_| EditError::UnsupportedImage("Letters can’t read its pixels.".into()))?;
    let [left, bottom, right, top] = region;
    let (width, height) = (picture.width() as f32, picture.height() as f32);
    // image rows run from the top of the unit square down
    let x = (left * width).floor() as u32;
    let y = ((1.0 - top) * height).floor() as u32;
    let kept_width = ((right - left) * width).ceil().max(1.0) as u32;
    let kept_height = ((top - bottom) * height).ceil().max(1.0) as u32;
    let cut = picture.crop_imm(
        x,
        y,
        kept_width.min(picture.width() - x),
        kept_height.min(picture.height() - y),
    );
    let mut png = Vec::new();
    cut.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|_| EditError::UnsupportedImage("The cut picture couldn’t be written.".into()))?;
    let added = image_data::add_image_object(draft, &png)?;
    let name = plan.resource_name(
        draft.doc,
        &model.run,
        Some(record.source),
        "XObject",
        added.object,
    );
    // the kept pixels' exact share of the square, after rounding to pixels
    let placed = Matrix::new(
        cut.width() as f32 / width,
        0.0,
        0.0,
        cut.height() as f32 / height,
        x as f32 / width,
        1.0 - (y + cut.height()) as f32 / height,
    );
    let drawing = format!(
        "q {} cm {} Do Q",
        matrix_numbers(&placed),
        writer::name(&name)
    );
    plan.op(record.source, record.op).replace = Some(drawing.into_bytes());
    let drawn = placed.then(&record.ctm);
    Ok([(0.0, 1.0), (1.0, 1.0), (0.0, 0.0), (1.0, 0.0)]
        .map(|(u, v)| model.run.space.visible_point(drawn.apply(Point::new(u, v)))))
}

/// mirrors an image in its own place.
pub fn flip_image(
    doc: &mut Document,
    page: usize,
    id: &str,
    direction: FlipDirection,
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    run_edit(doc, page, consent, |model, _, plan| {
        let image = model.find_image(id)?;
        let record = &model.run.images[image];
        // a flip inside the unit square: x becomes 1 - x (or y, 1 - y)
        let flip = match direction {
            FlipDirection::Horizontal => "q -1 0 0 1 1 0 cm",
            FlipDirection::Vertical => "q 1 0 0 -1 0 1 cm",
        };
        wrap(plan, record.source, record.op, record.op, flip, "Q");
        let quad = model.image_quad(image);
        let flipped = match direction {
            FlipDirection::Horizontal => [quad[1], quad[0], quad[3], quad[2]],
            FlipDirection::Vertical => [quad[2], quad[3], quad[0], quad[1]],
        };
        let mut outcome = Outcome::default();
        outcome.landings.push(Landing::Image(flipped));
        Ok(outcome)
    })
}

/// sets how opaque an image is, from 0 (invisible) to 1.
pub fn set_image_opacity(
    doc: &mut Document,
    page: usize,
    id: &str,
    opacity: f32,
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    if !(0.0..=1.0).contains(&opacity) {
        return Err(EditError::InvalidValue(
            "Opacity must be between 0 and 1.".into(),
        ));
    }
    run_edit(doc, page, consent, |model, draft, plan| {
        let image = model.find_image(id)?;
        let record = &model.run.images[image];
        let mut state = Dictionary::new();
        state.set("Type", name("ExtGState"));
        state.set("ca", real(opacity));
        state.set("CA", real(opacity));
        let state_id = draft.add(Object::Dictionary(state));
        let resource = plan.resource_name(
            draft.doc,
            &model.run,
            Some(record.source),
            "ExtGState",
            state_id,
        );
        wrap(
            plan,
            record.source,
            record.op,
            record.op,
            &format!("q {} gs", writer::name(&resource)),
            "Q",
        );
        let mut outcome = Outcome::default();
        outcome
            .landings
            .push(Landing::Image(model.image_quad(image)));
        Ok(outcome)
    })
}
