//! text markup: highlight, underline, strike-out and squiggly underline.
//!
//! the app sends one rectangle per line of selected text. each becomes
//! one quadrilateral in /QuadPoints, which is how pdf records exactly
//! which words are marked, even on a rotated page.

use lopdf::{Dictionary, Document, Object};
use serde::{Deserialize, Serialize};

use crate::annots;
use crate::appearance::{Appearance, Canvas};
use crate::color::Color;
use crate::common::{self, Basics};
use crate::error::AnnotateError;
use crate::geometry::{Point, Rect};
use crate::page_space::PageSpace;
use crate::pdf_values::{dict_numbers, number_array};
use crate::read::AnnotationInfo;

/// the four ways of marking text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextMarkupKind {
    Highlight,
    Underline,
    StrikeOut,
    Squiggly,
}

impl TextMarkupKind {
    pub(crate) fn subtype(self) -> &'static str {
        match self {
            TextMarkupKind::Highlight => "Highlight",
            TextMarkupKind::Underline => "Underline",
            TextMarkupKind::StrikeOut => "StrikeOut",
            TextMarkupKind::Squiggly => "Squiggly",
        }
    }

    pub(crate) fn from_subtype(subtype: &str) -> Option<Self> {
        match subtype {
            "Highlight" => Some(TextMarkupKind::Highlight),
            "Underline" => Some(TextMarkupKind::Underline),
            "StrikeOut" => Some(TextMarkupKind::StrikeOut),
            "Squiggly" => Some(TextMarkupKind::Squiggly),
            _ => None,
        }
    }
}

/// what the app sends to mark some text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextMarkupParams {
    pub kind: TextMarkupKind,
    /// one rectangle per line of selected text, in visible page points
    pub rects: Vec<Rect>,
    pub color: Color,
    #[serde(default = "crate::full_opacity")]
    pub opacity: f32,
    /// an optional comment shown with the mark
    #[serde(default)]
    pub contents: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

/// marks text on a page. empty rectangles are ignored; if none are left
/// the mark is refused, because a mark over nothing can't be seen or
/// selected.
pub fn add_text_markup(
    doc: &mut Document,
    page: usize,
    params: &TextMarkupParams,
) -> Result<AnnotationInfo, AnnotateError> {
    let page = annots::page(doc, page)?;
    let rects: Vec<Rect> = params
        .rects
        .iter()
        .filter(|rect| !rect.is_empty())
        .copied()
        .collect();
    if rects.is_empty() {
        return Err(AnnotateError::EmptyArea);
    }
    let basics = Basics {
        color: Some(params.color),
        opacity: params.opacity,
        contents: params.contents.as_deref(),
        author: params.author.as_deref(),
    };
    let mut annotation = common::new_dictionary(params.kind.subtype(), &page, &basics);
    let quads: Vec<[Point; 4]> = rects.iter().map(Rect::corners).collect();
    annotation.set("QuadPoints", quad_points(&quads, &page.space));
    common::add(doc, &page, annotation)
}

/// quadrilaterals (visible corners in top left, top right, bottom left,
/// bottom right order) as a /QuadPoints array in user space.
///
/// the standard describes a counter-clockwise order, but acrobat and
/// every major reader use this one, so files from Letters match theirs.
pub(crate) fn quad_points(quads: &[[Point; 4]], space: &PageSpace) -> Object {
    let values: Vec<f32> = quads
        .iter()
        .flat_map(|quad| quad.iter())
        .flat_map(|corner| {
            let user = space.user_point(*corner);
            [user.x, user.y]
        })
        .collect();
    number_array(&values)
}

/// the marked quadrilaterals, each as four visible corners in the order
/// top left, top right, bottom left, bottom right.
pub(crate) fn quads(doc: &Document, annotation: &Dictionary, space: &PageSpace) -> Vec<[Point; 4]> {
    let values = dict_numbers(doc, annotation, b"QuadPoints").unwrap_or_default();
    let (quads, _) = values.as_chunks::<8>();
    quads
        .iter()
        .map(|quad| {
            [0, 2, 4, 6].map(|start| space.visible_point(Point::new(quad[start], quad[start + 1])))
        })
        .map(upright_order)
        .collect()
}

/// puts a quadrilateral's corners in top left, top right, bottom left,
/// bottom right order as seen on screen. other apps write the corners
/// in different orders, and the page may be rotated.
fn upright_order(corners: [Point; 4]) -> [Point; 4] {
    let mut by_height = corners;
    by_height.sort_by(|a, b| a.y.total_cmp(&b.y));
    let (mut top, mut bottom) = ([by_height[0], by_height[1]], [by_height[2], by_height[3]]);
    top.sort_by(|a, b| a.x.total_cmp(&b.x));
    bottom.sort_by(|a, b| a.x.total_cmp(&b.x));
    [top[0], top[1], bottom[0], bottom[1]]
}

