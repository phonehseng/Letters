//! drawn shapes: rectangles (/Square), ellipses (/Circle), lines with
//! optional arrowheads (/Line), and polygons and open polylines.

use lopdf::{Dictionary, Document, Object};
use serde::{Deserialize, Serialize};

use crate::annots;
use crate::appearance::{Appearance, Canvas};
use crate::color::Color;
use crate::common::{self, Basics, Border};
use crate::error::AnnotateError;
use crate::geometry::{Point, Rect};
use crate::page_space::PageSpace;
use crate::pdf_values::{dict_numbers, dict_rect, name, number_array, resolve};
use crate::read::AnnotationInfo;

/// a rectangle or an ellipse filling a box.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShapeKind {
    Square,
    Circle,
}

fn default_stroke_width() -> f32 {
    1.0
}

/// how an outline is drawn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrokeStyle {
    pub color: Color,
    #[serde(default = "default_stroke_width")]
    pub width: f32,
    /// alternating dash and gap lengths in points; empty for a solid line
    #[serde(default)]
    pub dash: Vec<f32>,
}

/// what the app sends to draw a rectangle or an ellipse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeParams {
    pub kind: ShapeKind,
    /// the shape's outer edge, in visible page points
    pub rect: Rect,
    pub stroke: StrokeStyle,
    #[serde(default)]
    pub fill: Option<Color>,
    #[serde(default = "crate::full_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub contents: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

/// what can be drawn at either end of a line, using the pdf names.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineEnding {
    #[default]
    None,
    OpenArrow,
    ClosedArrow,
    Circle,
    Square,
}

impl LineEnding {
    pub(crate) fn pdf_name(self) -> &'static str {
        match self {
            LineEnding::None => "None",
            LineEnding::OpenArrow => "OpenArrow",
            LineEnding::ClosedArrow => "ClosedArrow",
            LineEnding::Circle => "Circle",
            LineEnding::Square => "Square",
        }
    }

    /// endings Letters doesn't draw (such as "Diamond") read as none.
    fn from_pdf_name(ending: &[u8]) -> LineEnding {
        match ending {
            b"OpenArrow" => LineEnding::OpenArrow,
            b"ClosedArrow" => LineEnding::ClosedArrow,
            b"Circle" => LineEnding::Circle,
            b"Square" => LineEnding::Square,
            _ => LineEnding::None,
        }
    }
}

/// what the app sends to draw a straight line or arrow.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineParams {
    pub start: Point,
    pub end: Point,
    pub stroke: StrokeStyle,
    #[serde(default)]
    pub start_ending: LineEnding,
    #[serde(default)]
    pub end_ending: LineEnding,
    /// the inside of closed arrowheads, circles and squares at the ends;
    /// none fills them with the line colour
    #[serde(default)]
    pub fill: Option<Color>,
    #[serde(default = "crate::full_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub contents: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

fn closed_by_default() -> bool {
    true
}

/// what the app sends to draw a polygon (closed) or polyline (open).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolygonParams {
    pub points: Vec<Point>,
    #[serde(default = "closed_by_default")]
    pub closed: bool,
    pub stroke: StrokeStyle,
    /// only polygons are filled
    #[serde(default)]
    pub fill: Option<Color>,
    #[serde(default = "crate::full_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub contents: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

/// draws a rectangle or ellipse on a page.
pub fn add_shape(
    doc: &mut Document,
    page: usize,
    params: &ShapeParams,
) -> Result<AnnotationInfo, AnnotateError> {
    let page = annots::page(doc, page)?;
    common::check_rect(params.rect)?;
    check_stroke(&params.stroke)?;
    let subtype = match params.kind {
        ShapeKind::Square => "Square",
        ShapeKind::Circle => "Circle",
    };
    let mut annotation = new_shape_dictionary(
        subtype,
        &page,
        &params.stroke,
        params.opacity,
        &params.contents,
        &params.author,
    );
    if let Some(fill) = params.fill {
        annotation.set("IC", fill.to_object());
    }
    annotation.set("Rect", number_array(&page.space.user_rect(params.rect)));
    common::add(doc, &page, annotation)
}

