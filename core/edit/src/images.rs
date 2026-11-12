//! adding, replacing, moving, resizing, turning, deleting and saving out
//! images that are part of the page content.
//!
//! an image is drawn into a unit square placed by the current matrix.
//! moving or resizing one changes only its own drawing: the operator
//! that draws it is wrapped in "q <matrix> cm ... Q", so the same image
//! drawn elsewhere (or a drawing that shares a form) stays as it was.

use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

use crate::draft::Draft;
use crate::error::EditError;
use crate::geometry::{Matrix, Point, Rect};
use crate::image_data::{self, ExtractedImage};
use crate::layout::{self, PageModel};
use crate::model::PageObjects;
use crate::pdf_values::format_number;
use crate::rewrite::{self, Plan};
use crate::writer;

/// how a new picture fills the place of the one it replaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageFit {
    /// exactly the old picture's place, stretched if the shapes differ
    #[default]
    Stretch,
    /// as large as fits inside the old place, keeping its proportions
    Fit,
    /// covers the old place, keeping its proportions; the overflow is cut
    Fill,
}

/// the page after an image edit, and the id the image has now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageEditResult {
    pub objects: PageObjects,
    /// none after a delete
    pub image_id: Option<String>,
}

fn matrix_operator(matrix: Matrix) -> String {
    let numbers: Vec<String> = matrix
        .to_array()
        .iter()
        .map(|value| format_number(*value))
        .collect();
    format!("{} cm", numbers.join(" "))
}

/// runs an image edit against a draft, commits it, and finds the image
/// again on the changed page by where its drawing now sits.
fn edit(
    doc: &mut Document,
    page: usize,
    work: impl FnOnce(&PageModel, &mut Draft, &mut Plan) -> Result<Option<[Point; 4]>, EditError>,
) -> Result<ImageEditResult, EditError> {
    let model = layout::build(doc, page)?;
    let changes;
    let landing;
    {
        let mut draft = Draft::new(doc);
        let mut plan = Plan::default();
        landing = work(&model, &mut draft, &mut plan)?;
        rewrite::apply(&mut draft, &model.run, plan)?;
        changes = draft.finish();
    }
    changes.commit(doc);
    let objects = layout::build(doc, page)?.objects();
    let image_id = landing.and_then(|quad| {
        objects
            .images
            .iter()
            .find(|image| {
                image
                    .quad
                    .iter()
                    .zip(&quad)
                    .all(|(a, b)| (a.x - b.x).abs() < 0.05 && (a.y - b.y).abs() < 0.05)
            })
            .map(|image| image.id.clone())
    });
    Ok(ImageEditResult { objects, image_id })
}

/// the matrix that draws the unit square on a visible quad (top left,
/// top right, bottom left, bottom right on screen).
fn placement(model: &PageModel, quad: &[Point; 4]) -> Matrix {
    let user = quad.map(|corner| model.run.space.user_point(corner));
    Matrix::unit_square_to(user[0], user[1], user[2])
}

fn check_rect(rect: Rect) -> Result<(), EditError> {
    if rect.is_empty() {
        Err(EditError::InvalidValue(
            "Choose an area with some width and height for the picture.".into(),
        ))
    } else {
        Ok(())
    }
}

/// places a png or jpeg picture on the page, on top of what is there.
pub fn add_image(
    doc: &mut Document,
    page: usize,
    rect: Rect,
    bytes: &[u8],
) -> Result<ImageEditResult, EditError> {
    check_rect(rect)?;
    edit(doc, page, |model, draft, plan| {
        let image = image_data::add_image_object(draft, bytes)?;
        let name = plan.resource_name(draft.doc, &model.run, None, "XObject", image.object);
        let quad = rect.corners();
        let drawing = format!(
            "q {} {} Do Q",
            matrix_operator(placement(model, &quad)),
            writer::name(&name)
        );
        plan.append(drawing.as_bytes());
        Ok(Some(quad))
    })
}

