//! rectangles, transformation matrices and the page as the person sees it.
//!
//! pdf measures from the bottom left of the paper with y going up, and a
//! page can be cropped (/CropBox) and turned (/Rotate). the ui draws on the
//! page as shown, so every location this crate reports is measured from
//! the visible top left corner, in points, with y going down.

use lopdf::{Document, ObjectId};
use serde::{Deserialize, Serialize};

use crate::objects::{inherited, number_box};

/// a rectangle on the visible page, in points from the top left corner.
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

    /// the smallest rectangle holding all the points.
    pub fn around(points: &[(f32, f32)]) -> Self {
        let mut left = f32::INFINITY;
        let mut top = f32::INFINITY;
        let mut right = f32::NEG_INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for &(x, y) in points {
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
        }
        if !left.is_finite() || !top.is_finite() {
            return Self::default();
        }
        Self::new(left, top, right - left, bottom - top)
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn area(&self) -> f32 {
        self.width.max(0.0) * self.height.max(0.0)
    }

    pub fn union(&self, other: &Rect) -> Rect {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Rect::new(left, top, right - left, bottom - top)
    }

    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        (right > left && bottom > top).then(|| Rect::new(left, top, right - left, bottom - top))
    }

    pub fn contains_point(&self, (x, y): (f32, f32)) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    /// the union of many rectangles, or none for an empty list.
    pub fn union_all<'a>(rects: impl IntoIterator<Item = &'a Rect>) -> Option<Rect> {
        rects
            .into_iter()
            .fold(None, |sum: Option<Rect>, rect| match sum {
                None => Some(*rect),
                Some(sum) => Some(sum.union(rect)),
            })
    }
}

/// a pdf transformation matrix [a b c d e f]: x' = a x + c y + e, y' = b x + d y + f.
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

    pub fn new(values: [f32; 6]) -> Self {
        let [a, b, c, d, e, f] = values;
        Self { a, b, c, d, e, f }
    }

    pub fn translate(x: f32, y: f32) -> Self {
        Self::new([1.0, 0.0, 0.0, 1.0, x, y])
    }

    /// this transformation followed by `then`, the order pdf uses when it
    /// writes `self × then`.
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

    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// how much the matrix stretches a vertical length, used for the size
    /// text appears at.
    pub fn vertical_scale(&self) -> f32 {
        (self.c * self.c + self.d * self.d).sqrt()
    }
}

/// the part of the paper that is shown, and how it is turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageFrame {
    /// left, bottom, right, top of the shown area in pdf coordinates
    pub shown: [f32; 4],
    /// clockwise quarter turns: 0, 90, 180 or 270
    pub rotate: u32,
}

/// us letter, used when a page has no usable size at all.
const FALLBACK_BOX: [f32; 4] = [0.0, 0.0, 612.0, 792.0];

impl PageFrame {
    pub fn read(doc: &Document, page: ObjectId) -> Self {
        let media = inherited(doc, page, b"MediaBox")
            .and_then(number_box)
            .filter(|b| b[2] > b[0] && b[3] > b[1])
            .unwrap_or(FALLBACK_BOX);
        // the crop box is clipped to the paper, as viewers do
        let shown = inherited(doc, page, b"CropBox")
            .and_then(number_box)
            .map(|crop| {
                [
                    crop[0].max(media[0]),
                    crop[1].max(media[1]),
                    crop[2].min(media[2]),
                    crop[3].min(media[3]),
                ]
            })
            .filter(|b| b[2] > b[0] && b[3] > b[1])
            .unwrap_or(media);
        let rotate = inherited(doc, page, b"Rotate")
            .and_then(|value| value.as_float().ok())
            .map(|degrees| (degrees as i64).rem_euclid(360) as u32 / 90 * 90)
            .unwrap_or(0);
        Self { shown, rotate }
    }

    fn unturned_size(&self) -> (f32, f32) {
        (self.shown[2] - self.shown[0], self.shown[3] - self.shown[1])
    }