/// draws a line or arrow on a page.
pub fn add_line(
    doc: &mut Document,
    page: usize,
    params: &LineParams,
) -> Result<AnnotationInfo, AnnotateError> {
    let page = annots::page(doc, page)?;
    common::check_point(params.start)?;
    common::check_point(params.end)?;
    if params.start.distance(params.end) <= 0.0 {
        return Err(AnnotateError::EmptyArea);
    }
    check_stroke(&params.stroke)?;
    let mut annotation = new_shape_dictionary(
        "Line",
        &page,
        &params.stroke,
        params.opacity,
        &params.contents,
        &params.author,
    );
    write_line_ends(&mut annotation, &page.space, params.start, params.end);
    annotation.set(
        "LE",
        Object::Array(vec![
            name(params.start_ending.pdf_name()),
            name(params.end_ending.pdf_name()),
        ]),
    );
    if let Some(fill) = params.fill {
        annotation.set("IC", fill.to_object());
    }
    common::add(doc, &page, annotation)
}

/// draws a polygon or an open polyline on a page.
pub fn add_polygon(
    doc: &mut Document,
    page: usize,
    params: &PolygonParams,
) -> Result<AnnotationInfo, AnnotateError> {
    let page = annots::page(doc, page)?;
    let points: Vec<Point> = params
        .points
        .iter()
        .copied()
        .filter(|point| point.is_finite())
        .collect();
    let needed = if params.closed { 3 } else { 2 };
    if points.len() < needed || Rect::around(points.iter().copied()).is_none() {
        return Err(AnnotateError::EmptyArea);
    }
    check_stroke(&params.stroke)?;
    let subtype = if params.closed { "Polygon" } else { "PolyLine" };
    let mut annotation = new_shape_dictionary(
        subtype,
        &page,
        &params.stroke,
        params.opacity,
        &params.contents,
        &params.author,
    );
    write_vertices(&mut annotation, &page.space, &points);
    if let (true, Some(fill)) = (params.closed, params.fill) {
        annotation.set("IC", fill.to_object());
    }
    common::add(doc, &page, annotation)
}

fn check_stroke(stroke: &StrokeStyle) -> Result<(), AnnotateError> {
    common::check_stroke_width(stroke.width)
}

fn new_shape_dictionary(
    subtype: &str,
    page: &annots::PageContext,
    stroke: &StrokeStyle,
    opacity: f32,
    contents: &Option<String>,
    author: &Option<String>,
) -> Dictionary {
    let basics = Basics {
        color: Some(stroke.color),
        opacity,
        contents: contents.as_deref(),
        author: author.as_deref(),
    };
    let mut annotation = common::new_dictionary(subtype, page, &basics);
    common::set_border(
        &mut annotation,
        &Border {
            width: stroke.width,
            dash: common::clean_dash(stroke.dash.clone()),
        },
    );
    annotation
}

pub(crate) fn write_line_ends(
    annotation: &mut Dictionary,
    space: &PageSpace,
    start: Point,
    end: Point,
) {
    let (start, end) = (space.user_point(start), space.user_point(end));
    annotation.set("L", number_array(&[start.x, start.y, end.x, end.y]));
}

pub(crate) fn write_vertices(annotation: &mut Dictionary, space: &PageSpace, points: &[Point]) {
    let values: Vec<f32> = points
        .iter()
        .flat_map(|point| {
            let user = space.user_point(*point);
            [user.x, user.y]
        })
        .collect();
    annotation.set("Vertices", number_array(&values));
}

/// pairs of numbers as visible points.
pub(crate) fn visible_points(values: &[f32], space: &PageSpace) -> Vec<Point> {
    let (pairs, _) = values.as_chunks::<2>();
    pairs
        .iter()
        .map(|[x, y]| space.visible_point(Point::new(*x, *y)))
        .collect()
}

/// a line's two ends, in visible points.
pub(crate) fn line_ends(
    doc: &Document,
    annotation: &Dictionary,
    space: &PageSpace,
) -> Option<(Point, Point)> {
    let values = dict_numbers(doc, annotation, b"L")?;
    match visible_points(&values, space).as_slice() {
        [start, end, ..] => Some((*start, *end)),
        _ => None,
    }
}

