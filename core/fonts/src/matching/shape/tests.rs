//! tests for outline geometry; `square` and `ring` are shared with other
//! matching tests.

use super::*;

/// a square from (x0, y0) to (x1, y1), drawn clockwise (y up).
pub fn square(x0: f64, y0: f64, x1: f64, y1: f64) -> Contour {
    Contour {
        nodes: vec![
            Node::on(x0, y0),
            Node::on(x0, y1),
            Node::on(x1, y1),
            Node::on(x1, y0),
        ],
    }
}

/// a ring like an "o": a clockwise outer square and an anticlockwise hole.
pub fn ring() -> Shape {
    let mut hole = square(0.2, 0.2, 0.4, 0.4);
    hole.reverse();
    Shape {
        contours: vec![square(0.1, 0.1, 0.5, 0.5), hole],
    }
}

#[test]
fn implied_points_sit_between_control_points() {
    let contour = Contour {
        nodes: vec![
            Node::on(0.0, 0.0),
            Node::off(0.0, 1.0),
            Node::off(1.0, 1.0),
            Node::on(1.0, 0.0),
        ],
    };
    let pieces = contour.pieces();
    assert_eq!(pieces.len(), 3);
    assert_eq!(pieces[0].to, (0.5, 1.0));
    assert_eq!(pieces[2].to, (0.0, 0.0));
}

#[test]
fn directions_are_made_truetype() {
    let mut wrong = ring();
    for contour in &mut wrong.contours {
        contour.reverse();
    }
    let fixed = wrong.with_truetype_direction();
    assert!(fixed.contours[0].signed_area() < 0.0, "outer is clockwise");
    assert!(
        fixed.contours[1].signed_area() > 0.0,
        "hole is anticlockwise"
    );
    assert_eq!(fixed, ring().with_truetype_direction());
    // a hole drawn the wrong way is found by its nesting
    let mut bad_hole = ring();
    bad_hole.contours[1].reverse();
    assert_eq!(bad_hole.with_nesting_direction(), ring());
}

#[test]
fn overlapping_contours_keep_their_direction() {
    // a stem and a crossbar drawn over it, both clockwise, as variable
    // fonts keep them: neither may become a hole
    let shape = Shape {
        contours: vec![square(0.0, 0.0, 0.1, 0.7), square(0.0, 0.4, 0.5, 0.5)],
    };
    let fixed = shape.with_truetype_direction();
    assert!(fixed.contours.iter().all(|c| c.signed_area() < 0.0));
}

#[test]
fn canonical_starts_ignore_where_a_contour_began() {
    let a = Shape {
        contours: vec![square(0.0, 0.0, 1.0, 1.0)],
    };
    let mut rotated = a.clone();
    rotated.contours[0].nodes.rotate_left(2);
    assert_ne!(a, rotated);
    assert_eq!(a.with_canonical_starts(), rotated.with_canonical_starts());
}

#[test]
fn extrema_become_points() {
    // a curve bulging up to y = 0.5 at its middle
    let contour = Contour {
        nodes: vec![Node::on(0.0, 0.0), Node::off(0.5, 1.0), Node::on(1.0, 0.0)],
    };
    let shape = Shape {
        contours: vec![contour],
    }
    .with_extrema();
    let top = shape.contours[0]
        .nodes
        .iter()
        .filter(|node| node.on)
        .map(|node| node.y)
        .fold(f64::MIN, f64::max);
    assert!((top - 0.5).abs() < 1e-9);
}

#[test]
fn cubics_become_close_quadratics() {
    let cubic = ((0.0, 0.0), (0.0, 0.55), (0.45, 1.0), (1.0, 1.0));
    let quads = cubic_to_quadratics(cubic, 0.0005);
    assert!(quads.len() >= 2);
    assert_eq!(quads.last().unwrap().1, (1.0, 1.0));
    // the middle of the cubic is on the converted curve
    let mut builder = ShapeBuilder::new(1.0);
    builder.move_to(0.0, 0.0);
    builder.cubic_to(0.0, 0.55, 0.45, 1.0, 1.0, 1.0);
    builder.line_to(1.0, 0.0);
    let shape = builder.finish();
    let points = shape.contours[0].flatten(0.001);
    // the cubic at t = 0.5: (p0 + 3 p1 + 3 p2 + p3) / 8
    let middle = ((3.0 * 0.45 + 1.0) / 8.0, (3.0 * 0.55 + 3.0 + 1.0) / 8.0);
    let nearest = points
        .iter()
        .map(|p| distance(*p, middle))
        .fold(f64::MAX, f64::min);
    assert!(nearest < 0.003, "{nearest}");
}

#[test]
fn path_data_is_compact() {
    let shape = Shape {
        contours: vec![square(0.0, 0.0, 0.5, 0.7)],
    };
    assert_eq!(shape.to_path_data(1000.0), "M0 0L0 700L500 700L500 0L0 0Z");
}

#[test]
fn cleaning_drops_repeats_and_specks() {
    let shape = Shape {
        contours: vec![
            Contour {
                nodes: vec![
                    Node::on(0.0, 0.0),
                    Node::on(0.0, 0.0),
                    Node::on(0.0, 1.0),
                    Node::on(1.0, 1.0),
                    Node::on(0.0, 0.0),
                ],
            },
            square(0.0, 0.0, 0.0001, 0.0001),
        ],
    };
    let cleaned = shape.cleaned();
    assert_eq!(cleaned.contours.len(), 1);
    assert_eq!(cleaned.contours[0].nodes.len(), 3);
}
