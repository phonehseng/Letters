//! points, rectangles and the affine maps that carry them between spaces.
//!
//! three spaces matter in this crate:
//! - the pixels of the image tesseract read (after straightening and turning)
//! - the pixels of the original page image, which the person sees
//! - points on the page (1/72 inch), measured from the visible top left
//!
//! every preprocessing step that moves pixels records how to go back, as an
//! `Affine`, so recognised words can be put back where they are on the page.

use serde::{Deserialize, Serialize};

/// a position, in whatever space the surrounding code says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn distance_to(self, other: Point) -> f32 {
        ((other.x - self.x).powi(2) + (other.y - self.y).powi(2)).sqrt()
    }
}

/// an upright rectangle, measured from its top left corner with y going down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
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

    /// the smallest upright rectangle holding every point.
    pub fn around(points: &[Point]) -> Self {
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for point in points {
            left = left.min(point.x);
            top = top.min(point.y);
            right = right.max(point.x);
            bottom = bottom.max(point.y);
        }
        if points.is_empty() {
            return Self::default();
        }
        Self::new(left, top, right - left, bottom - top)
    }

    /// the smallest rectangle holding both.
    pub fn union(self, other: Rect) -> Self {
        Self::around(&[
            Point::new(self.x, self.y),
            Point::new(self.right(), self.bottom()),
            Point::new(other.x, other.y),
            Point::new(other.right(), other.bottom()),
        ])
    }

    pub fn right(self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    pub fn center(self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn corners(self) -> [Point; 4] {
        [
            Point::new(self.x, self.y),
            Point::new(self.right(), self.y),
            Point::new(self.right(), self.bottom()),
            Point::new(self.x, self.bottom()),
        ]
    }
}

/// a map of the form  x' = a x + c y + e,  y' = b x + d y + f.
///
/// the letters follow the pdf convention, so the same six numbers can be
/// written into a pdf `cm` or `Tm` operator unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Affine {
    pub const IDENTITY: Affine = Affine::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);

    pub const fn new(a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        Self { a, b, c, d, e, f }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self::new(sx, 0.0, 0.0, sy, 0.0, 0.0)
    }

    pub fn apply(&self, point: Point) -> Point {
        Point::new(
            self.a * point.x + self.c * point.y + self.e,
            self.b * point.x + self.d * point.y + self.f,
        )
    }

    /// applies only the turning and stretching part, for directions.
    pub fn apply_to_vector(&self, vector: Point) -> Point {
        Point::new(
            self.a * vector.x + self.c * vector.y,
            self.b * vector.x + self.d * vector.y,
        )
    }

    /// the map that does `self` first and then `next`.
    pub fn then(&self, next: &Affine) -> Affine {
        Affine::new(
            next.a * self.a + next.c * self.b,
            next.b * self.a + next.d * self.b,
            next.a * self.c + next.c * self.d,
            next.b * self.c + next.d * self.d,
            next.a * self.e + next.c * self.f + next.e,
            next.b * self.e + next.d * self.f + next.f,
        )
    }

    /// the map that undoes this one, or none when it squashes space flat.
    pub fn invert(&self) -> Option<Affine> {
        let determinant = self.a * self.d - self.b * self.c;
        if determinant.abs() < 1e-12 {
            return None;
        }
        let a = self.d / determinant;
        let b = -self.b / determinant;
        let c = -self.c / determinant;
        let d = self.a / determinant;
        Some(Affine::new(
            a,
            b,
            c,
            d,
            -(a * self.e + c * self.f),
            -(b * self.e + d * self.f),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Point, b: Point) -> bool {
        a.distance_to(b) < 1e-3
    }

    #[test]
    fn then_runs_the_first_map_first() {
        let shift = Affine::new(1.0, 0.0, 0.0, 1.0, 10.0, 0.0);
        let double = Affine::scale(2.0, 2.0);
        // shift then double: (1, 1) -> (11, 1) -> (22, 2)
        assert!(close(
            shift.then(&double).apply(Point::new(1.0, 1.0)),
            Point::new(22.0, 2.0)
        ));
    }

    #[test]
    fn invert_undoes_the_map() {
        let map = Affine::new(0.8, -0.6, 0.6, 0.8, 30.0, -12.0);
        let back = map.invert().unwrap();
        let point = Point::new(5.0, 7.0);
        assert!(close(back.apply(map.apply(point)), point));
    }

    #[test]
    fn a_flat_map_has_no_inverse() {
        assert_eq!(Affine::scale(0.0, 1.0).invert(), None);
    }

    #[test]
    fn around_finds_the_bounding_box() {
        let rect = Rect::around(&[Point::new(3.0, 9.0), Point::new(-1.0, 2.0)]);
        assert_eq!(rect, Rect::new(-1.0, 2.0, 4.0, 7.0));
    }
}