/// moves one drawing of an image so its corners land on `quad`.
fn set_quad(
    model: &PageModel,
    plan: &mut Plan,
    image: usize,
    quad: &[Point; 4],
) -> Result<(), EditError> {
    let record = &model.run.images[image];
    let current = record.ctm;
    let inverse = current
        .invert()
        .ok_or_else(|| EditError::Damaged("the picture has no size".into()))?;
    // the change, applied in the drawing's own space: new = change x current
    let change = placement(model, quad).then(&inverse);
    // inside a form, the form's box must grow to hold the new place
    let reach = quad.map(|corner| model.run.space.user_point(corner));
    plan.reach(record.source, &reach);
    let edit = plan.op(record.source, record.op);
    edit.before = format!("q {}", matrix_operator(change)).into_bytes();
    edit.after = b"Q".to_vec();
    Ok(())
}

/// moves an image by `dx`, `dy` points on screen.
pub fn move_image(
    doc: &mut Document,
    page: usize,
    id: &str,
    dx: f32,
    dy: f32,
) -> Result<ImageEditResult, EditError> {
    if !(dx.is_finite() && dy.is_finite()) {
        return Err(EditError::InvalidValue(
            "That move isn’t a number of points.".into(),
        ));
    }
    edit(doc, page, |model, _, plan| {
        let image = model.find_image(id)?;
        let quad = model
            .image_quad(image)
            .map(|corner| Point::new(corner.x + dx, corner.y + dy));
        set_quad(model, plan, image, &quad)?;
        Ok(Some(quad))
    })
}

/// stretches an image so the box around it becomes `rect`. a turned
/// image stays turned, scaled to the new box.
pub fn resize_image(
    doc: &mut Document,
    page: usize,
    id: &str,
    rect: Rect,
) -> Result<ImageEditResult, EditError> {
    check_rect(rect)?;
    edit(doc, page, |model, _, plan| {
        let image = model.find_image(id)?;
        let quad = model.image_quad(image);
        let around = Rect::around(quad).expect("four corners");
        let scale_x = rect.width / around.width.max(1e-6);
        let scale_y = rect.height / around.height.max(1e-6);
        let quad = quad.map(|corner| {
            Point::new(
                rect.x + (corner.x - around.x) * scale_x,
                rect.y + (corner.y - around.y) * scale_y,
            )
        });
        set_quad(model, plan, image, &quad)?;
        Ok(Some(quad))
    })
}

/// turns an image clockwise on screen by `degrees`, around its centre.
pub fn rotate_image(
    doc: &mut Document,
    page: usize,
    id: &str,
    degrees: f32,
) -> Result<ImageEditResult, EditError> {
    if !degrees.is_finite() {
        return Err(EditError::InvalidValue(
            "That turn isn’t a number of degrees.".into(),
        ));
    }
    edit(doc, page, |model, _, plan| {
        let image = model.find_image(id)?;
        let quad = model.image_quad(image);
        let centre = quad.iter().fold(Point::new(0.0, 0.0), |sum, corner| {
            sum.plus(corner.scaled(0.25))
        });
        let (sin, cos) = degrees.to_radians().sin_cos();
        // y goes down on screen, so this turn is clockwise as seen
        let turned = quad.map(|corner| {
            let offset = corner.minus(centre);
            Point::new(
                centre.x + offset.x * cos - offset.y * sin,
                centre.y + offset.x * sin + offset.y * cos,
            )
        });
        set_quad(model, plan, image, &turned)?;
        Ok(Some(turned))
    })
}

/// removes one drawing of an image from the page. the image object stays
/// in the file if something else still draws it.
pub fn delete_image(
    doc: &mut Document,
    page: usize,
    id: &str,
) -> Result<ImageEditResult, EditError> {
    edit(doc, page, |model, _, plan| {
        let image = model.find_image(id)?;
        let record = &model.run.images[image];
        plan.op(record.source, record.op).replace = Some(Vec::new());
        Ok(None)
    })
}

