//! points, rectangles, quadrilaterals and the matrices that move them.
//!
//! everything the app sends or receives is measured in points from the
//! top left corner of the page as the person sees it, with y going down.
//! the matrix type is used inside the crate to reach pdf space; it is a
//! copy of the one in letters-annotate so the two crates stay independent.

use serde::{Deserialize, Serialize};

/// a position on the page, in points from the visible top left corner.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub(crate) fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    pub(crate) fn minus(self, other: Point) -> Point {
        Point::new(self.x - other.x, self.y - other.y)
    }

    pub(crate) fn plus(self, other: Point) -> Point {
        Point::new(self.x + other.x, self.y + other.y)
    }

    pub(crate) fn scaled(self, factor: f32) -> Point {
        Point::new(self.x * factor, self.y * factor)
    }

    pub(crate) fn dot(self, other: Point) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub(crate) fn length(self) -> f32 {
        self.x.hypot(self.y)
    }

    /// the same direction with length one (or zero for no direction).
    pub(crate) fn unit(self) -> Point {
        let length = self.length();
        if length > 1e-9 {
            self.scaled(1.0 / length)
        } else {
            Point::new(1.0, 0.0)
        }
    }
}

/// an upright rectangle, in points from the visible top left corner.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// true for rectangles with no area or with numbers that aren't numbers.
    pub fn is_empty(&self) -> bool {
        let finite = [self.x, self.y, self.width, self.height]
            .iter()
            .all(|value| value.is_finite());
        !finite || self.width <= 0.0 || self.height <= 0.0
    }

    /// the smallest rectangle holding every point, or none for no points.
    pub(crate) fn around(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
        let mut points = points.into_iter();
        let first = points.next()?;
        let (mut left, mut top, mut right, mut bottom) = (first.x, first.y, first.x, first.y);
        for point in points {
            left = left.min(point.x);
            top = top.min(point.y);
            right = right.max(point.x);
            bottom = bottom.max(point.y);
        }
        Some(Rect::new(left, top, right - left, bottom - top))
    }

    pub fn union(&self, other: &Rect) -> Rect {
        Rect::around([
            Point::new(self.x, self.y),
            Point::new(self.right(), self.bottom()),
            Point::new(other.x, other.y),
            Point::new(other.right(), other.bottom()),
        ])
        .expect("four points")
    }

    /// top left, top right, bottom left, bottom right.
    pub fn corners(&self) -> [Point; 4] {
        [
            Point::new(self.x, self.y),
            Point::new(self.right(), self.y),
            Point::new(self.x, self.bottom()),
            Point::new(self.right(), self.bottom()),
        ]
    }
}

/// a 2d transform in pdf's own order: x' = a x + c y + e, y' = b x + d y + f.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    pub const fn new(a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        Self { a, b, c, d, e, f }
    }

    pub fn from_slice(values: &[f32]) -> Option<Matrix> {
        match values {
            [a, b, c, d, e, f] => Some(Matrix::new(*a, *b, *c, *d, *e, *f)),
            _ => None,
        }
    }

    pub fn translate(x: f32, y: f32) -> Matrix {
        Matrix::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    pub fn apply(&self, point: Point) -> Point {
        Point::new(
            self.a * point.x + self.c * point.y + self.e,
            self.b * point.x + self.d * point.y + self.f,
        )
    }

    /// moves a direction, ignoring the matrix's own move.
    pub fn apply_vector(&self, vector: Point) -> Point {
        Point::new(
            self.a * vector.x + self.c * vector.y,
            self.b * vector.x + self.d * vector.y,
        )
    }

    /// the transform that does `self` first and then `next`.
    pub fn then(&self, next: &Matrix) -> Matrix {
        Matrix::new(
            self.a * next.a + self.b * next.c,
            self.a * next.b + self.b * next.d,
            self.c * next.a + self.d * next.c,
            self.c * next.b + self.d * next.d,
            self.e * next.a + self.f * next.c + next.e,
            self.e * next.b + self.f * next.d + next.f,
        )
    }

    pub fn invert(&self) -> Option<Matrix> {
        let determinant = self.a * self.d - self.b * self.c;
        if determinant.abs() < 1e-12 || !determinant.is_finite() {
            return None;
        }
        let a = self.d / determinant;
        let b = -self.b / determinant;
        let c = -self.c / determinant;
        let d = self.a / determinant;
        let e = -(self.e * a + self.f * c);
        let f = -(self.e * b + self.f * d);
        Some(Matrix::new(a, b, c, d, e, f))
    }

    pub fn to_array(self) -> [f32; 6] {
        [self.a, self.b, self.c, self.d, self.e, self.f]
    }

    /// the matrix that maps the unit square onto a quadrilateral given as
    /// top left, top right and bottom left corners (in pdf space). this is
    /// how an image, which is drawn into the unit square, is placed.
    pub fn unit_square_to(top_left: Point, top_right: Point, bottom_left: Point) -> Matrix {
        let x_axis = top_right.minus(top_left);
        let y_axis = top_left.minus(bottom_left);
        Matrix::new(
            x_axis.x,
            x_axis.y,
            y_axis.x,
            y_axis.y,
            bottom_left.x,
            bottom_left.y,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        a.minus(b).length() < 1e-3
    }

    #[test]
    fn then_applies_the_first_matrix_first() {
        let scale = Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
        let shift = Matrix::translate(10.0, 0.0);
        let point = Point::new(1.0, 1.0);
        assert!(close(
            scale.then(&shift).apply(point),
            Point::new(12.0, 2.0)
        ));
        assert!(close(
            shift.then(&scale).apply(point),
            Point::new(22.0, 2.0)
        ));
    }

    #[test]
    fn inverting_undoes_the_transform() {
        let matrix = Matrix::new(0.0, 1.0, -1.0, 0.0, 30.0, -4.0);
        let point = Point::new(5.0, 7.0);
        assert!(close(
            matrix.invert().unwrap().apply(matrix.apply(point)),
            point
        ));
    }

    #[test]
    fn the_unit_square_lands_on_the_quad() {
        let matrix = Matrix::unit_square_to(
            Point::new(10.0, 50.0),
            Point::new(40.0, 50.0),
            Point::new(10.0, 20.0),
        );
        assert!(close(
            matrix.apply(Point::new(0.0, 0.0)),
            Point::new(10.0, 20.0)
        ));
        assert!(close(
            matrix.apply(Point::new(1.0, 1.0)),
            Point::new(40.0, 50.0)
        ));
    }
}
