//! editing vector drawings: moving, resizing, turning, restyling,
//! deleting, restacking and duplicating paths, and editing the points of
//! simple shapes.
//!
//! every operation changes only the chosen drawings. a path is its
//! construction operators (m, l, c, re ...) and one painting operator
//! (f, S, B ...). moving wraps exactly that range in "q <matrix> cm ...
//! Q"; restyling wraps it with new colour or line operators (and may swap
//! the painting operator); deleting empties it. a path inside a form
//! XObject is changed in a copy of the form that only this page draws.
//!
//! several ids can be given at once, so a logo made of many paths (see
//! `PageObjects::path_groups`) moves as one object.

mod points;
mod restack;
mod style;

pub(crate) use points::reshape;
pub use points::set_path_points;
pub use restack::{bring_paths_to_front, duplicate_paths, send_paths_to_back};
pub use style::{delete_paths, set_path_style};

use lopdf::{Dictionary, Document, Object};
use serde::{Deserialize, Serialize};

use crate::color_ops;
use crate::draft::Draft;
use crate::edit_run::{run_edit, Landing, Outcome};
use crate::error::EditError;
use crate::geometry::{Matrix, Point, Rect};
use crate::interpret::{PathRecord, Segment};
use crate::layout::PageModel;
use crate::model::PathShape;
use crate::moving::{self, local_change, matrix_numbers, page_change, screen_move, wrap};
use crate::pdf_values::{format_number, name, real};
use crate::rewrite::Plan;
use crate::summary::{ContentEditResult, EditConsent};
use crate::writer;

/// a new colour for a path's fill or outline.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NewPaint {
    /// no fill (or no outline)
    None,
    Rgb {
        color: crate::colors::Color,
    },
    /// 0 is black, 1 white
    Gray {
        value: f32,
    },
    /// 0 to 1 each
    Cmyk {
        c: f32,
        m: f32,
        y: f32,
        k: f32,
    },
}

/// a change of look for paths. fields left out stay as they are.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PathStyle {
    pub fill: Option<NewPaint>,
    pub stroke: Option<NewPaint>,
    /// line width as seen on the page, in points
    pub stroke_width: Option<f32>,
    /// dash lengths as seen, in points; an empty list makes a solid line
    pub dash: Option<Vec<f32>>,
    /// 0 to 1, for fill and outline together
    pub opacity: Option<f32>,
}

fn check_ids(ids: &[String]) -> Result<(), EditError> {
    if ids.is_empty() {
        Err(EditError::InvalidValue("Select a drawing first.".into()))
    } else {
        Ok(())
    }
}

fn find_all(model: &PageModel, ids: &[String]) -> Result<Vec<usize>, EditError> {
    ids.iter().map(|id| model.find_path(id)).collect()
}

/// the box around several paths on screen.
fn union_rect(model: &PageModel, paths: &[usize]) -> Rect {
    paths
        .iter()
        .skip(1)
        .fold(model.path_rect(paths[0]), |all, path| {
            all.union(&model.path_rect(*path))
        })
}

/// where a path's box lands after a page-space change.
fn landed_rect(model: &PageModel, path: usize, change: &Matrix) -> Rect {
    let rect = model.path_rect(path);
    let corners = rect.corners().map(|corner| {
        let user = model.run.space.user_point(corner);
        model.run.space.visible_point(change.apply(user))
    });
    Rect::around(corners).expect("four corners")
}

/// applies one page-space change to several paths.
fn transform_paths(
    doc: &mut Document,
    page: usize,
    ids: &[String],
    consent: &EditConsent,
    change_for: impl Fn(&PageModel, &[usize]) -> Matrix,
) -> Result<ContentEditResult, EditError> {
    check_ids(ids)?;
    run_edit(doc, page, consent, |model, draft, plan| {
        let paths = find_all(model, ids)?;
        let change = change_for(model, &paths);
        let mut outcome = Outcome::default();
        for path in &paths {
            transform_one(draft, model, plan, *path, &change)?;
            outcome
                .landings
                .push(Landing::Path(landed_rect(model, *path, &change)));
            outcome.summary.paths_moved += 1;
        }
        Ok(outcome)
    })
}

/// moves one path; a shading whose clip is its own moves with its clip.
fn transform_one(
    draft: &mut Draft,
    model: &PageModel,
    plan: &mut Plan,
    path: usize,
    change: &Matrix,
) -> Result<(), EditError> {
    let record = &model.run.paths[path];
    if record.paint.shading
        && record.first_op != record.paint_op
        && !shading_owns_clip(model, record)
    {
        // the clip limits other drawings too: move only the shading
        let local = local_change(&record.ctm, change);
        wrap(
            plan,
            record.source,
            record.paint_op,
            record.paint_op,
            &format!("q {} cm", matrix_numbers(&local)),
            "Q",
        );
        return Ok(());
    }
    moving::move_path(draft, model, plan, path, change)
}

/// whether nothing but the shading is drawn through its clip: the q
/// block ends right after it.
fn shading_owns_clip(model: &PageModel, record: &PathRecord) -> bool {
    let operations = &model.run.sources[record.source].operations;
    for operation in &operations[record.paint_op + 1..] {
        match operation.operator.as_str() {
            "Q" => return true,
            "EMC" => {}
            _ => return false,
        }
    }
    true
}

/// moves paths by `dx`, `dy` points on screen.
pub fn move_paths(
    doc: &mut Document,
    page: usize,
    ids: &[String],
    dx: f32,
    dy: f32,
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    if !(dx.is_finite() && dy.is_finite()) {
        return Err(EditError::InvalidValue(
            "That move isnâ€™t a number of points.".into(),
        ));
    }
    transform_paths(doc, page, ids, consent, |model, _| {
        screen_move(model, dx, dy)
    })
}

/// stretches paths so the box around them all becomes `rect`. line widths
/// scale with them (use `set_path_points` to reshape without that).
pub fn resize_paths(
    doc: &mut Document,
    page: usize,
    ids: &[String],
    rect: Rect,
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    if rect.is_empty() {
        return Err(EditError::InvalidValue(
            "Choose an area with some width and height.".into(),
        ));
    }
    transform_paths(doc, page, ids, consent, |model, paths| {
        let around = union_rect(model, paths);
        let scale = Matrix::new(
            rect.width / around.width.max(1e-6),
            0.0,
            0.0,
            rect.height / around.height.max(1e-6),
            0.0,
            0.0,
        );
        let visible = Matrix::translate(-around.x, -around.y)
            .then(&scale)
            .then(&Matrix::translate(rect.x, rect.y));
        page_change(model, &visible)
    })
}

/// turns paths clockwise on screen by `degrees` around the centre of the
/// box around them all.
pub fn rotate_paths(
    doc: &mut Document,
    page: usize,
    ids: &[String],
    degrees: f32,
    consent: &EditConsent,
) -> Result<ContentEditResult, EditError> {
    if !degrees.is_finite() {
        return Err(EditError::InvalidValue(
            "That turn isnâ€™t a number of degrees.".into(),
        ));
    }
    transform_paths(doc, page, ids, consent, |model, paths| {
        let around = union_rect(model, paths);
        let centre = Point::new(
            around.x + around.width / 2.0,
            around.y + around.height / 2.0,
        );
        let (sin, cos) = degrees.to_radians().sin_cos();
        // y goes down on screen, so this turn is clockwise as seen
        let turn = Matrix::new(cos, sin, -sin, cos, 0.0, 0.0);
        let visible = Matrix::translate(-centre.x, -centre.y)
            .then(&turn)
            .then(&Matrix::translate(centre.x, centre.y));
        page_change(model, &visible)
    })
}
