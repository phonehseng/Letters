//! glyph outlines as drawing commands, and their conversion to truetype
//! contours (recipe 3.3).
//!
//! word ignores embedded cff fonts and falls back to calibri (measured,
//! 16.0.20527, e1), so every cubic source (cff, type 1, type 3) is turned
//! into quadratic curves before it is embedded.

use crate::units::round_half_up;

/// one drawing command in font units, y up.
#[derive(Clone, Debug, PartialEq)]
pub enum PathOp {
    Move(f64, f64),
    Line(f64, f64),
    Quad(f64, f64, f64, f64),
    Cubic(f64, f64, f64, f64, f64, f64),
    Close,
}

/// a glyph's outline as recorded from its font program.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outline {
    pub ops: Vec<PathOp>,
}

impl Outline {
    pub fn is_empty(&self) -> bool {
        !self
            .ops
            .iter()
            .any(|op| !matches!(op, PathOp::Move(..) | PathOp::Close))
    }

    /// a fingerprint for finding the same glyph in two subsets of a face:
    /// the commands with coordinates in 1/1000 unit.
    pub fn signature(&self) -> Vec<i64> {
        let mut values = Vec::new();
        for op in &self.ops {
            let (tag, coordinates): (i64, Vec<f64>) = match op {
                PathOp::Move(x, y) => (1, vec![*x, *y]),
                PathOp::Line(x, y) => (2, vec![*x, *y]),
                PathOp::Quad(a, b, c, d) => (3, vec![*a, *b, *c, *d]),
                PathOp::Cubic(a, b, c, d, e, f) => (4, vec![*a, *b, *c, *d, *e, *f]),
                PathOp::Close => (5, Vec::new()),
            };
            values.push(tag);
            values.extend(coordinates.iter().map(|v| (v * 1000.0).round() as i64));
        }
        values
    }
}

/// records outlines from ttf-parser, which reads truetype and cff programs.
#[derive(Default)]
pub struct Recorder {
    pub outline: Outline,
}

impl ttf_parser::OutlineBuilder for Recorder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.outline
            .ops
            .push(PathOp::Move(f64::from(x), f64::from(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.outline
            .ops
            .push(PathOp::Line(f64::from(x), f64::from(y)));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.outline.ops.push(PathOp::Quad(
            f64::from(x1),
            f64::from(y1),
            f64::from(x),
            f64::from(y),
        ));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.outline.ops.push(PathOp::Cubic(
            f64::from(x1),
            f64::from(y1),
            f64::from(x2),
            f64::from(y2),
            f64::from(x),
            f64::from(y),
        ));
    }
    fn close(&mut self) {
        self.outline.ops.push(PathOp::Close);
    }
}

/// one truetype contour point in font units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContourPoint {
    pub x: i32,
    pub y: i32,
    pub on_curve: bool,
}

/// a truetype glyph: closed contours of on- and off-curve points.
pub type Contours = Vec<Vec<ContourPoint>>;

/// the largest distance between a cubic and the quadratic that shares its
/// end points and has the control point `(3 (c1 + c2) - (p0 + p3)) / 4`.
///
/// the difference of the two curves is `D t (t - 1/2) (t - 1)` with
/// `D = p3 - 3 c2 + 3 c1 - p0`, whose largest value on [0, 1] is
/// `√3/36 |D|`. splitting the cubic into n equal pieces divides D by n³.
fn single_quadratic_error(p0: (f64, f64), c1: (f64, f64), c2: (f64, f64), p3: (f64, f64)) -> f64 {
    let dx = p3.0 - 3.0 * c2.0 + 3.0 * c1.0 - p0.0;
    let dy = p3.1 - 3.0 * c2.1 + 3.0 * c1.1 - p0.1;
    3f64.sqrt() / 36.0 * dx.hypot(dy)
}

/// how many quadratic pieces keep a cubic within `tolerance`.
pub fn pieces_for(
    p0: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    p3: (f64, f64),
    tolerance: f64,
) -> usize {
    let error = single_quadratic_error(p0, c1, c2, p3);
    ((error / tolerance).cbrt().ceil() as usize).max(1)
}

