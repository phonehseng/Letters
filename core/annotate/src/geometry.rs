//! points, rectangles and the matrices that move them around.
//!
//! everything the app sends or receives is measured in points from the
//! top left corner of the page as the person sees it, with y going down.
//! the matrix type is only used inside the crate to reach pdf space.

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

    /// the point a fraction `t` of the way from `self` to `other`.
    pub(crate) fn lerp(self, other: Point, t: f32) -> Point {
        Point::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }

    pub(crate) fn distance(self, other: Point) -> f32 {
        (other.x - self.x).hypot(other.y - self.y)
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

    /// the same rectangle made bigger by `amount` on every side.
    pub(crate) fn grow(&self, amount: f32) -> Rect {
        Rect::new(
            self.x - amount,
            self.y - amount,
            self.width + 2.0 * amount,
            self.height + 2.0 * amount,
        )
    }

    /// top left, top right, bottom left, bottom right: the order pdf
    /// viewers expect for one quadrilateral of /QuadPoints.
    pub(crate) fn corners(&self) -> [Point; 4] {
        [
            Point::new(self.x, self.y),
            Point::new(self.right(), self.y),
            Point::new(self.x, self.bottom()),
            Point::new(self.right(), self.bottom()),
        ]
    }

    pub(crate) fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
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

    pub fn apply(&self, point: Point) -> Point {
        Point::new(
            self.a * point.x + self.c * point.y + self.e,
            self.b * point.x + self.d * point.y + self.f,
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
        if determinant.abs() < 1e-9 || !determinant.is_finite() {
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

    /// the same transform without its move, which is what an appearance
    /// stream's /Matrix needs (the viewer places the result on /Rect itself).
    pub fn without_translation(&self) -> Matrix {
        Matrix::new(self.a, self.b, self.c, self.d, 0.0, 0.0)
    }

    pub fn to_array(self) -> [f32; 6] {
        [self.a, self.b, self.c, self.d, self.e, self.f]
    }

    /// the upright box around a rectangle after it has been transformed.
    pub fn apply_to_box(&self, [left, bottom, right, top]: [f32; 4]) -> [f32; 4] {
        let corners = [
            self.apply(Point::new(left, bottom)),
            self.apply(Point::new(right, bottom)),
            self.apply(Point::new(left, top)),
            self.apply(Point::new(right, top)),
        ];
        let around = Rect::around(corners).expect("four corners");
        [around.x, around.y, around.right(), around.bottom()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-3
    }

    #[test]
    fn then_applies_the_first_matrix_first() {
        let scale = Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
        let shift = Matrix::new(1.0, 0.0, 0.0, 1.0, 10.0, 0.0);
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
        let inverse = matrix.invert().unwrap();
        let point = Point::new(5.0, 7.0);
        assert!(close(inverse.apply(matrix.apply(point)), point));
    }

    #[test]
    fn rectangles_know_their_bounds() {
        let rect = Rect::around([Point::new(3.0, 9.0), Point::new(1.0, 4.0)]).unwrap();
        assert_eq!(rect, Rect::new(1.0, 4.0, 2.0, 5.0));
        assert!(Rect::new(0.0, 0.0, 0.0, 4.0).is_empty());
        assert!(Rect::new(0.0, f32::NAN, 1.0, 4.0).is_empty());
        assert!(!rect.is_empty());
    }
}
