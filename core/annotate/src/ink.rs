//! freehand drawing (/Ink).
//!
//! the points the pointer passed through are stored as they are in
//! /InkList, so nothing the person drew is lost. the appearance joins
//! them with smooth curves instead of straight steps, which is what
//! makes handwriting look like handwriting. a wide stroke with low
//! opacity works as a freehand highlighter.

use lopdf::{Dictionary, Document, Object};
use serde::{Deserialize, Serialize};

use crate::annots;
use crate::appearance::{Appearance, Canvas};
use crate::color::Color;
use crate::common::{self, Basics, Border};
use crate::error::AnnotateError;
use crate::geometry::{Point, Rect};
use crate::page_space::PageSpace;
use crate::pdf_values::{number_array, numbers, resolve};
use crate::read::AnnotationInfo;
use crate::shapes::visible_points;

/// points closer together than this (in points) add nothing a viewer
/// could show, only wobble, so they are skipped when drawing.
const MIN_POINT_SPACING: f32 = 0.5;

fn default_ink_width() -> f32 {
    2.0
}

/// what the app sends for a freehand drawing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InkParams {
    /// each stroke is the list of points one press of the pen passed
    /// through, in visible page points
    pub strokes: Vec<Vec<Point>>,
    pub color: Color,
    #[serde(default = "default_ink_width")]
    pub width: f32,
    #[serde(default = "crate::full_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub contents: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

/// adds a freehand drawing to a page.
pub fn add_ink(
    doc: &mut Document,
    page: usize,
    params: &InkParams,
) -> Result<AnnotationInfo, AnnotateError> {
    let page = annots::page(doc, page)?;
    common::check_stroke_width(params.width)?;
    let strokes = usable_strokes(&params.strokes);
    if strokes.is_empty() {
        return Err(AnnotateError::EmptyArea);
    }
    let basics = Basics {
        color: Some(params.color),
        opacity: params.opacity,
        contents: params.contents.as_deref(),
        author: params.author.as_deref(),
    };
    let mut annotation = common::new_dictionary("Ink", &page, &basics);
    write_strokes(&mut annotation, &page.space, &strokes);
    common::set_border(
        &mut annotation,
        &Border {
            width: params.width,
            dash: Vec::new(),
        },
    );
    common::add(doc, &page, annotation)
}

/// strokes with their unusable points removed, and empty strokes dropped.
pub(crate) fn usable_strokes(strokes: &[Vec<Point>]) -> Vec<Vec<Point>> {
    strokes
        .iter()
        .map(|stroke| {
            stroke
                .iter()
                .copied()
                .filter(|point| point.is_finite())
                .collect::<Vec<_>>()
        })
        .filter(|stroke| !stroke.is_empty())
        .collect()
}

pub(crate) fn write_strokes(
    annotation: &mut Dictionary,
    space: &PageSpace,
    strokes: &[Vec<Point>],
) {
    let list = strokes
        .iter()
        .map(|stroke| {
            let values: Vec<f32> = stroke
                .iter()
                .flat_map(|point| {
                    let user = space.user_point(*point);
                    [user.x, user.y]
                })
                .collect();
            number_array(&values)
        })
        .collect();
    annotation.set("InkList", Object::Array(list));
}

/// the strokes of an ink annotation, in visible points.
pub(crate) fn strokes(
    doc: &Document,
    annotation: &Dictionary,
    space: &PageSpace,
) -> Vec<Vec<Point>> {
    let Some(list) = annotation
        .get(b"InkList")
        .ok()
        .and_then(|list| resolve(doc, list).as_array().ok())
    else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|stroke| numbers(doc, stroke))
        .map(|values| visible_points(&values, space))
        .filter(|stroke| !stroke.is_empty())
        .collect()
}

/// one piece of a smoothed stroke: a cubic bezier curve to `end`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Curve {
    pub handle_1: Point,
    pub handle_2: Point,
    pub end: Point,
}

/// turns the points of a stroke into curves that pass through every
/// point.
///
/// this is the catmull-rom spline: the curve at each point heads in the
/// direction from the point before to the point after, which gives a
/// smooth line with no corners. each piece between two points becomes
/// one bezier curve whose handles sit a sixth of that direction away.
pub(crate) fn smooth(points: &[Point]) -> Vec<Curve> {
    let count = points.len();
    let at = |index: isize| points[index.clamp(0, count as isize - 1) as usize];
    (0..count.saturating_sub(1) as isize)
        .map(|index| {
            let (before, start, end, after) =
                (at(index - 1), at(index), at(index + 1), at(index + 2));
            Curve {
                handle_1: Point::new(
                    start.x + (end.x - before.x) / 6.0,
                    start.y + (end.y - before.y) / 6.0,
                ),
                handle_2: Point::new(
                    end.x - (after.x - start.x) / 6.0,
                    end.y - (after.y - start.y) / 6.0,
                ),
                end,
            }
        })
        .collect()
}