fn point_on_cubic(
    p0: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    p3: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    (
        a * p0.0 + b * c1.0 + c * c2.0 + d * p3.0,
        a * p0.1 + b * c1.1 + c * c2.1 + d * p3.1,
    )
}

/// the control points of the piece of a cubic between t0 and t1.
fn cubic_piece(
    p0: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    p3: (f64, f64),
    t0: f64,
    t1: f64,
) -> [(f64, f64); 4] {
    // the derivative gives the inner control points of the sub-curve
    let derivative = |t: f64| {
        let u = 1.0 - t;
        (
            3.0 * (u * u * (c1.0 - p0.0) + 2.0 * u * t * (c2.0 - c1.0) + t * t * (p3.0 - c2.0)),
            3.0 * (u * u * (c1.1 - p0.1) + 2.0 * u * t * (c2.1 - c1.1) + t * t * (p3.1 - c2.1)),
        )
    };
    let start = point_on_cubic(p0, c1, c2, p3, t0);
    let end = point_on_cubic(p0, c1, c2, p3, t1);
    let span = t1 - t0;
    let (d0, d1) = (derivative(t0), derivative(t1));
    [
        start,
        (start.0 + d0.0 * span / 3.0, start.1 + d0.1 * span / 3.0),
        (end.0 - d1.0 * span / 3.0, end.1 - d1.1 * span / 3.0),
        end,
    ]
}

/// quadratic pieces `(control, end)` that follow a cubic within
/// `tolerance` font units (before the points are rounded to whole units).
pub fn cubic_to_quadratics(
    p0: (f64, f64),
    c1: (f64, f64),
    c2: (f64, f64),
    p3: (f64, f64),
    tolerance: f64,
) -> Vec<((f64, f64), (f64, f64))> {
    let pieces = pieces_for(p0, c1, c2, p3, tolerance);
    (0..pieces)
        .map(|i| {
            let t0 = i as f64 / pieces as f64;
            let t1 = (i + 1) as f64 / pieces as f64;
            let [a, b, c, d] = cubic_piece(p0, c1, c2, p3, t0, t1);
            let control = (
                (3.0 * (b.0 + c.0) - (a.0 + d.0)) / 4.0,
                (3.0 * (b.1 + c.1) - (a.1 + d.1)) / 4.0,
            );
            (control, d)
        })
        .collect()
}

