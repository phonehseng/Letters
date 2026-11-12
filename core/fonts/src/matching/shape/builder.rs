//! building shapes from drawing commands (ttf-parser's outlines, path
//! data), with cubic curves turned into close quadratic ones.

use super::*;

/// builds shapes from drawing commands, as ttf-parser and path data give
/// them. coordinates are multiplied by `scale` (1 / units per em).
pub struct ShapeBuilder {
    pub(crate) scale: f64,
    pub(crate) shape: Shape,
    pub(crate) current: Vec<Node>,
    pub(crate) at: (f64, f64),
    /// the largest error allowed when a cubic becomes quadratics, in em
    pub(crate) tolerance: f64,
}

impl ShapeBuilder {
    pub fn new(scale: f64) -> ShapeBuilder {
        ShapeBuilder {
            scale,
            shape: Shape::default(),
            current: Vec::new(),
            at: (0.0, 0.0),
            tolerance: 0.0005,
        }
    }

    pub fn finish(mut self) -> Shape {
        self.end_contour();
        self.shape
    }

    pub(crate) fn end_contour(&mut self) {
        if self.current.len() >= 2 {
            let mut nodes = std::mem::take(&mut self.current);
            // a contour that comes back to its start repeats the point
            let first = nodes[0];
            let last = nodes[nodes.len() - 1];
            if last.on && (first.x - last.x).abs() < 1e-12 && (first.y - last.y).abs() < 1e-12 {
                nodes.pop();
            }
            self.shape.contours.push(Contour { nodes });
        }
        self.current.clear();
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        self.end_contour();
        self.at = (x * self.scale, y * self.scale);
        self.current.push(Node::on(self.at.0, self.at.1));
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        self.at = (x * self.scale, y * self.scale);
        self.current.push(Node::on(self.at.0, self.at.1));
    }

    pub fn quad_to(&mut self, x1: f64, y1: f64, x: f64, y: f64) {
        self.current
            .push(Node::off(x1 * self.scale, y1 * self.scale));
        self.line_to(x, y);
    }

    pub fn cubic_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        let s = self.scale;
        let cubic = (self.at, (x1 * s, y1 * s), (x2 * s, y2 * s), (x * s, y * s));
        for (control, end) in cubic_to_quadratics(cubic, self.tolerance) {
            self.current.push(Node::off(control.0, control.1));
            self.current.push(Node::on(end.0, end.1));
        }
        self.at = (x * s, y * s);
    }

    pub fn close(&mut self) {
        self.end_contour();
    }
}

impl ttf_parser::OutlineBuilder for ShapeBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        ShapeBuilder::move_to(self, f64::from(x), f64::from(y));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        ShapeBuilder::line_to(self, f64::from(x), f64::from(y));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        ShapeBuilder::quad_to(
            self,
            f64::from(x1),
            f64::from(y1),
            f64::from(x),
            f64::from(y),
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        ShapeBuilder::cubic_to(
            self,
            f64::from(x1),
            f64::from(y1),
            f64::from(x2),
            f64::from(y2),
            f64::from(x),
            f64::from(y),
        );
    }
    fn close(&mut self) {
        ShapeBuilder::close(self);
    }
}

pub(crate) type Cubic = ((f64, f64), (f64, f64), (f64, f64), (f64, f64));

/// a cubic curve as quadratic pieces (control, end), split into more
/// pieces until each is within `tolerance` of the cubic.
pub fn cubic_to_quadratics(cubic: Cubic, tolerance: f64) -> Vec<((f64, f64), (f64, f64))> {
    let (p0, p1, p2, p3) = cubic;
    // the error of one quadratic for a cubic is at most sqrt(3)/36 times
    // the size of the cubic's third difference; split until small enough
    let third = (
        p3.0 - 3.0 * p2.0 + 3.0 * p1.0 - p0.0,
        p3.1 - 3.0 * p2.1 + 3.0 * p1.1 - p0.1,
    );
    let size = (third.0 * third.0 + third.1 * third.1).sqrt();
    let mut pieces = 1usize;
    while pieces < 32 && size * 3f64.sqrt() / 36.0 / (pieces as f64).powi(3) > tolerance {
        pieces += 1;
    }
    let point = |t: f64| {
        let u = 1.0 - t;
        (
            u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0,
            u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1,
        )
    };
    let derivative = |t: f64| {
        let u = 1.0 - t;
        (
            3.0 * (u * u * (p1.0 - p0.0) + 2.0 * u * t * (p2.0 - p1.0) + t * t * (p3.0 - p2.0)),
            3.0 * (u * u * (p1.1 - p0.1) + 2.0 * u * t * (p2.1 - p1.1) + t * t * (p3.1 - p2.1)),
        )
    };
    let mut result = Vec::with_capacity(pieces);
    for index in 0..pieces {
        let t0 = index as f64 / pieces as f64;
        let t1 = (index + 1) as f64 / pieces as f64;
        let (a, b) = (point(t0), point(t1));
        let (da, db) = (derivative(t0), derivative(t1));
        let h = t1 - t0;
        // the quadratic's control point: where the end tangents meet,
        // approximated by the average of both tangent estimates
        let from_start = (a.0 + da.0 * h / 2.0, a.1 + da.1 * h / 2.0);
        let from_end = (b.0 - db.0 * h / 2.0, b.1 - db.1 * h / 2.0);
        let control = (
            (from_start.0 + from_end.0) / 2.0,
            (from_start.1 + from_end.1) / 2.0,
        );
        result.push((control, b));
    }
    result
}