/// draws the mark over each quadrilateral.
pub(crate) fn draw(
    doc: &Document,
    annotation: &Dictionary,
    space: &PageSpace,
) -> Result<Appearance, AnnotateError> {
    let subtype = annots::subtype(doc, annotation);
    let kind = TextMarkupKind::from_subtype(&subtype).ok_or(AnnotateError::CannotChange)?;
    let quads = quads(doc, annotation, space);
    let frame = Rect::around(quads.iter().flatten().copied())
        .ok_or(AnnotateError::EmptyArea)?
        // a little room so line ends and wave peaks aren't clipped
        .grow(1.0);
    let color = common::color(doc, annotation, b"C").unwrap_or(Color::HIGHLIGHT_YELLOW);
    let opacity = common::opacity(doc, annotation);

    let mut canvas = Canvas::new(frame);
    match kind {
        TextMarkupKind::Highlight => {
            canvas.opacity(opacity, true);
            canvas.fill_color(color);
            for quad in &quads {
                let [top_left, top_right, bottom_left, bottom_right] = *quad;
                canvas.polyline(&[top_left, top_right, bottom_right, bottom_left], true);
            }
            canvas.fill();
        }
        _ => {
            canvas.opacity(opacity, false);
            canvas.stroke_color(color);
            for quad in &quads {
                draw_line_mark(&mut canvas, kind, quad);
            }
        }
    }
    Ok(canvas.finish())
}

/// how far down a line of text (0 is the top of the selection, 1 the
/// bottom) each mark is drawn. selections run from the tops of the tall
/// letters to below the baseline, so the baseline sits near 0.85 and the
/// middle of the small letters near 0.6.
const UNDERLINE_DEPTH: f32 = 0.88;
const STRIKE_OUT_DEPTH: f32 = 0.58;

/// draws an underline, strike-out or squiggle along one quadrilateral.
fn draw_line_mark(canvas: &mut Canvas, kind: TextMarkupKind, quad: &[Point; 4]) {
    let [top_left, top_right, bottom_left, bottom_right] = *quad;
    let line_height = top_left.distance(bottom_left);
    let thickness = (line_height / 14.0).max(0.5);
    canvas.line_width(thickness);
    let along = |depth: f32| {
        (
            top_left.lerp(bottom_left, depth),
            top_right.lerp(bottom_right, depth),
        )
    };
    match kind {
        TextMarkupKind::StrikeOut => {
            let (start, end) = along(STRIKE_OUT_DEPTH);
            canvas.move_to(start);
            canvas.line_to(end);
        }
        TextMarkupKind::Squiggly => {
            let (start, end) = along(UNDERLINE_DEPTH);
            draw_wave(canvas, start, end, line_height / 14.0 + 0.5);
        }
        _ => {
            let (start, end) = along(UNDERLINE_DEPTH);
            canvas.move_to(start);
            canvas.line_to(end);
        }
    }
    canvas.stroke();
}

/// a zigzag from `start` to `end`, rising and falling by `height`.
fn draw_wave(canvas: &mut Canvas, start: Point, end: Point, height: f32) {
    let length = start.distance(end);
    if length <= 0.0 {
        return;
    }
    // one rise or fall every few points, whatever the line's direction
    let step = (height * 2.0).max(1.0);
    let steps = (length / step).ceil().max(1.0) as usize;
    let (dx, dy) = ((end.x - start.x) / length, (end.y - start.y) / length);
    // "up" on screen, at right angles to the line
    let (ux, uy) = (dy, -dx);
    canvas.move_to(start);
    for index in 1..=steps {
        let base = start.lerp(end, index as f32 / steps as f32);
        let lift = if index % 2 == 1 { height } else { 0.0 };
        canvas.line_to(Point::new(base.x + ux * lift, base.y + uy * lift));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corners_are_sorted_whatever_order_they_came_in() {
        let corners = [
            Point::new(0.0, 10.0),
            Point::new(5.0, 0.0),
            Point::new(5.0, 10.0),
            Point::new(0.0, 0.0),
        ];
        assert_eq!(
            upright_order(corners),
            [
                Point::new(0.0, 0.0),
                Point::new(5.0, 0.0),
                Point::new(0.0, 10.0),
                Point::new(5.0, 10.0),
            ]
        );
    }

    #[test]
    fn quad_points_follow_the_page_rotation() {
        let space = PageSpace::new([0.0, 0.0, 612.0, 792.0], 90);
        let rect = Rect::new(10.0, 20.0, 100.0, 12.0);
        let values =
            crate::pdf_values::numbers(&Document::new(), &quad_points(&[rect.corners()], &space))
                .unwrap();
        // the visible top left (10, 20) is user (20, 10) on a page turned a quarter
        assert_eq!(&values[0..2], &[20.0, 10.0]);
        assert_eq!(values.len(), 8);
    }
}
