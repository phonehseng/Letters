//! glyph outlines as plain geometry, in em units (1.0 is the em, y up,
//! the glyph's origin at 0,0), so fonts with any units per em compare
//! directly.
//!
//! every outline is kept the way TrueType keeps it: contours of points,
//! each on the curve or a quadratic control point off it. two control
//! points in a row have an implied on-curve point halfway between them.
//! cubic curves (CFF fonts, generated paths) are turned into quadratic
//! ones on the way in, so everything after that (transforms, offsetting,
//! writing a TrueType font) only ever handles one kind of curve.

use serde::{Deserialize, Serialize};

mod builder;
mod geometry;
#[cfg(test)]
pub(crate) mod tests;

pub use builder::*;
pub use geometry::*;

/// a point of an outline.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub x: f64,
    pub y: f64,
    /// false for a quadratic control point
    pub on: bool,
}

impl Node {
    pub fn on(x: f64, y: f64) -> Node {
        Node { x, y, on: true }
    }

    pub fn off(x: f64, y: f64) -> Node {
        Node { x, y, on: false }
    }
}

/// a closed contour. the last point joins back to the first.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Contour {
    pub nodes: Vec<Node>,
}

/// one glyph's outline: any number of closed contours.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub contours: Vec<Contour>,
}

/// a box: left, bottom, right, top.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}

impl Bounds {
    pub fn width(&self) -> f64 {
        self.x_max - self.x_min
    }

    pub fn height(&self) -> f64 {
        self.y_max - self.y_min
    }

    pub fn centre_x(&self) -> f64 {
        (self.x_min + self.x_max) / 2.0
    }

    pub fn union(self, other: Bounds) -> Bounds {
        Bounds {
            x_min: self.x_min.min(other.x_min),
            y_min: self.y_min.min(other.y_min),
            x_max: self.x_max.max(other.x_max),
            y_max: self.y_max.max(other.y_max),
        }
    }
}

/// one explicit piece of a contour: a straight line or a quadratic curve
/// from `from` to `to`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub from: (f64, f64),
    pub control: Option<(f64, f64)>,
    pub to: (f64, f64),
}

impl Piece {
    /// the point at `t` (0 to 1) along the piece.
    pub fn at(&self, t: f64) -> (f64, f64) {
        match self.control {
            None => (
                self.from.0 + (self.to.0 - self.from.0) * t,
                self.from.1 + (self.to.1 - self.from.1) * t,
            ),
            Some(c) => {
                let u = 1.0 - t;
                (
                    u * u * self.from.0 + 2.0 * u * t * c.0 + t * t * self.to.0,
                    u * u * self.from.1 + 2.0 * u * t * c.1 + t * t * self.to.1,
                )
            }
        }
    }
}

impl Contour {
    /// the contour as explicit lines and curves, with the implied
    /// on-curve points between control points filled in.
    pub fn pieces(&self) -> Vec<Piece> {
        let nodes = &self.nodes;
        if nodes.len() < 2 {
            return Vec::new();
        }
        // start at an on-curve point, or at the implied one between the
        // last and first control points when there is none
        let start_index = nodes.iter().position(|node| node.on);
        let (start, order): ((f64, f64), Vec<Node>) = match start_index {
            Some(at) => {
                let mut order: Vec<Node> = nodes[at + 1..].to_vec();
                order.extend_from_slice(&nodes[..=at]);
                ((nodes[at].x, nodes[at].y), order)
            }
            None => {
                let last = nodes[nodes.len() - 1];
                let first = nodes[0];
                (
                    ((last.x + first.x) / 2.0, (last.y + first.y) / 2.0),
                    nodes.to_vec(),
                )
            }
        };
        let mut pieces = Vec::new();
        let mut at = start;
        let mut control: Option<(f64, f64)> = None;
        for node in order
            .iter()
            .chain(std::iter::once(&Node::on(start.0, start.1)))
        {
            let point = (node.x, node.y);
            match (node.on, control) {
                (true, None) => {
                    pieces.push(Piece {
                        from: at,
                        control: None,
                        to: point,
                    });
                    at = point;
                }
                (true, Some(c)) => {
                    pieces.push(Piece {
                        from: at,
                        control: Some(c),
                        to: point,
                    });
                    at = point;
                    control = None;
                }
                (false, None) => control = Some(point),
                (false, Some(c)) => {
                    let middle = ((c.0 + point.0) / 2.0, (c.1 + point.1) / 2.0);
                    pieces.push(Piece {
                        from: at,
                        control: Some(c),
                        to: middle,
                    });
                    at = middle;
                    control = Some(point);
                }
            }
        }
        // the closing on-point was added twice when the start was a real
        // node; drop pieces of no length
        pieces.retain(|piece| piece.from != piece.to || piece.control.is_some());
        pieces
    }

