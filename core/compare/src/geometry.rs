//! points, rectangles, matrices and quarter turns.
//!
//! every rectangle the crate hands out is in points (1/72 inch) on the
//! page as the person sees it: measured from the top left corner, with y
//! going down. that is the same convention the viewer and letters-pdf use,
//! so the ui can draw a rectangle without converting it.

use serde::{Deserialize, Serialize};

/// a rectangle in points, from the top left corner of the visible page.
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

    /// builds a rectangle from its four edges, in any order.
    pub fn from_edges(left: f32, top: f32, right: f32, bottom: f32) -> Self {
        let (left, right) = (left.min(right), left.max(right));
        let (top, bottom) = (top.min(bottom), top.max(bottom));
        Self::new(left, top, right - left, bottom - top)
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
        Rect::from_edges(
            self.x.min(other.x),
            self.y.min(other.y),
            self.right().max(other.right()),
            self.bottom().max(other.bottom()),
        )
    }

    /// the rectangle grown by `by` points on every side.
    pub fn expanded(&self, by: f32) -> Rect {
        Rect::new(
            self.x - by,
            self.y - by,
            self.width + 2.0 * by,
            self.height + 2.0 * by,
        )
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// how far the two overlap along the x axis, zero when they don't.
    pub fn horizontal_overlap(&self, other: &Rect) -> f32 {
        (self.right().min(other.right()) - self.x.max(other.x)).max(0.0)
    }

    /// how far the two overlap along the y axis, zero when they don't.
    pub fn vertical_overlap(&self, other: &Rect) -> f32 {
        (self.bottom().min(other.bottom()) - self.y.max(other.y)).max(0.0)
    }

    /// moves the rectangle by an offset.
    pub fn shifted(&self, dx: f32, dy: f32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }

    /// the union of many rectangles, or none for an empty list.
    pub fn union_all<'a>(rects: impl IntoIterator<Item = &'a Rect>) -> Option<Rect> {
        rects.into_iter().fold(None, |sum, rect| match sum {
            None => Some(*rect),
            Some(sum) => Some(sum.union(rect)),
        })
    }
}

/// the size of a page in points, as the person sees it (after cropping and turning).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

impl PageSize {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    pub fn is_landscape(&self) -> bool {
        self.width > self.height
    }

    pub fn full_page(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width, self.height)
    }
}

/// a pdf transformation matrix [a b c d e f].
///
/// pdf multiplies row vectors: a point (x, y) becomes
/// (a x + c y + e, b x + d y + f). `then` follows the same order as pdf's
/// own operators, so `text.then(&ctm)` is "text space, then page space".
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
    pub const IDENTITY: Matrix = Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn new(a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Self {
        Self { a, b, c, d, e, f }
    }

    pub fn translation(x: f32, y: f32) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    /// this transformation followed by `next`.
    pub fn then(&self, next: &Matrix) -> Matrix {
        Matrix {
            a: self.a * next.a + self.b * next.c,
            b: self.a * next.b + self.b * next.d,
            c: self.c * next.a + self.d * next.c,
            d: self.c * next.b + self.d * next.d,
            e: self.e * next.a + self.f * next.c + next.e,
            f: self.e * next.b + self.f * next.d + next.f,
        }
    }

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// how long a vector of length 1 pointing up becomes. used to find the
    /// size text is really drawn at once all scaling is applied.
    pub fn vertical_scale(&self) -> f32 {
        (self.c * self.c + self.d * self.d).sqrt()
    }
}

/// how far text on a page is turned from upright, clockwise, in quarter turns.
///
/// the comparison lays text out in the upright frame, where lines run left
/// to right, and maps rectangles back to the visible page at the end. that
/// way a page that is turned in one version but not the other still lines
/// up word for word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum QuarterTurn {
    #[default]
    None,
    Clockwise,
    Half,
    CounterClockwise,
}

impl QuarterTurn {
    pub fn from_degrees(degrees: i64) -> Self {
        match degrees.rem_euclid(360) {
            90 => QuarterTurn::Clockwise,
            180 => QuarterTurn::Half,
            270 => QuarterTurn::CounterClockwise,
            _ => QuarterTurn::None,
        }
    }

