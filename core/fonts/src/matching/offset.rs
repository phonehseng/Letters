//! making strokes heavier or lighter by moving the outline sideways.
//!
//! every point (on and off the curve) moves along the miter of the two
//! control-polygon edges beside it, as FreeType's emboldening does. with
//! TrueType direction (ink on the right of travel) "left" is always away
//! from the ink, for outer contours and counters alike, so one rule makes
//! the outside grow and the counters shrink. overlapping contours stay
//! correct because the fill rule is nonzero.
//!
//! the sideways and the up-and-down movement can differ: type designers
//! change the vertical stems far more than the hairlines and serifs from
//! one weight to the next (Times New Roman Bold's serifs are 18 ‰ of the
//! em thick, its stems 150 ‰), so a weight change moves vertical edges by
//! one amount and horizontal edges by another.
//!
//! thinning is capped below half the thinnest stroke, so a counter or a
//! stroke can never turn inside out.

use super::shape::{Contour, Node, Shape};

/// a miter longer than this many times the offset is cut short, so sharp
/// corners don't grow spikes.
const MITER_LIMIT: f64 = 3.0;

/// the shape with every stroke `amount` em heavier on each side (negative
/// for lighter). the shape must have TrueType direction.
pub fn offset_shape(shape: &Shape, amount: f64) -> Shape {
    offset_shape_xy(shape, amount, amount)
}

/// the same with separate amounts: `across` for vertical edges (stems),
/// `up` for horizontal ones (bars, hairlines, serifs).
pub fn offset_shape_xy(shape: &Shape, across: f64, up: f64) -> Shape {
    if across == 0.0 && up == 0.0 {
        return shape.clone();
    }
    Shape {
        contours: shape
            .contours
            .iter()
            .map(|contour| offset_contour(contour, across, up))
            .collect(),
    }
}

fn offset_contour(contour: &Contour, across: f64, up: f64) -> Contour {
    let nodes = &contour.nodes;
    let count = nodes.len();
    if count < 3 {
        return contour.clone();
    }
    let moved = (0..count)
        .map(|index| {
            let node = nodes[index];
            let before = previous_distinct(nodes, index);
            let after = next_distinct(nodes, index);
            let (mx, my) = miter(before, (node.x, node.y), after);
            Node {
                x: node.x + mx * across,
                y: node.y + my * up,
                on: node.on,
            }
        })
        .collect();
    Contour { nodes: moved }
}

fn previous_distinct(nodes: &[Node], index: usize) -> (f64, f64) {
    let here = nodes[index];
    for step in 1..nodes.len() {
        let other = nodes[(index + nodes.len() - step) % nodes.len()];
        if (other.x - here.x).abs() > 1e-9 || (other.y - here.y).abs() > 1e-9 {
            return (other.x, other.y);
        }
    }
    (here.x, here.y)
}

fn next_distinct(nodes: &[Node], index: usize) -> (f64, f64) {
    let here = nodes[index];
    for step in 1..nodes.len() {
        let other = nodes[(index + step) % nodes.len()];
        if (other.x - here.x).abs() > 1e-9 || (other.y - here.y).abs() > 1e-9 {
            return (other.x, other.y);
        }
    }
    (here.x, here.y)
}

/// the direction (scaled so each edge moves by one unit) that moves a
/// corner point outward: the sum of the edges' left normals divided by
/// one plus their dot product.
fn miter(before: (f64, f64), here: (f64, f64), after: (f64, f64)) -> (f64, f64) {
    let left_normal = |a: (f64, f64), b: (f64, f64)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length = (dx * dx + dy * dy).sqrt();
        if length < 1e-12 {
            (0.0, 0.0)
        } else {
            (-dy / length, dx / length)
        }
    };
    let n1 = left_normal(before, here);
    let n2 = left_normal(here, after);
    let dot = n1.0 * n2.0 + n1.1 * n2.1;
    let sum = (n1.0 + n2.0, n1.1 + n2.1);
    if 1.0 + dot < 1e-6 {
        // the contour doubles back on itself here: move along either edge
        return n1;
    }
    let scale = (1.0 / (1.0 + dot)).min(MITER_LIMIT * MITER_LIMIT / 2.0);
    let mut m = (sum.0 * scale, sum.1 * scale);
    let length = (m.0 * m.0 + m.1 * m.1).sqrt();
    if length > MITER_LIMIT {
        m = (m.0 / length * MITER_LIMIT, m.1 / length * MITER_LIMIT);
    }
    m
}

/// the most strokes `stroke` em thick may be thinned by on each side, so
/// none vanishes or turns inside out (0.35 a side leaves 30% of it).
pub fn thinning_limit(stroke: f64) -> f64 {
    (stroke * 0.35).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matching::shape::tests::{ring, square};
    use crate::matching::style::horizontal_spans;

    #[test]
    fn emboldening_grows_ink_and_shrinks_counters() {
        let heavier = offset_shape(&ring().with_truetype_direction(), 0.02);
        let spans = horizontal_spans(&heavier, 0.3);
        // the outer edge moved out and the hole's edge moved in
        assert_eq!(spans.len(), 2);
        assert!((spans[0].0 - 0.08).abs() < 1e-9, "{spans:?}");
        assert!((spans[0].1 - 0.22).abs() < 1e-9, "{spans:?}");
        // the corners moved diagonally by the miter, staying square
        let outer = &heavier.contours[0];
        assert!(outer
            .nodes
            .iter()
            .any(|n| (n.x - 0.08).abs() < 1e-9 && (n.y - 0.08).abs() < 1e-9));
    }

    #[test]
    fn stems_and_bars_can_change_by_different_amounts() {
        let shape = Shape {
            contours: vec![square(0.0, 0.0, 0.1, 0.7)],
        };
        let changed = offset_shape_xy(&shape, 0.02, 0.005);
        let bounds = changed.bounds().unwrap();
        assert!((bounds.width() - 0.14).abs() < 1e-9, "{bounds:?}");
        assert!((bounds.height() - 0.71).abs() < 1e-9, "{bounds:?}");
    }

    #[test]
    fn thinning_is_the_reverse() {
        let shape = Shape {
            contours: vec![square(0.0, 0.0, 0.1, 0.7)],
        };
        let thinner = offset_shape(&shape, -0.01);
        let spans = horizontal_spans(&thinner, 0.3);
        assert!((spans[0].1 - spans[0].0 - 0.08).abs() < 1e-9);
        assert!(thinning_limit(0.1) < 0.05);
    }

    #[test]
    fn curves_offset_close_to_a_true_offset() {
        // a quarter-circle-ish bowl: control polygon offsetting keeps the
        // curve close to the exact offset
        let contour = Contour {
            nodes: vec![
                Node::on(0.0, 0.0),
                Node::off(0.0, 0.5),
                Node::on(0.5, 0.5),
                Node::on(0.5, 0.0),
            ],
        };
        let shape = Shape {
            contours: vec![contour],
        }
        .with_truetype_direction();
        let heavier = offset_shape(&shape, 0.02);
        let before = shape.bounds().unwrap();
        let after = heavier.bounds().unwrap();
        assert!((after.width() - before.width() - 0.04).abs() < 0.004);
    }
}
