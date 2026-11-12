//! vector content: simple paths become drawingml shapes, the rest is
//! rasterised (recipe 10).
//!
//! simple means what word can draw exactly: axis-aligned rectangles
//! (filled or stroked) and straight stroked segments, solid colour, no
//! dash, no transparency, no clip that cuts them. everything else (curves,
//! dashes, shadings, patterns, transparency, real clipping, filled
//! polygons) is drawn from a page that holds nothing but those items, so no
//! text ever ends up as pixels.

use crate::pdf::content::Point;
use crate::pdf::content::{PathItem, Segment};

const AXIS_TOLERANCE: f64 = 0.01;
/// a clip may reach this far into a path and still count as harmless
const CLIP_MARGIN_PT: f64 = 0.5;

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect {
        left: f64,
        bottom: f64,
        right: f64,
        top: f64,
        fill: Option<[f64; 3]>,
        stroke: Option<[f64; 3]>,
        line_width: f64,
        sequence: usize,
    },
    Line {
        start: Point,
        end: Point,
        color: [f64; 3],
        line_width: f64,
        sequence: usize,
    },
}

/// splits a path where a new subpath starts.
fn subpaths(segments: &[Segment]) -> Vec<Vec<Segment>> {
    let mut found: Vec<Vec<Segment>> = Vec::new();
    let mut current: Vec<Segment> = Vec::new();
    for segment in segments {
        if matches!(segment, Segment::Move(_) | Segment::Rect(_)) && !current.is_empty() {
            found.push(std::mem::take(&mut current));
        }
        current.push(segment.clone());
    }
    if !current.is_empty() {
        found.push(current);
    }
    found
}

fn round3(value: f64) -> i64 {
    (value * 1000.0).round_ties_even() as i64
}

/// an axis-aligned rectangle (left, bottom, right, top), if the subpath is one.
fn as_rect(subpath: &[Segment]) -> Option<(f64, f64, f64, f64)> {
    let points: Vec<Point> = if let [Segment::Rect(corners)] = subpath {
        corners.to_vec()
    } else {
        if subpath
            .iter()
            .any(|s| !matches!(s, Segment::Move(_) | Segment::Line(_) | Segment::Close))
        {
            return None;
        }
        let mut points: Vec<Point> = subpath.iter().flat_map(Segment::points).collect();
        if points.len() == 5
            && (points[0].0 - points[4].0).abs() < AXIS_TOLERANCE
            && (points[0].1 - points[4].1).abs() < AXIS_TOLERANCE
        {
            points.truncate(4);
        }
        if points.len() != 4 {
            return None;
        }
        points
    };
    let mut xs: Vec<i64> = points.iter().map(|p| round3(p.0)).collect();
    let mut ys: Vec<i64> = points.iter().map(|p| round3(p.1)).collect();
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();
    if xs.len() > 2 || ys.len() > 2 {
        return None; // rotated or skewed
    }
    let to_pt = |v: i64| v as f64 / 1000.0;
    Some((
        to_pt(xs[0]),
        to_pt(ys[0]),
        to_pt(*xs.last()?),
        to_pt(*ys.last()?),
    ))
}

/// every clip a single rectangle that contains the path within 0.5 pt.
fn clip_is_harmless(item: &PathItem) -> bool {
    let (left, bottom, right, top) = item.bbox();
    item.clip.iter().all(|clip| {
        let pieces = subpaths(clip);
        let [piece] = pieces.as_slice() else {
            return false;
        };
        match as_rect(piece) {
            Some((cl, cb, cr, ct)) => {
                left >= cl - CLIP_MARGIN_PT
                    && bottom >= cb - CLIP_MARGIN_PT
                    && right <= cr + CLIP_MARGIN_PT
                    && top <= ct + CLIP_MARGIN_PT
            }
            None => false,
        }
    })
}

/// drawingml shapes for a simple path, or `None` if it must be rasterised.
pub fn classify(item: &PathItem) -> Option<Vec<Shape>> {
    if item.has_curves()
        || item.dashed
        || item.alpha < 1.0
        || item.pattern
        || !clip_is_harmless(item)
    {
        return None;
    }
    let mut shapes = Vec::new();
    for subpath in subpaths(&item.segments) {
        let closed = subpath
            .iter()
            .any(|s| matches!(s, Segment::Close | Segment::Rect(_)));
        let rect = if item.fill || closed {
            as_rect(&subpath)
        } else {
            None
        };
        if let Some((left, bottom, right, top)) = rect {
            shapes.push(Shape::Rect {
                left,
                bottom,
                right,
                top,
                fill: item.fill_color,
                stroke: item.stroke_color,
                line_width: item.line_width,
                sequence: item.sequence,
            });
            continue;
        }
        if item.fill {
            return None; // a filled polygon: kept exact as pixels rather than guessed
        }
        if subpath
            .iter()
            .any(|s| !matches!(s, Segment::Move(_) | Segment::Line(_)))
        {
            return None;
        }
        let points: Vec<Point> = subpath.iter().flat_map(Segment::points).collect();
        if points.len() < 2 {
            return None;
        }
        for pair in points.windows(2) {
            shapes.push(Shape::Line {
                start: pair[0],
                end: pair[1],
                color: item.stroke_color.unwrap_or([0.0; 3]),
                line_width: item.line_width,
                sequence: item.sequence,
            });
        }
    }
    Some(shapes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(segments: Vec<Segment>, fill: bool, stroke: bool, clip: Vec<Vec<Segment>>) -> PathItem {
        PathItem {
            page: 0,
            segments,
            fill,
            stroke,
            even_odd: false,
            fill_color: fill.then_some([0.0; 3]),
            stroke_color: stroke.then_some([0.0; 3]),
            line_width: 1.0,
            dashed: false,
            alpha: 1.0,
            clip,
            pattern: false,
            raw: Vec::new(),
            sequence: 1,
        }
    }

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Segment {
        Segment::Rect([(x0, y0), (x1, y0), (x1, y1), (x0, y1)])
    }

    // the prototype's cases (test_prototype.py)
    #[test]
    fn rectangle_becomes_a_rect_shape() {
        let shapes = classify(&item(
            vec![rect(10.0, 10.0, 60.0, 20.0)],
            true,
            false,
            Vec::new(),
        ))
        .unwrap();
        assert_eq!(shapes.len(), 1);
        assert!(matches!(shapes[0], Shape::Rect { .. }));
    }

    #[test]
    fn curve_must_be_rasterised() {
        let segments = vec![
            Segment::Move((0.0, 0.0)),
            Segment::Curve((1.0, 1.0), (2.0, 2.0), (3.0, 0.0)),
        ];
        assert!(classify(&item(segments, true, false, Vec::new())).is_none());
    }

    #[test]
    fn stroked_polyline_becomes_segments() {
        let segments = vec![
            Segment::Move((0.0, 0.0)),
            Segment::Line((10.0, 0.0)),
            Segment::Line((10.0, 10.0)),
        ];
        assert_eq!(
            classify(&item(segments, false, true, Vec::new()))
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn clip_that_cuts_the_path_forces_a_raster() {
        let clip = vec![vec![rect(0.0, 0.0, 20.0, 20.0)]];
        assert!(classify(&item(vec![rect(10.0, 10.0, 60.0, 20.0)], true, false, clip)).is_none());
    }

    #[test]
    fn filled_triangle_is_rasterised() {
        let segments = vec![
            Segment::Move((0.0, 0.0)),
            Segment::Line((10.0, 0.0)),
            Segment::Line((5.0, 8.0)),
            Segment::Close,
        ];
        assert!(classify(&item(segments, true, false, Vec::new())).is_none());
    }
}