    /// the contour cut into straight lines, curves into pieces no more
    /// than about `step` em long.
    pub fn flatten(&self, step: f64) -> Vec<(f64, f64)> {
        let mut points = Vec::new();
        for piece in self.pieces() {
            let steps = match piece.control {
                None => 1,
                Some(c) => {
                    let rough = distance(piece.from, c) + distance(c, piece.to);
                    ((rough / step.max(1e-4)).ceil() as usize).clamp(1, 64)
                }
            };
            for i in 0..steps {
                points.push(piece.at(i as f64 / steps as f64));
            }
        }
        points
    }

    /// the signed area: positive when the contour runs anticlockwise
    /// (with y up).
    pub fn signed_area(&self) -> f64 {
        polygon_area(&self.flatten(0.01))
    }

    pub fn reverse(&mut self) {
        self.nodes.reverse();
    }

    pub fn bounds(&self) -> Option<Bounds> {
        bounds_of(self.flatten(0.005).into_iter())
    }
}

impl Shape {
    pub fn is_empty(&self) -> bool {
        self.contours.iter().all(|contour| contour.nodes.len() < 2)
    }

    pub fn point_count(&self) -> usize {
        self.contours
            .iter()
            .map(|contour| contour.nodes.len())
            .sum()
    }

    /// the box around the ink (curves included, not just their points).
    pub fn bounds(&self) -> Option<Bounds> {
        self.contours
            .iter()
            .filter_map(Contour::bounds)
            .reduce(Bounds::union)
    }

    /// every contour cut into straight lines (closed polygons).
    pub fn polygons(&self, step: f64) -> Vec<Vec<(f64, f64)>> {
        self.contours
            .iter()
            .map(|contour| contour.flatten(step))
            .filter(|polygon| polygon.len() >= 2)
            .collect()
    }

    /// moves and scales every point: x' = x × sx + dx, y' = y × sy + dy.
    pub fn scaled(&self, sx: f64, sy: f64, dx: f64, dy: f64) -> Shape {
        self.map(|x, y| (x * sx + dx, y * sy + dy))
    }

    pub fn translated(&self, dx: f64, dy: f64) -> Shape {
        self.scaled(1.0, 1.0, dx, dy)
    }

