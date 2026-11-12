//! paths while running content: construction operators are collected
//! until a painting operator ends the path, which is then recorded with
//! the colours, line style, opacity and clip in force.

use super::*;

impl Runner<'_> {
    /// path construction and painting. returns true when the operation
    /// was one of them.
    pub(super) fn path_operation(
        &mut self,
        source: usize,
        op: usize,
        operation: &Operation,
        numbers: &[f32],
        state: &mut GraphicsState,
    ) -> bool {
        let point = |x: f32, y: f32| Point::new(x, y);
        let segment = match (operation.operator.as_str(), numbers) {
            ("m", [x, y]) => Segment::Move(point(*x, *y)),
            ("l", [x, y]) => Segment::Line(point(*x, *y)),
            ("c", [x1, y1, x2, y2, x3, y3]) => {
                Segment::Curve(point(*x1, *y1), point(*x2, *y2), point(*x3, *y3))
            }
            // "v" starts its curve at the current point, "y" ends it there
            ("v", [x2, y2, x3, y3]) => {
                let current = self.current_point().unwrap_or(point(*x2, *y2));
                Segment::Curve(current, point(*x2, *y2), point(*x3, *y3))
            }
            ("y", [x1, y1, x3, y3]) => {
                Segment::Curve(point(*x1, *y1), point(*x3, *y3), point(*x3, *y3))
            }
            ("h", []) => Segment::Close,
            ("re", [x, y, w, h]) => Segment::Rect(point(*x, *y), *w, *h),
            ("W", _) => {
                self.pending.clip = true;
                return true;
            }
            ("W*", _) => {
                self.pending.clip = true;
                self.pending.even_odd_clip = true;
                return true;
            }
            ("sh", _) => {
                self.record_shading(source, op, state);
                return true;
            }
            (paint, _) => {
                return match paint_of(paint) {
                    Some(paint) => {
                        self.finish_path(source, op, paint, state);
                        true
                    }
                    None => false,
                };
            }
        };
        self.pending.first_op.get_or_insert(op);
        self.pending.segments.push(segment);
        true
    }

    pub(super) fn current_point(&self) -> Option<Point> {
        self.pending
            .segments
            .iter()
            .rev()
            .find_map(|segment| match segment {
                Segment::Move(at) | Segment::Line(at) | Segment::Curve(_, _, at) => Some(*at),
                Segment::Rect(at, _, _) => Some(*at),
                Segment::Close => None,
            })
    }

    /// ends the path being built with its painting operator.
    pub(super) fn finish_path(
        &mut self,
        source: usize,
        op: usize,
        mut paint: Paint,
        state: &mut GraphicsState,
    ) {
        let pending = std::mem::take(&mut self.pending);
        let Some(first_op) = pending.first_op else {
            return;
        };
        paint.clip = pending.clip;
        let bounds = page_bounds(&pending.segments, &state.ctm);
        if paint.clip {
            state.clip_bounds = match (state.clip_bounds, bounds) {
                (Some(old), Some(new)) => Some(intersect_bounds(old, new)),
                (None, new) => new,
                (old, None) => old,
            };
            state.clip_start = Some((source, first_op, state.block));
        }
        // a path that only clips (â€¦ W n) draws nothing; it is not an object
        if !paint.fill && !paint.stroke {
            return;
        }
        let clip_bounds = if paint.clip { None } else { state.clip_bounds };
        self.result.paths.push(PathRecord {
            source,
            first_op,
            paint_op: op,
            segments: pending.segments,
            paint,
            ctm: state.ctm,
            fill: state.text.fill.clone(),
            stroke: state.stroke.clone(),
            line: state.line.clone(),
            fill_alpha: state.fill_alpha,
            stroke_alpha: state.stroke_alpha,
            block: state.block,
            marked: self.open_marked.last().copied(),
            clip_bounds,
        });
    }

    /// a shading painted over the clip. when the clip was made in the
    /// same q block of the same stream, the clip path belongs to it.
    pub(super) fn record_shading(&mut self, source: usize, op: usize, state: &GraphicsState) {
        let first_op = match state.clip_start {
            Some((clip_source, first, block)) if clip_source == source && block == state.block => {
                first
            }
            _ => op,
        };
        self.result.paths.push(PathRecord {
            source,
            first_op,
            paint_op: op,
            segments: Vec::new(),
            paint: Paint {
                shading: true,
                ..Paint::default()
            },
            ctm: state.ctm,
            fill: state.text.fill.clone(),
            stroke: state.stroke.clone(),
            line: state.line.clone(),
            fill_alpha: state.fill_alpha,
            stroke_alpha: state.stroke_alpha,
            block: state.block,
            marked: self.open_marked.last().copied(),
            clip_bounds: state.clip_bounds,
        });
    }
}

/// what a painting operator does, or none for anything else.
fn paint_of(operator: &str) -> Option<Paint> {
    let (fill, stroke, even_odd) = match operator {
        "S" | "s" => (false, true, false),
        "f" | "F" => (true, false, false),
        "f*" => (true, false, true),
        "B" | "b" => (true, true, false),
        "B*" | "b*" => (true, true, true),
        "n" => (false, false, false),
        _ => return None,
    };
    Some(Paint {
        fill,
        stroke,
        even_odd,
        ..Paint::default()
    })
}

/// every point a path's pieces name (control points included).
pub(crate) fn segment_points(segments: &[Segment]) -> Vec<Point> {
    let mut points = Vec::new();
    for segment in segments {
        match *segment {
            Segment::Move(at) | Segment::Line(at) => points.push(at),
            Segment::Curve(a, b, c) => points.extend([a, b, c]),
            Segment::Rect(at, w, h) => points.extend([
                at,
                Point::new(at.x + w, at.y),
                Point::new(at.x, at.y + h),
                Point::new(at.x + w, at.y + h),
            ]),
            Segment::Close => {}
        }
    }
    points
}

/// a path's bounds in page space as [left, bottom, right, top].
pub(crate) fn page_bounds(segments: &[Segment], ctm: &Matrix) -> Option<[f32; 4]> {
    let points: Vec<Point> = segment_points(segments)
        .into_iter()
        .map(|point| ctm.apply(point))
        .collect();
    let first = points.first()?;
    Some(points.iter().fold(
        [first.x, first.y, first.x, first.y],
        |[l, b, r, t], point| {
            [
                l.min(point.x),
                b.min(point.y),
                r.max(point.x),
                t.max(point.y),
            ]
        },
    ))
}

fn intersect_bounds(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let left = a[0].max(b[0]);
    let bottom = a[1].max(b[1]);
    // an empty clip stays empty rather than turning inside out
    [
        left,
        bottom,
        a[2].min(b[2]).max(left),
        a[3].min(b[3]).max(bottom),
    ]
}