/// what is drawn at the start and end of a line.
pub(crate) fn line_endings(doc: &Document, annotation: &Dictionary) -> (LineEnding, LineEnding) {
    let Some(names) = annotation
        .get(b"LE")
        .ok()
        .and_then(|endings| resolve(doc, endings).as_array().ok())
    else {
        return (LineEnding::None, LineEnding::None);
    };
    let ending = |index: usize| {
        names
            .get(index)
            .and_then(|ending| ending.as_name().ok())
            .map(LineEnding::from_pdf_name)
            .unwrap_or_default()
    };
    (ending(0), ending(1))
}

pub(crate) fn vertices(doc: &Document, annotation: &Dictionary, space: &PageSpace) -> Vec<Point> {
    let values = dict_numbers(doc, annotation, b"Vertices").unwrap_or_default();
    visible_points(&values, space)
}

/// the colour, border and fill every shape shares.
struct ShapeStyle {
    color: Option<Color>,
    fill: Option<Color>,
    border: Border,
    opacity: f32,
}

fn read_style(doc: &Document, annotation: &Dictionary) -> ShapeStyle {
    ShapeStyle {
        color: common::color(doc, annotation, b"C"),
        fill: common::color(doc, annotation, b"IC"),
        border: common::border(doc, annotation, 1.0),
        opacity: common::opacity(doc, annotation),
    }
}

/// sets up the canvas for a shape and says whether it has an outline.
fn begin(canvas: &mut Canvas, style: &ShapeStyle) -> bool {
    canvas.opacity(style.opacity, false);
    let outlined = style.color.is_some() && style.border.width > 0.0;
    if let Some(color) = style.color {
        canvas.stroke_color(color);
    }
    canvas.line_width(style.border.width);
    if !style.border.dash.is_empty() {
        canvas.dash(&style.border.dash);
    }
    if let Some(fill) = style.fill {
        canvas.fill_color(fill);
    }
    outlined
}

/// fills and/or strokes the path just built, as the style asks.
fn paint(canvas: &mut Canvas, outlined: bool, filled: bool) {
    match (filled, outlined) {
        (true, true) => canvas.fill_and_stroke(),
        (true, false) => canvas.fill(),
        (false, true) => canvas.stroke(),
        // a shape with neither is invisible, which pdf allows
        (false, false) => canvas.end_path(),
    }
}

/// draws a /Square or /Circle inside its /Rect.
pub(crate) fn draw_box_shape(
    doc: &Document,
    annotation: &Dictionary,
    space: &PageSpace,
) -> Result<Appearance, AnnotateError> {
    let user_rect = dict_rect(doc, annotation, b"Rect").ok_or(AnnotateError::EmptyArea)?;
    let frame = space.visible_rect(user_rect);
    if frame.is_empty() {
        return Err(AnnotateError::EmptyArea);
    }
    let style = read_style(doc, annotation);
    // /RD says how far inside /Rect other apps drew the shape
    let inner = match dict_numbers(doc, annotation, b"RD").as_deref() {
        Some([left, bottom, right, top]) => {
            let [l, b, r, t] = user_rect;
            space.visible_rect([l + left, b + bottom, r - right, t - top])
        }
        _ => frame,
    };
    // the outline is drawn centred on its path, so the path sits half a
    // line width inside the box to keep the whole line within it
    let half = style.border.width / 2.0;
    let path = inner.grow(-half.min(inner.width / 2.0).min(inner.height / 2.0));
    let mut canvas = Canvas::new(frame);
    let outlined = begin(&mut canvas, &style);
    if annots::subtype(doc, annotation) == "Circle" {
        canvas.ellipse(path);
    } else {
        canvas.rect(path);
    }
    paint(&mut canvas, outlined, style.fill.is_some());
    Ok(canvas.finish())
}

/// how long an arrowhead is for a line of the given width.
fn arrow_length(width: f32) -> f32 {
    4.0 * width + 5.0
}