    /// width and height as shown, after turning.
    pub fn visible_size(&self) -> (f32, f32) {
        let (width, height) = self.unturned_size();
        if self.rotate % 180 == 90 {
            (height, width)
        } else {
            (width, height)
        }
    }

    /// a point in pdf page coordinates, measured on the page as shown.
    pub fn to_visible(self, x: f32, y: f32) -> (f32, f32) {
        let (width, height) = self.unturned_size();
        // first measure from the top left of the shown area, y down
        let across = x - self.shown[0];
        let down = self.shown[3] - y;
        match self.rotate {
            90 => (height - down, across),
            180 => (width - across, height - down),
            270 => (down, width - across),
            _ => (across, down),
        }
    }

    /// a point measured on the page as shown, back in pdf coordinates.
    /// only the tests need it so far, to check `to_visible` both ways.
    #[cfg(test)]
    pub fn back_to_pdf(self, x: f32, y: f32) -> (f32, f32) {
        let (width, height) = self.unturned_size();
        let (across, down) = match self.rotate {
            90 => (y, height - x),
            180 => (width - x, height - y),
            270 => (width - y, x),
            _ => (x, y),
        };
        (self.shown[0] + across, self.shown[3] - down)
    }

    /// the visible rectangle around some points given in pdf coordinates.
    pub fn visible_rect(&self, points: &[(f32, f32)]) -> Rect {
        let turned: Vec<(f32, f32)> = points.iter().map(|&(x, y)| self.to_visible(x, y)).collect();
        Rect::around(&turned)
    }

    /// a rectangle written in the file as [x1 y1 x2 y2] (annotations do
    /// this), measured on the page as shown.
    pub fn visible_box(&self, corners: [f32; 4]) -> Rect {
        let [left, bottom, right, top] = corners;
        self.visible_rect(&[(left, bottom), (right, bottom), (left, top), (right, top)])
    }

    pub fn visible_area(&self) -> f32 {
        let (width, height) = self.visible_size();
        width * height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rotate: u32) -> PageFrame {
        PageFrame {
            shown: [0.0, 0.0, 200.0, 100.0],
            rotate,
        }
    }

    #[test]
    fn an_unturned_page_measures_from_the_top_left() {
        assert_eq!(frame(0).to_visible(10.0, 90.0), (10.0, 10.0));
        assert_eq!(frame(0).visible_size(), (200.0, 100.0));
    }

    #[test]
    fn a_quarter_turn_moves_the_top_left_corner_to_the_top_right() {
        let turned = frame(90);
        assert_eq!(turned.visible_size(), (100.0, 200.0));
        // the paper's top left corner ends up at the shown top right
        assert_eq!(turned.to_visible(0.0, 100.0), (100.0, 0.0));
        // and its bottom left at the shown top left
        assert_eq!(turned.to_visible(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn half_and_three_quarter_turns() {
        assert_eq!(frame(180).to_visible(0.0, 100.0), (200.0, 100.0));
        assert_eq!(frame(270).to_visible(0.0, 100.0), (0.0, 200.0));
    }

    #[test]
    fn visible_points_turn_back_into_pdf_points() {
        for rotate in [0, 90, 180, 270] {
            let frame = frame(rotate);
            let (x, y) = frame.to_visible(30.0, 40.0);
            let (back_x, back_y) = frame.back_to_pdf(x, y);
            assert!((back_x - 30.0).abs() < 0.01 && (back_y - 40.0).abs() < 0.01);
        }
    }

    #[test]
    fn a_crop_box_moves_the_origin() {
        let cropped = PageFrame {
            shown: [50.0, 50.0, 150.0, 150.0],
            rotate: 0,
        };
        assert_eq!(cropped.to_visible(60.0, 140.0), (10.0, 10.0));
    }

    #[test]
    fn matrices_combine_in_pdf_order() {
        let scale = Matrix::new([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
        let shift = Matrix::translate(10.0, 0.0);
        // scale first, then shift
        assert_eq!(scale.then(&shift).apply(1.0, 1.0), (12.0, 2.0));
        // shift first, then scale
        assert_eq!(shift.then(&scale).apply(1.0, 1.0), (22.0, 2.0));
    }
}
