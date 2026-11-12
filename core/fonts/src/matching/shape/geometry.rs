//! plain polygon geometry for outlines: areas, boxes, inside tests, and
//! splitting quadratic curves at their extremes.

use super::*;

pub(crate) fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// the shoelace area of a closed polygon; positive when anticlockwise.
pub fn polygon_area(points: &[(f64, f64)]) -> f64 {
    let mut twice = 0.0;
    for (index, a) in points.iter().enumerate() {
        let b = points[(index + 1) % points.len()];
        twice += a.0 * b.1 - b.0 * a.1;
    }
    twice / 2.0
}

pub fn bounds_of(points: impl Iterator<Item = (f64, f64)>) -> Option<Bounds> {
    let mut bounds: Option<Bounds> = None;
    for (x, y) in points {
        let point = Bounds {
            x_min: x,
            y_min: y,
            x_max: x,
            y_max: y,
        };
        bounds = Some(bounds.map_or(point, |b| b.union(point)));
    }
    bounds
}

/// even-odd point in polygon test.
pub fn point_in_polygon(point: (f64, f64), polygon: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for &current in polygon {
        if (current.1 > point.1) != (previous.1 > point.1) {
            let x = previous.0
                + (point.1 - previous.1) * (current.0 - previous.0) / (current.1 - previous.1);
            if point.0 < x {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

/// a point just inside the polygon, next to its first long edge, used to
/// ask "which contours is this contour inside of?".
pub(crate) fn interior_probe(polygon: &[(f64, f64)]) -> (f64, f64) {
    let anticlockwise = polygon_area(polygon) > 0.0;
    for (index, a) in polygon.iter().enumerate() {
        let b = polygon[(index + 1) % polygon.len()];
        let length = distance(*a, b);
        if length < 1e-6 {
            continue;
        }
        // the inside is on the left of travel for an anticlockwise contour
        let (nx, ny) = (-(b.1 - a.1) / length, (b.0 - a.0) / length);
        let side = if anticlockwise { 1.0 } else { -1.0 };
        let middle = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        return (middle.0 + nx * side * 1e-4, middle.1 + ny * side * 1e-4);
    }
    polygon[0]
}

/// splits one contour's curves at their x and y turning points.
pub(crate) fn contour_with_extrema(contour: &Contour) -> Contour {
    let mut nodes = Vec::new();
    for piece in contour.pieces() {
        nodes.push(Node::on(piece.from.0, piece.from.1));
        let Some(control) = piece.control else {
            continue;
        };
        let mut cuts: Vec<f64> = [
            turning_point(piece.from.0, control.0, piece.to.0),
            turning_point(piece.from.1, control.1, piece.to.1),
        ]
        .into_iter()
        .flatten()
        .collect();
        cuts.sort_by(f64::total_cmp);
        let mut rest = (piece.from, control, piece.to);
        let mut done = 0.0;
        for cut in cuts {
            // the cut is measured on the whole piece; rescale it to the rest
            let local = (cut - done) / (1.0 - done);
            if !(0.001..0.999).contains(&local) {
                continue;
            }
            let (first, second) = split_quad(rest, local);
            nodes.push(Node::off(first.1 .0, first.1 .1));
            nodes.push(Node::on(first.2 .0, first.2 .1));
            rest = second;
            done = cut;
        }
        nodes.push(Node::off(rest.1 .0, rest.1 .1));
    }
    Contour { nodes }
}

pub(crate) type Quad = ((f64, f64), (f64, f64), (f64, f64));

/// where a quadratic's coordinate turns back, strictly inside the curve.
pub(crate) fn turning_point(a: f64, b: f64, c: f64) -> Option<f64> {
    let denominator = a - 2.0 * b + c;
    if denominator.abs() < 1e-12 {
        return None;
    }
    let t = (a - b) / denominator;
    (t > 0.0 && t < 1.0).then_some(t)
}

/// de casteljau's split of a quadratic at `t`.
pub(crate) fn split_quad((p0, p1, p2): Quad, t: f64) -> (Quad, Quad) {
    let lerp = |a: (f64, f64), b: (f64, f64)| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    let a = lerp(p0, p1);
    let b = lerp(p1, p2);
    let middle = lerp(a, b);
    ((p0, a, middle), (middle, b, p2))
}