/// draws a /Line with its endings.
pub(crate) fn draw_line(
    doc: &Document,
    annotation: &Dictionary,
    space: &PageSpace,
) -> Result<Appearance, AnnotateError> {
    let (start, end) = line_ends(doc, annotation, space).ok_or(AnnotateError::EmptyArea)?;
    let mut style = read_style(doc, annotation);
    // a line without a colour would be invisible; black is what readers assume
    style.color = style.color.or(Some(Color::BLACK));
    let (start_ending, end_ending) = line_endings(doc, annotation);
    let reach = style.border.width / 2.0 + arrow_length(style.border.width) + 1.0;
    let frame = Rect::around([start, end])
        .ok_or(AnnotateError::EmptyArea)?
        .grow(reach);

    let mut canvas = Canvas::new(frame);
    let outlined = begin(&mut canvas, &style);
    canvas.round_line_ends();
    canvas.move_to(start);
    canvas.line_to(end);
    canvas.stroke();
    // endings are always solid, even on a dashed line
    canvas.dash(&[]);
    let ending_fill = style.fill.or(style.color).unwrap_or(Color::BLACK);
    canvas.fill_color(ending_fill);
    draw_ending(
        &mut canvas,
        start_ending,
        start,
        end,
        style.border.width,
        outlined,
    );
    draw_ending(
        &mut canvas,
        end_ending,
        end,
        start,
        style.border.width,
        outlined,
    );
    Ok(canvas.finish())
}

/// draws one line ending at `tip`, pointing away from `other_end`.
fn draw_ending(
    canvas: &mut Canvas,
    ending: LineEnding,
    tip: Point,
    other_end: Point,
    width: f32,
    outlined: bool,
) {
    let length = arrow_length(width);
    let distance = tip.distance(other_end);
    if distance <= 0.0 {
        return;
    }
    // a unit step from the tip back along the line
    let (bx, by) = (
        (other_end.x - tip.x) / distance,
        (other_end.y - tip.y) / distance,
    );
    // the two sides of an arrowhead are 30 degrees either side of the line
    let (sin, cos) = (0.5_f32, 0.866_025_4_f32);
    let wing = |side: f32| {
        Point::new(
            tip.x + length * (bx * cos - by * sin * side),
            tip.y + length * (by * cos + bx * sin * side),
        )
    };
    let size = length / 2.0;
    match ending {
        LineEnding::None => {}
        LineEnding::OpenArrow => {
            canvas.polyline(&[wing(1.0), tip, wing(-1.0)], false);
            canvas.stroke();
        }
        LineEnding::ClosedArrow => {
            canvas.polyline(&[wing(1.0), tip, wing(-1.0)], true);
            paint(canvas, outlined, true);
        }
        LineEnding::Circle => {
            canvas.ellipse(Rect::new(
                tip.x - size / 2.0,
                tip.y - size / 2.0,
                size,
                size,
            ));
            paint(canvas, outlined, true);
        }
        LineEnding::Square => {
            canvas.rect(Rect::new(
                tip.x - size / 2.0,
                tip.y - size / 2.0,
                size,
                size,
            ));
            paint(canvas, outlined, true);
        }
    }
}

/// draws a /Polygon (closed, may be filled) or /PolyLine (open).
pub(crate) fn draw_polygon(
    doc: &Document,
    annotation: &Dictionary,
    space: &PageSpace,
) -> Result<Appearance, AnnotateError> {
    let points = vertices(doc, annotation, space);
    let closed = annots::subtype(doc, annotation) == "Polygon";
    let mut style = read_style(doc, annotation);
    style.color = style.color.or(Some(Color::BLACK));
    if !closed {
        style.fill = None;
    }
    let frame = Rect::around(points.iter().copied())
        .ok_or(AnnotateError::EmptyArea)?
        .grow(style.border.width / 2.0 + 1.0);
    let mut canvas = Canvas::new(frame);
    let outlined = begin(&mut canvas, &style);
    // round corners keep sharp angles from poking out of the frame
    canvas.round_line_ends();
    canvas.polyline(&points, closed);
    paint(&mut canvas, outlined, style.fill.is_some());
    Ok(canvas.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_endings_round_trip_by_name() {
        for ending in [
            LineEnding::None,
            LineEnding::OpenArrow,
            LineEnding::ClosedArrow,
            LineEnding::Circle,
            LineEnding::Square,
        ] {
            assert_eq!(
                LineEnding::from_pdf_name(ending.pdf_name().as_bytes()),
                ending
            );
        }
        assert_eq!(LineEnding::from_pdf_name(b"Diamond"), LineEnding::None);
    }

    #[test]
    fn arrowheads_grow_with_the_line() {
        assert!(arrow_length(4.0) > arrow_length(1.0));
    }
}
