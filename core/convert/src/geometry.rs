//! rectangles and matrices.
//!
//! every position this crate hands out is in points from the top left of
//! the page as the person sees it, with y going down, like the viewer and
//! the editing tools. matrices follow the pdf convention: a point (x, y)
//! becomes (a x + c y + e, b x + d y + f).

use serde::{Deserialize, Serialize};

/// an upright rectangle in points from the visible top left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
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

    /// the rectangle between two corners given in any order.
    pub fn from_corners(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs())
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn center_x(&self) -> f32 {
        self.x + self.width / 2.0
    }

    pub fn center_y(&self) -> f32 {
        self.y + self.height / 2.0
    }

    /// the smallest rectangle holding both.
    pub fn union(&self, other: &Rect) -> Rect {
        Rect::from_corners(
            self.x.min(other.x),
            self.y.min(other.y),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
    }

    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    /// how far the two overlap left to right (zero or less when apart).
    pub fn horizontal_overlap(&self, other: &Rect) -> f32 {
        self.right().min(other.right()) - self.x.max(other.x)
    }

    /// how far the two overlap top to bottom (zero or less when apart).
    pub fn vertical_overlap(&self, other: &Rect) -> f32 {
        self.bottom().min(other.bottom()) - self.y.max(other.y)
    }

    pub(crate) fn from_edit(rect: letters_edit::Rect) -> Self {
        Self::new(rect.x, rect.y, rect.width, rect.height)
    }
}

/// the smallest rectangle around all of them, none for an empty list.
pub fn union_all<'a>(rects: impl IntoIterator<Item = &'a Rect>) -> Option<Rect> {
    rects.into_iter().fold(None, |sum, rect| match sum {
        None => Some(*rect),
        Some(sum) => Some(sum.union(rect)),
    })
}

/// an affine transformation in the pdf convention.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
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

    /// reads the six numbers of a `cm` operator or a /Matrix array.
    pub fn from_numbers(numbers: &[f32]) -> Option<Self> {
        match numbers {
            [a, b, c, d, e, f] => Some(Self::new(*a, *b, *c, *d, *e, *f)),
            _ => None,
        }
    }

    /// this transformation followed by `then`.
    pub fn then(&self, then: &Matrix) -> Matrix {
        Matrix::new(
            self.a * then.a + self.b * then.c,
            self.a * then.b + self.b * then.d,
            self.c * then.a + self.d * then.c,
            self.c * then.b + self.d * then.d,
            self.e * then.a + self.f * then.c + then.e,
            self.e * then.b + self.f * then.d + then.f,
        )
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// how much lengths grow, on average over both directions. used for
    /// line widths, which pdf measures in the space they are drawn in.
    pub fn average_scale(&self) -> f32 {
        let x = self.a.hypot(self.b);
        let y = self.c.hypot(self.d);
        (x + y) / 2.0
    }

    /// whether the axes stay horizontal and vertical (no turn or slant
    /// other than quarter turns).
    pub fn keeps_axes(&self) -> bool {
        let tiny = 1e-4;
        (self.b.abs() < tiny && self.c.abs() < tiny) || (self.a.abs() < tiny && self.d.abs() < tiny)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrices_chain_in_pdf_order() {
        let scale = Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
        let shift = Matrix::new(1.0, 0.0, 0.0, 1.0, 10.0, 5.0);
        // scaled first, then shifted
        assert_eq!(scale.then(&shift).apply(1.0, 1.0), (12.0, 7.0));
        // shifted first, then scaled
        assert_eq!(shift.then(&scale).apply(1.0, 1.0), (22.0, 12.0));
    }

    #[test]
    fn rectangles_overlap_and_join() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 8.0, 10.0, 10.0);
        assert_eq!(a.horizontal_overlap(&b), 5.0);
        assert_eq!(a.vertical_overlap(&b), 2.0);
        assert_eq!(a.union(&b), Rect::new(0.0, 0.0, 15.0, 18.0));
    }
}