    pub fn degrees(&self) -> u32 {
        match self {
            QuarterTurn::None => 0,
            QuarterTurn::Clockwise => 90,
            QuarterTurn::Half => 180,
            QuarterTurn::CounterClockwise => 270,
        }
    }

    pub fn is_sideways(&self) -> bool {
        matches!(self, QuarterTurn::Clockwise | QuarterTurn::CounterClockwise)
    }

    /// the size of the upright frame for a visible page of `visible` size.
    pub fn upright_size(&self, visible: PageSize) -> PageSize {
        if self.is_sideways() {
            PageSize::new(visible.height, visible.width)
        } else {
            visible
        }
    }

    /// turns a point on the visible page into the upright frame.
    pub fn point_to_upright(&self, x: f32, y: f32, visible: PageSize) -> (f32, f32) {
        match self {
            QuarterTurn::None => (x, y),
            // the page was turned clockwise, so turning back is counter clockwise
            QuarterTurn::Clockwise => (y, visible.width - x),
            QuarterTurn::Half => (visible.width - x, visible.height - y),
            QuarterTurn::CounterClockwise => (visible.height - y, x),
        }
    }

    /// turns a point in the upright frame back onto the visible page.
    pub fn point_to_visible(&self, x: f32, y: f32, visible: PageSize) -> (f32, f32) {
        match self {
            QuarterTurn::None => (x, y),
            QuarterTurn::Clockwise => (visible.width - y, x),
            QuarterTurn::Half => (visible.width - x, visible.height - y),
            QuarterTurn::CounterClockwise => (y, visible.height - x),
        }
    }

    pub fn rect_to_upright(&self, rect: &Rect, visible: PageSize) -> Rect {
        let (x0, y0) = self.point_to_upright(rect.x, rect.y, visible);
        let (x1, y1) = self.point_to_upright(rect.right(), rect.bottom(), visible);
        Rect::from_edges(x0, y0, x1, y1)
    }

    pub fn rect_to_visible(&self, rect: &Rect, visible: PageSize) -> Rect {
        let (x0, y0) = self.point_to_visible(rect.x, rect.y, visible);
        let (x1, y1) = self.point_to_visible(rect.right(), rect.bottom(), visible);
        Rect::from_edges(x0, y0, x1, y1)
    }
}

/// rounds to one decimal place, which is as fine as sizes are reported.
pub fn round_tenth(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTER: PageSize = PageSize {
        width: 612.0,
        height: 792.0,
    };

    #[test]
    fn matrices_apply_in_pdf_order() {
        let scale = Matrix::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0);
        let shift = Matrix::translation(10.0, 5.0);
        // scale first, then shift
        assert_eq!(scale.then(&shift).apply(1.0, 1.0), (12.0, 7.0));
        // shift first, then scale
        assert_eq!(shift.then(&scale).apply(1.0, 1.0), (22.0, 12.0));
    }

    #[test]
    fn quarter_turns_round_trip() {
        for turn in [
            QuarterTurn::None,
            QuarterTurn::Clockwise,
            QuarterTurn::Half,
            QuarterTurn::CounterClockwise,
        ] {
            let visible = if turn.is_sideways() {
                PageSize::new(792.0, 612.0)
            } else {
                LETTER
            };
            let rect = Rect::new(72.0, 100.0, 50.0, 12.0);
            let upright = turn.rect_to_upright(&rect, visible);
            let back = turn.rect_to_visible(&upright, visible);
            assert!((back.x - rect.x).abs() < 0.01, "{turn:?}");
            assert!((back.y - rect.y).abs() < 0.01, "{turn:?}");
            assert!((back.width - rect.width).abs() < 0.01, "{turn:?}");
        }
    }

    #[test]
    fn a_clockwise_page_puts_the_upright_top_left_at_the_visible_top_right() {
        let visible = PageSize::new(792.0, 612.0);
        let (x, y) = QuarterTurn::Clockwise.point_to_visible(0.0, 0.0, visible);
        assert_eq!((x, y), (792.0, 0.0));
    }

    #[test]
    fn unions_cover_both_rectangles() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(20.0, 5.0, 10.0, 10.0);
        assert_eq!(a.union(&b), Rect::new(0.0, 0.0, 30.0, 15.0));
        assert!(!a.intersects(&b));
        assert_eq!(a.vertical_overlap(&b), 5.0);
    }
}
