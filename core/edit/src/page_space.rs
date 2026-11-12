//! converting between what the person sees and pdf user space.
//!
//! the app measures from the top left corner of the page as it is shown,
//! with y going down. a pdf measures from the bottom left corner of the
//! page as it is stored, with y going up, before /Rotate turns it and
//! /CropBox cuts it. this is the same conversion letters-annotate uses,
//! copied so the crates don't depend on each other.

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::EditError;
use crate::geometry::{Matrix, Point, Rect};
use crate::pdf_values::{number, rect_from_object};

const MAX_PAGE_TREE_DEPTH: usize = 64;

/// the page as the person sees it, and how to get back to pdf space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PageSpace {
    to_user: Matrix,
    to_visible: Matrix,
    /// clockwise turn applied when showing the page: 0, 90, 180 or 270
    pub rotation: i64,
    pub width: f32,
    pub height: f32,
}

impl PageSpace {
    pub fn of_page(doc: &Document, page_id: ObjectId) -> Result<Self, EditError> {
        let page = doc
            .get_dictionary(page_id)
            .map_err(|_| EditError::Damaged("the page is missing".into()))?;
        let media = inherited(doc, page, b"MediaBox")
            .and_then(|object| rect_from_object(doc, object))
            .unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let crop = inherited(doc, page, b"CropBox")
            .and_then(|object| rect_from_object(doc, object))
            .and_then(|crop| intersect(crop, media))
            .unwrap_or(media);
        let rotation = inherited(doc, page, b"Rotate")
            .and_then(|object| number(doc, object))
            .unwrap_or(0.0);
        Ok(Self::new(crop, rotation as i64))
    }

    pub fn new(visible_box: [f32; 4], rotation: i64) -> Self {
        let [x0, y0, x1, y1] = visible_box;
        let (width, height) = (x1 - x0, y1 - y0);
        let rotation = match rotation.rem_euclid(360) {
            turn @ (90 | 180 | 270) => turn,
            _ => 0,
        };
        // where a step right (vx) and a step down (vy) on screen go in
        // user space, starting from the shown top left corner
        let to_user = match rotation {
            90 => Matrix::new(0.0, 1.0, 1.0, 0.0, x0, y0),
            180 => Matrix::new(-1.0, 0.0, 0.0, 1.0, x1, y0),
            270 => Matrix::new(0.0, -1.0, -1.0, 0.0, x1, y1),
            _ => Matrix::new(1.0, 0.0, 0.0, -1.0, x0, y1),
        };
        let (width, height) = if rotation % 180 == 0 {
            (width, height)
        } else {
            (height, width)
        };
        Self {
            to_user,
            to_visible: to_user.invert().expect("a turn can always be undone"),
            rotation,
            width,
            height,
        }
    }

    pub fn user_point(&self, point: Point) -> Point {
        self.to_user.apply(point)
    }

    pub fn visible_point(&self, point: Point) -> Point {
        self.to_visible.apply(point)
    }

    pub fn user_vector(&self, vector: Point) -> Point {
        self.to_user.apply_vector(vector)
    }

    pub fn visible_vector(&self, vector: Point) -> Point {
        self.to_visible.apply_vector(vector)
    }

    /// screen points to user space, as a matrix.
    pub fn to_user_matrix(self) -> Matrix {
        self.to_user
    }

    /// user space to screen points, as a matrix.
    pub fn to_visible_matrix(self) -> Matrix {
        self.to_visible
    }

    pub fn visible_rect_of(&self, user_points: &[Point]) -> Option<Rect> {
        Rect::around(user_points.iter().map(|point| self.visible_point(*point)))
    }
}

/// a page setting, either on the page or on the nearest parent that has it.
pub(crate) fn inherited<'a>(
    doc: &'a Document,
    page: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Object> {
    let mut node = page;
    for _ in 0..MAX_PAGE_TREE_DEPTH {
        if let Ok(value) = node.get(key) {
            return Some(value);
        }
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        node = doc.get_dictionary(parent).ok()?;
    }
    None
}

fn intersect(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let shared = [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ];
    (shared[0] < shared[2] && shared[1] < shared[3]).then_some(shared)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: [f32; 4] = [10.0, 20.0, 110.0, 220.0];

    #[test]
    fn visible_and_user_points_round_trip_at_every_rotation() {
        for rotation in [0, 90, 180, 270] {
            let space = PageSpace::new(BOX, rotation);
            let point = Point::new(13.0, 41.0);
            let back = space.visible_point(space.user_point(point));
            assert!(back.minus(point).length() < 1e-3, "rotation {rotation}");
        }
    }

    #[test]
    fn a_quarter_turn_shows_the_bottom_left_corner_at_the_top_left() {
        let space = PageSpace::new(BOX, 90);
        assert_eq!((space.width, space.height), (200.0, 100.0));
        assert_eq!(
            space.user_point(Point::new(0.0, 0.0)),
            Point::new(10.0, 20.0)
        );
    }
}