    /// applies any point mapping to every point, on and off the curve.
    /// exact for affine mappings; for others (zoned vertical scaling) it
    /// is the usual close approximation.
    pub fn map(&self, mapping: impl Fn(f64, f64) -> (f64, f64)) -> Shape {
        Shape {
            contours: self
                .contours
                .iter()
                .map(|contour| Contour {
                    nodes: contour
                        .nodes
                        .iter()
                        .map(|node| {
                            let (x, y) = mapping(node.x, node.y);
                            Node { x, y, on: node.on }
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// slants the shape: x' = x + shear × y (a positive shear leans right).
    pub fn sheared(&self, shear: f64) -> Shape {
        self.map(|x, y| (x + shear * y, y))
    }

    /// the shapes of `self` and `other` together.
    pub fn merged(&self, other: &Shape) -> Shape {
        let mut contours = self.contours.clone();
        contours.extend(other.contours.iter().cloned());
        Shape { contours }
    }

    /// puts the shape in TrueType's direction: outer contours clockwise
    /// and holes anticlockwise (y up), so the filled side is always on
    /// the right. rasterisers and offsetting both rely on it.
    ///
    /// fonts already draw every contour one consistent way (TrueType
    /// clockwise, CFF the other way round), and variable fonts keep
    /// overlapping contours, so the whole shape is turned round when its
    /// total area says it runs the CFF way; contours are never judged
    /// one by one here (that would turn overlaps into holes).
    pub fn with_truetype_direction(&self) -> Shape {
        let total: f64 = self.contours.iter().map(Contour::signed_area).sum();
        let mut result = self.clone();
        if total > 0.0 {
            for contour in &mut result.contours {
                contour.reverse();
            }
        }
        result
    }

    /// sets each contour's direction from how deeply it is nested: outer
    /// contours clockwise, holes anticlockwise. only for outlines from
    /// outside a font (generated glyphs), which are checked to have no
    /// overlapping contours first.
    pub fn with_nesting_direction(&self) -> Shape {
        let polygons: Vec<Vec<(f64, f64)>> = self
            .contours
            .iter()
            .map(|contour| contour.flatten(0.01))
            .collect();
        let mut result = self.clone();
        for (index, contour) in result.contours.iter_mut().enumerate() {
            let polygon = &polygons[index];
            if polygon.len() < 3 {
                continue;
            }
            let probe = interior_probe(polygon);
            let depth = polygons
                .iter()
                .enumerate()
                .filter(|(other, other_polygon)| {
                    *other != index
                        && other_polygon.len() >= 3
                        && point_in_polygon(probe, other_polygon)
                })
                .count();
            let clockwise = polygon_area(polygon) < 0.0;
            let should_be_clockwise = depth % 2 == 0;
            if clockwise != should_be_clockwise {
                contour.reverse();
            }
        }
        result
    }

    /// starts each contour at its lowest, then leftmost, on-curve point,
    /// so two copies of one design list their points the same way.
    pub fn with_canonical_starts(&self) -> Shape {
        let mut result = self.clone();
        for contour in &mut result.contours {
            let best = contour
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, node)| node.on)
                .min_by(|(_, a), (_, b)| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)))
                .map(|(at, _)| at);
            if let Some(at) = best {
                contour.nodes.rotate_left(at);
            }
        }
        result
    }

    /// splits curves where they turn in x or y, so every horizontal and
    /// vertical extreme is an on-curve point. rasterisers without hinting
    /// draw cleaner stems and overshoots that way.
    pub fn with_extrema(&self) -> Shape {
        Shape {
            contours: self
                .contours
                .iter()
                .map(contour_with_extrema)
                .filter(|contour| contour.nodes.len() >= 2)
                .collect(),
        }
    }

    /// removes points that repeat the one before, and contours too small
    /// to see (less than a thousandth of the em across).
    pub fn cleaned(&self) -> Shape {
        let mut contours = Vec::new();
        for contour in &self.contours {
            let mut nodes: Vec<Node> = Vec::with_capacity(contour.nodes.len());
            for node in &contour.nodes {
                if !(node.x.is_finite() && node.y.is_finite()) {
                    continue;
                }
                if nodes.last().is_some_and(|last: &Node| {
                    (last.x - node.x).abs() < 1e-9 && (last.y - node.y).abs() < 1e-9
                }) {
                    // a repeated point: keep it on the curve if either was
                    let last = nodes.last_mut().expect("checked above");
                    last.on |= node.on;
                    continue;
                }
                nodes.push(*node);
            }
            while nodes.len() > 1 {
                let first = nodes[0];
                let last = nodes[nodes.len() - 1];
                if (first.x - last.x).abs() < 1e-9 && (first.y - last.y).abs() < 1e-9 {
                    nodes[0].on |= last.on;
                    nodes.pop();
                } else {
                    break;
                }
            }
            let cleaned = Contour { nodes };
            let big_enough = cleaned
                .bounds()
                .is_some_and(|b| b.width() + b.height() > 0.001);
            if cleaned.nodes.len() >= 2 && big_enough {
                contours.push(cleaned);
            }
        }
        Shape { contours }
    }

    /// the outline as svg-like path data on a grid of `grid` units per em
    /// (y up, as fonts are): "M x y L x y Q cx cy x y Z".
    pub fn to_path_data(&self, grid: f64) -> String {
        use std::fmt::Write;
        let round = |value: f64| (value * grid).round() as i64;
        let mut path = String::new();
        for contour in &self.contours {
            let pieces = contour.pieces();
            let Some(first) = pieces.first() else {
                continue;
            };
            let _ = write!(path, "M{} {}", round(first.from.0), round(first.from.1));
            for piece in &pieces {
                let _ = match piece.control {
                    None => write!(path, "L{} {}", round(piece.to.0), round(piece.to.1)),
                    Some(c) => write!(
                        path,
                        "Q{} {} {} {}",
                        round(c.0),
                        round(c.1),
                        round(piece.to.0),
                        round(piece.to.1)
                    ),
                };
            }
            path.push('Z');
        }
        path
    }
}