/// drops points that sit almost on top of the previous one.
fn thin_out(stroke: &[Point]) -> Vec<Point> {
    let mut kept: Vec<Point> = Vec::with_capacity(stroke.len());
    for point in stroke {
        match kept.last() {
            Some(last) if last.distance(*point) < MIN_POINT_SPACING => {}
            _ => kept.push(*point),
        }
    }
    // keep the true end of the stroke, even if it was close to the last kept point
    if kept.len() > 1 {
        if let (Some(last_kept), Some(last)) = (kept.last_mut(), stroke.last()) {
            *last_kept = *last;
        }
    }
    kept
}

/// draws all strokes as one path, so where a translucent stroke crosses
/// itself it doesn't get darker, just like a real highlighter pen.
pub(crate) fn draw(
    doc: &Document,
    annotation: &Dictionary,
    space: &PageSpace,
) -> Result<Appearance, AnnotateError> {
    let strokes: Vec<Vec<Point>> = strokes(doc, annotation, space)
        .iter()
        .map(|stroke| thin_out(stroke))
        .collect();
    let curves: Vec<Vec<Curve>> = strokes.iter().map(|stroke| smooth(stroke)).collect();
    let width = common::border(doc, annotation, 1.0).width.max(0.1);
    // the curves stay inside their handles, so the handles bound the drawing
    let every_point = strokes.iter().flatten().copied().chain(
        curves
            .iter()
            .flatten()
            .flat_map(|curve| [curve.handle_1, curve.handle_2, curve.end]),
    );
    let frame = Rect::around(every_point)
        .ok_or(AnnotateError::EmptyArea)?
        .grow(width / 2.0 + 1.0);

    let mut canvas = Canvas::new(frame);
    canvas.opacity(common::opacity(doc, annotation), false);
    canvas.stroke_color(common::color(doc, annotation, b"C").unwrap_or(Color::BLACK));
    canvas.line_width(width);
    canvas.round_line_ends();
    for (stroke, curves) in strokes.iter().zip(&curves) {
        canvas.move_to(stroke[0]);
        if curves.is_empty() {
            // a single tap: a zero length line with round ends is a dot
            canvas.line_to(stroke[0]);
        }
        for curve in curves {
            canvas.curve_to(curve.handle_1, curve.handle_2, curve.end);
        }
    }
    canvas.stroke();
    Ok(canvas.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothing_passes_through_every_point() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 5.0),
            Point::new(20.0, 0.0),
            Point::new(30.0, 5.0),
        ];
        let curves = smooth(&points);
        assert_eq!(curves.len(), 3);
        for (curve, point) in curves.iter().zip(&points[1..]) {
            assert_eq!(curve.end, *point);
        }
    }

    #[test]
    fn smooth_curves_have_no_corners() {
        // at a shared point, the handle arriving and the handle leaving
        // lie on one straight line through the point
        let points = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(20.0, 0.0),
        ];
        let curves = smooth(&points);
        let arriving = curves[0].handle_2;
        let leaving = curves[1].handle_1;
        let joint = points[1];
        let cross = (joint.x - arriving.x) * (leaving.y - joint.y)
            - (joint.y - arriving.y) * (leaving.x - joint.x);
        assert!(cross.abs() < 1e-4);
    }

    #[test]
    fn a_straight_stroke_stays_straight() {
        let points = [
            Point::new(0.0, 0.0),
            Point::new(5.0, 0.0),
            Point::new(10.0, 0.0),
        ];
        assert!(smooth(&points)
            .iter()
            .all(|curve| curve.handle_1.y == 0.0 && curve.handle_2.y == 0.0));
    }

    #[test]
    fn crowded_points_are_thinned_but_the_ends_stay() {
        let stroke = [
            Point::new(0.0, 0.0),
            Point::new(0.1, 0.0),
            Point::new(5.0, 0.0),
            Point::new(5.2, 0.0),
        ];
        assert_eq!(
            thin_out(&stroke),
            [Point::new(0.0, 0.0), Point::new(5.2, 0.0)]
        );
    }
}
