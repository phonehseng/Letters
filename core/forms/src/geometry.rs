//! positions on a page, and turning between the two ways of measuring them.
//!
//! a pdf measures from the bottom left of its media box with y going up,
//! and a page can be turned (/Rotate) and cut down (/CropBox) when shown.
//! the ui measures from the top left of the page as the reader sees it,
//! with y going down. every position that crosses the crate's public api
//! is in the ui's way; everything inside the file is in the pdf's way.

use serde::{Deserialize, Serialize};

/// a rectangle in points, measured from the visible top left of the page
/// with y going down. this is what the ui sends and receives.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisibleRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl VisibleRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn is_empty(&self) -> bool {
        !(self.width > 0.0 && self.height > 0.0)
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// whether `other` lies completely inside this rectangle, allowing
    /// `slack` points of overhang on each side.
    pub fn contains(&self, other: &VisibleRect, slack: f32) -> bool {
        other.x >= self.x - slack
            && other.y >= self.y - slack
            && other.right() <= self.right() + slack
            && other.bottom() <= self.bottom() + slack
    }
}

/// a point in pdf user space (y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// a rectangle in pdf user space, stored by its lower left and upper right
/// corners the way /Rect and /BBox are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfRect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl PdfRect {
    /// builds a rectangle from any two opposite corners. pdf files do not
    /// promise the corners come in a particular order.
    pub fn from_corners(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self {
            left: x0.min(x1),
            bottom: y0.min(y1),
            right: x0.max(x1),
            top: y0.max(y1),
        }
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.top - self.bottom
    }

    /// the smallest rectangle holding all the points.
    pub fn around(points: &[Point]) -> Self {
        let mut rect = Self::from_corners(points[0].x, points[0].y, points[0].x, points[0].y);
        for point in &points[1..] {
            rect.left = rect.left.min(point.x);
            rect.right = rect.right.max(point.x);
            rect.bottom = rect.bottom.min(point.y);
            rect.top = rect.top.max(point.y);
        }
        rect
    }

    pub fn corners(&self) -> [Point; 4] {
        [
            Point::new(self.left, self.bottom),
            Point::new(self.right, self.bottom),
            Point::new(self.right, self.top),
            Point::new(self.left, self.top),
        ]
    }

    /// the part both rectangles share, or none if they do not overlap.
    pub fn intersect(&self, other: &PdfRect) -> Option<PdfRect> {
        let rect = PdfRect {
            left: self.left.max(other.left),
            bottom: self.bottom.max(other.bottom),
            right: self.right.min(other.right),
            top: self.top.min(other.top),
        };
        (rect.width() > 0.0 && rect.height() > 0.0).then_some(rect)
    }
}

/// a pdf transformation matrix [a b c d e f]: x' = a x + c y + e,
/// y' = b x + d y + f.
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

    pub fn translate(x: f32, y: f32) -> Self {
        Self::new(1.0, 0.0, 0.0, 1.0, x, y)
    }

    /// turns counterclockwise by a multiple of 90 degrees. exact numbers
    /// are used instead of sin and cos so no rounding creeps in.
    pub fn quarter_turns(degrees: i32) -> Self {
        match degrees.rem_euclid(360) {
            90 => Self::new(0.0, 1.0, -1.0, 0.0, 0.0, 0.0),
            180 => Self::new(-1.0, 0.0, 0.0, -1.0, 0.0, 0.0),
            270 => Self::new(0.0, -1.0, 1.0, 0.0, 0.0, 0.0),
            _ => Self::IDENTITY,
        }
    }

    /// the matrix that maps the unit square so that its corners land on
    /// `origin`, `origin + x_axis` and `origin + y_axis`.
    pub fn from_axes(origin: Point, x_end: Point, y_end: Point) -> Self {
        Self::new(
            x_end.x - origin.x,
            x_end.y - origin.y,
            y_end.x - origin.x,
            y_end.y - origin.y,
            origin.x,
            origin.y,
        )
    }

    /// first apply `self`, then `then`. this is the order pdf uses when
    /// a `cm` operator is followed by drawing in the new space.
    pub fn then(&self, then: &Matrix) -> Matrix {
        Matrix {
            a: self.a * then.a + self.b * then.c,
            b: self.a * then.b + self.b * then.d,
            c: self.c * then.a + self.d * then.c,
            d: self.c * then.b + self.d * then.d,
            e: self.e * then.a + self.f * then.c + then.e,
            f: self.e * then.b + self.f * then.d + then.f,
        }
    }

    pub fn apply(&self, point: Point) -> Point {
        Point::new(
            self.a * point.x + self.c * point.y + self.e,
            self.b * point.x + self.d * point.y + self.f,
        )
    }

    /// the bounding box of a rectangle after this matrix moves it.
    pub fn apply_rect(&self, rect: &PdfRect) -> PdfRect {
        let corners = rect.corners().map(|corner| self.apply(corner));
        PdfRect::around(&corners)
    }

    /// the six numbers as written in a content stream.
    pub fn to_operands(self) -> String {
        format!(
            "{} {} {} {} {} {}",
            number(self.a),
            number(self.b),
            number(self.c),
            number(self.d),
            number(self.e),
            number(self.f)
        )
    }
}