/// turns an outline into truetype contours: scaled by `scale`, cubic
/// curves split into quadratics within `tolerance` (in the scaled units)
/// and, for cubic sources, every contour reversed (postscript outlines run
/// the other way round from truetype ones), then rounded to whole units.
pub fn to_contours(outline: &Outline, scale: f64, tolerance: f64, reverse: bool) -> Contours {
    let mut contours: Vec<Vec<(f64, f64, bool)>> = Vec::new();
    let mut current: Vec<(f64, f64, bool)> = Vec::new();
    let mut pen = (0.0, 0.0);
    let finish = |current: &mut Vec<(f64, f64, bool)>,
                  contours: &mut Vec<Vec<(f64, f64, bool)>>| {
        // a closing point on top of the start is the same point
        if current.len() > 1 {
            let (first, last) = (current[0], current[current.len() - 1]);
            if last.2 && (first.0 - last.0).abs() < 1e-9 && (first.1 - last.1).abs() < 1e-9 {
                current.pop();
            }
        }
        if current.len() > 1 {
            contours.push(std::mem::take(current));
        }
        current.clear();
    };
    for op in &outline.ops {
        match *op {
            PathOp::Move(x, y) => {
                finish(&mut current, &mut contours);
                pen = (x * scale, y * scale);
                current.push((pen.0, pen.1, true));
            }
            PathOp::Line(x, y) => {
                pen = (x * scale, y * scale);
                current.push((pen.0, pen.1, true));
            }
            PathOp::Quad(cx, cy, x, y) => {
                current.push((cx * scale, cy * scale, false));
                pen = (x * scale, y * scale);
                current.push((pen.0, pen.1, true));
            }
            PathOp::Cubic(x1, y1, x2, y2, x, y) => {
                let end = (x * scale, y * scale);
                for (control, point) in cubic_to_quadratics(
                    pen,
                    (x1 * scale, y1 * scale),
                    (x2 * scale, y2 * scale),
                    end,
                    tolerance,
                ) {
                    current.push((control.0, control.1, false));
                    current.push((point.0, point.1, true));
                }
                pen = end;
            }
            PathOp::Close => finish(&mut current, &mut contours),
        }
    }
    finish(&mut current, &mut contours);
    contours
        .into_iter()
        .map(|mut contour| {
            if reverse {
                // keep the first point first, walk the rest backwards
                contour[1..].reverse();
            }
            contour
                .into_iter()
                .map(|(x, y, on_curve)| ContourPoint {
                    x: round_half_up(x) as i32,
                    y: round_half_up(y) as i32,
                    on_curve,
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quadratic_at(p0: (f64, f64), q: (f64, f64), p1: (f64, f64), t: f64) -> (f64, f64) {
        let u = 1.0 - t;
        (
            u * u * p0.0 + 2.0 * u * t * q.0 + t * t * p1.0,
            u * u * p0.1 + 2.0 * u * t * q.1 + t * t * p1.1,
        )
    }

    /// the stated bound holds: sampled on every piece, the quadratics stay
    /// within the tolerance of the cubic at the same parameter.
    #[test]
    fn cubic_to_quadratic_stays_within_the_tolerance() {
        let cases = [
            ((0.0, 0.0), (0.0, 552.0), (448.0, 1000.0), (1000.0, 1000.0)),
            ((0.0, 0.0), (1000.0, 1000.0), (-1000.0, 1000.0), (0.0, 0.0)),
            ((10.0, 20.0), (300.0, -400.0), (700.0, 900.0), (950.0, 30.0)),
            ((0.0, 0.0), (1.0, 2.0), (3.0, 2.0), (4.0, 0.0)),
        ];
        for tolerance in [1.0, 0.25] {
            for (p0, c1, c2, p3) in cases {
                let pieces = cubic_to_quadratics(p0, c1, c2, p3, tolerance);
                let n = pieces.len();
                let mut start = p0;
                for (i, (control, end)) in pieces.iter().enumerate() {
                    for step in 0..=50 {
                        let s = step as f64 / 50.0;
                        let t = (i as f64 + s) / n as f64;
                        let on_cubic = point_on_cubic(p0, c1, c2, p3, t);
                        let on_quad = quadratic_at(start, *control, *end, s);
                        let distance = (on_cubic.0 - on_quad.0).hypot(on_cubic.1 - on_quad.1);
                        assert!(distance <= tolerance + 1e-9, "{distance} > {tolerance}");
                    }
                    start = *end;
                }
            }
        }
    }

    #[test]
    fn a_straight_cubic_needs_one_piece() {
        assert_eq!(
            pieces_for((0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0), 1.0),
            1
        );
    }

    #[test]
    fn contours_drop_the_repeated_closing_point_and_reverse() {
        let outline = Outline {
            ops: vec![
                PathOp::Move(0.0, 0.0),
                PathOp::Line(100.0, 0.0),
                PathOp::Line(100.0, 100.0),
                PathOp::Line(0.0, 0.0),
                PathOp::Close,
            ],
        };
        let contours = to_contours(&outline, 1.0, 1.0, true);
        let points: Vec<(i32, i32)> = contours[0].iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(points, [(0, 0), (100, 100), (100, 0)]);
    }

    #[test]
    fn empty_outlines_are_recognised() {
        assert!(Outline::default().is_empty());
        assert!(Outline {
            ops: vec![PathOp::Move(1.0, 1.0), PathOp::Close]
        }
        .is_empty());
    }
}