/// puts a new png or jpeg picture in an image's place.
pub fn replace_image(
    doc: &mut Document,
    page: usize,
    id: &str,
    bytes: &[u8],
    fit: ImageFit,
) -> Result<ImageEditResult, EditError> {
    edit(doc, page, |model, draft, plan| {
        let image = model.find_image(id)?;
        let record = &model.run.images[image];
        let added = image_data::add_image_object(draft, bytes)?;
        let name = plan.resource_name(
            draft.doc,
            &model.run,
            Some(record.source),
            "XObject",
            added.object,
        );
        let quad = model.image_quad(image);
        let place_width = quad[1].minus(quad[0]).length();
        let place_height = quad[0].minus(quad[2]).length();
        let old_shape = place_width / place_height.max(1e-6);
        let new_shape = added.width as f32 / added.height.max(1) as f32;
        // the new picture's square inside the old one, in unit-square terms
        let (sx, sy) = match fit {
            ImageFit::Stretch => (1.0, 1.0),
            ImageFit::Fit if new_shape > old_shape => (1.0, old_shape / new_shape),
            ImageFit::Fit => (new_shape / old_shape, 1.0),
            ImageFit::Fill if new_shape > old_shape => (new_shape / old_shape, 1.0),
            ImageFit::Fill => (1.0, old_shape / new_shape),
        };
        let inner = Matrix::new(sx, 0.0, 0.0, sy, (1.0 - sx) / 2.0, (1.0 - sy) / 2.0);
        let clip = if fit == ImageFit::Fill {
            "0 0 1 1 re W n "
        } else {
            ""
        };
        let drawing = format!(
            "q {clip}{} {} Do Q",
            matrix_operator(inner),
            writer::name(&name)
        );
        plan.op(record.source, record.op).replace = Some(drawing.into_bytes());
        let reach = [(0.0, 0.0), (1.0, 1.0), (0.0, 1.0), (1.0, 0.0)]
            .map(|(x, y)| inner.then(&record.ctm).apply(Point::new(x, y)));
        plan.reach(record.source, &reach);
        // where the new picture's corners land, to find it again afterwards
        let drawn = inner.then(&record.ctm);
        let landed = [(0.0, 1.0), (1.0, 1.0), (0.0, 0.0), (1.0, 0.0)]
            .map(|(x, y)| model.run.space.visible_point(drawn.apply(Point::new(x, y))));
        Ok(Some(landed))
    })
}

/// the picture of an image as a file: the original bytes for a jpeg,
/// otherwise a png (with transparency when the image has a soft mask).
pub fn extract_image(doc: &Document, page: usize, id: &str) -> Result<ExtractedImage, EditError> {
    let model = layout::build(doc, page)?;
    let image = model.find_image(id)?;
    match model.run.images[image].object {
        Some(object) => image_data::extract(doc, object),
        None => Err(EditError::CannotExtract("It is written inside the page’s drawing instructions, which Letters can’t save out yet.".into())),
    }
}

/// draws an image above everything else on the page.
pub fn bring_image_to_front(
    doc: &mut Document,
    page: usize,
    id: &str,
) -> Result<ImageEditResult, EditError> {
    restack(doc, page, id, true)
}

/// draws an image below everything else on the page.
pub fn send_image_to_back(
    doc: &mut Document,
    page: usize,
    id: &str,
) -> Result<ImageEditResult, EditError> {
    restack(doc, page, id, false)
}

/// moves an image's drawing to the start or end of the page content,
/// with the full matrix it was drawn with, so it lands in the same place.
/// (a clipping path that cut the image where it was is not carried along.)
fn restack(
    doc: &mut Document,
    page: usize,
    id: &str,
    to_front: bool,
) -> Result<ImageEditResult, EditError> {
    edit(doc, page, |model, draft, plan| {
        let image = model.find_image(id)?;
        let record = &model.run.images[image];
        let source = &model.run.sources[record.source];
        let drawn = match record.object {
            Some(object) => {
                let name = plan.resource_name(draft.doc, &model.run, None, "XObject", object);
                writer::operation("Do", &[Object::Name(name)])
            }
            // an inline image carries its own data: copy it as it is
            None => source.content[source.operations[record.op].span.clone()].to_vec(),
        };
        let drawing = [
            format!("q {} ", matrix_operator(record.ctm)).into_bytes(),
            drawn,
            b" Q".to_vec(),
        ]
        .concat();
        plan.op(record.source, record.op).replace = Some(Vec::new());
        if to_front {
            plan.append(&drawing);
        } else {
            plan.prepend(&drawing);
        }
        Ok(Some(model.image_quad(image)))
    })
}