/// writes a number the short way pdf files expect: no exponent, no
/// trailing zeros, at most three decimals.
pub fn number(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let rounded = (value * 1000.0).round() / 1000.0;
    let text = format!("{rounded:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// how a page is shown: which part of user space is visible and how far
/// it is turned. built once per page and used for every conversion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageFrame {
    /// the visible part of the page in user space (crop box inside media box)
    pub visible: PdfRect,
    /// clockwise turn when shown: 0, 90, 180 or 270
    pub rotation: i32,
}

impl PageFrame {
    pub fn new(visible: PdfRect, rotate_degrees: i64) -> Self {
        let rotation = (rotate_degrees.rem_euclid(360) / 90 * 90) as i32;
        Self { visible, rotation }
    }

    /// the page's size as the reader sees it, width first.
    pub fn visible_size(&self) -> (f32, f32) {
        let (width, height) = (self.visible.width(), self.visible.height());
        if self.rotation % 180 == 0 {
            (width, height)
        } else {
            (height, width)
        }
    }

    /// a user space point measured from the visible top left, y down.
    pub fn to_visible_point(self, point: Point) -> (f32, f32) {
        let dx = point.x - self.visible.left;
        let dy = point.y - self.visible.bottom;
        let (width, height) = (self.visible.width(), self.visible.height());
        match self.rotation {
            90 => (dy, dx),
            180 => (width - dx, dy),
            270 => (height - dy, width - dx),
            _ => (dx, height - dy),
        }
    }

    /// the user space point at a position measured from the visible top left.
    pub fn to_user_point(self, x: f32, y: f32) -> Point {
        let (width, height) = (self.visible.width(), self.visible.height());
        let (dx, dy) = match self.rotation {
            90 => (y, x),
            180 => (width - x, y),
            270 => (width - y, height - x),
            _ => (x, height - y),
        };
        Point::new(self.visible.left + dx, self.visible.bottom + dy)
    }

    pub fn to_visible_rect(self, rect: &PdfRect) -> VisibleRect {
        let corners = rect.corners().map(|corner| self.to_visible_point(corner));
        let left = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min);
        let right = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max);
        let top = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min);
        let bottom = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max);
        VisibleRect::new(left, top, right - left, bottom - top)
    }

    pub fn to_user_rect(self, rect: &VisibleRect) -> PdfRect {
        let corners = [
            self.to_user_point(rect.x, rect.y),
            self.to_user_point(rect.right(), rect.y),
            self.to_user_point(rect.right(), rect.bottom()),
            self.to_user_point(rect.x, rect.bottom()),
        ];
        PdfRect::around(&corners)
    }

    /// the matrix that draws a unit square upright, as the reader sees the
    /// page, filling `rect`. (0, 0) of the square is its visible bottom left.
    /// images and upright appearances use it so they are never shown sideways.
    pub fn upright_unit_square(&self, rect: &VisibleRect) -> Matrix {
        Matrix::from_axes(
            self.to_user_point(rect.x, rect.bottom()),
            self.to_user_point(rect.right(), rect.bottom()),
            self.to_user_point(rect.x, rect.y),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letter(rotation: i64) -> PageFrame {
        PageFrame::new(PdfRect::from_corners(0.0, 0.0, 612.0, 792.0), rotation)
    }

    #[test]
    fn unturned_pages_flip_y() {
        let frame = letter(0);
        assert_eq!(
            frame.to_visible_point(Point::new(72.0, 720.0)),
            (72.0, 72.0)
        );
        assert_eq!(frame.to_user_point(72.0, 72.0), Point::new(72.0, 720.0));
    }

    #[test]
    fn every_rotation_round_trips() {
        for rotation in [0, 90, 180, 270, -90, 450] {
            let frame = PageFrame::new(PdfRect::from_corners(10.0, 20.0, 600.0, 700.0), rotation);
            let rect = VisibleRect::new(30.0, 40.0, 100.0, 20.0);
            let back = frame.to_visible_rect(&frame.to_user_rect(&rect));
            assert!((back.x - rect.x).abs() < 0.01, "{rotation}: {back:?}");
            assert!((back.y - rect.y).abs() < 0.01, "{rotation}: {back:?}");
            assert!((back.width - rect.width).abs() < 0.01);
            assert!((back.height - rect.height).abs() < 0.01);
        }
    }

    #[test]
    fn a_page_turned_clockwise_shows_its_bottom_left_at_the_top_left() {
        let frame = letter(90);
        assert_eq!(frame.visible_size(), (792.0, 612.0));
        assert_eq!(frame.to_visible_point(Point::new(0.0, 0.0)), (0.0, 0.0));
        assert_eq!(frame.to_visible_point(Point::new(0.0, 792.0)), (792.0, 0.0));
    }

    #[test]
    fn crop_box_origin_is_respected() {
        let frame = PageFrame::new(PdfRect::from_corners(100.0, 100.0, 400.0, 500.0), 0);
        assert_eq!(frame.to_visible_point(Point::new(100.0, 500.0)), (0.0, 0.0));
    }

    #[test]
    fn upright_square_keeps_text_upright_on_turned_pages() {
        for rotation in [0, 90, 180, 270] {
            let frame = letter(rotation);
            let rect = VisibleRect::new(50.0, 60.0, 200.0, 30.0);
            let matrix = frame.upright_unit_square(&rect);
            // the square's top left must land at the rectangle's visible top left
            let top_left = frame.to_visible_point(matrix.apply(Point::new(0.0, 1.0)));
            assert!((top_left.0 - 50.0).abs() < 0.01 && (top_left.1 - 60.0).abs() < 0.01);
        }
    }

    #[test]
    fn numbers_are_short() {
        assert_eq!(number(12.0), "12");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(-0.0001), "0");
        assert_eq!(number(1.23456), "1.235");
    }
}
