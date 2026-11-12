//! converting between what the person sees and pdf user space.
//!
//! the app measures from the top left corner of the page as it is shown,
//! with y going down. a pdf measures from the bottom left corner of the
//! page as it is stored, with y going up, before the page's /Rotate turns
//! it and its /CropBox cuts it. every annotation has to be written in the
//! stored space, so each operation asks this module to translate.

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::error::AnnotateError;
use crate::geometry::{Matrix, Point, Rect};
use crate::pdf_values::{number, rect_from_object};

/// how many /Parent links are followed when looking for an inherited
/// page setting. real files have a handful; a loop must not hang us.
const MAX_PAGE_TREE_DEPTH: usize = 64;

/// the page as the person sees it, and how to get back to pdf space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PageSpace {
    /// visible coordinates to pdf user space
    to_user: Matrix,
    /// pdf user space to visible coordinates
    to_visible: Matrix,
    /// clockwise turn applied when showing the page: 0, 90, 180 or 270
    pub rotation: i64,
    /// size of the page as shown, in points
    pub width: f32,
    pub height: f32,
}

impl PageSpace {
    /// reads the crop box, media box and rotation of a page, following
    /// the page tree for values a page inherits from its parents.
    pub fn of_page(doc: &Document, page_id: ObjectId) -> Result<Self, AnnotateError> {
        let page = doc
            .get_dictionary(page_id)
            .map_err(|_| AnnotateError::Damaged("the page is missing".into()))?;
        let media = inherited(doc, page, b"MediaBox")
            .and_then(|object| rect_from_object(doc, object))
            // us letter is what most readers assume for a page without one
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

    /// builds the conversion for a visible box `[left, bottom, right, top]`
    /// in user space, shown turned clockwise by `rotation` degrees.
    pub fn new(visible_box: [f32; 4], rotation: i64) -> Self {
        let [x0, y0, x1, y1] = visible_box;
        let (width, height) = (x1 - x0, y1 - y0);
        // a /Rotate that isn't a multiple of 90 is invalid; readers ignore it
        let rotation = match rotation.rem_euclid(360) {
            turn @ (90 | 180 | 270) => turn,
            _ => 0,
        };
        // each case says where a step right (vx) and a step down (vy) on
        // screen go in user space, starting from the shown top left corner
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

    /// a visible rectangle as a pdf rectangle `[left, bottom, right, top]`.
    pub fn user_rect(&self, rect: Rect) -> [f32; 4] {
        let corners = rect.corners().map(|corner| self.user_point(corner));
        let around = Rect::around(corners).expect("four corners");
        [around.x, around.y, around.right(), around.bottom()]
    }

    /// a pdf rectangle `[left, bottom, right, top]` as a visible rectangle.
    pub fn visible_rect(&self, [left, bottom, right, top]: [f32; 4]) -> Rect {
        let corners = [
            Point::new(left, bottom),
            Point::new(right, bottom),
            Point::new(left, top),
            Point::new(right, top),
        ]
        .map(|corner| self.visible_point(corner));
        Rect::around(corners).expect("four corners")
    }

    /// the /Matrix an appearance stream needs so that what it draws
    /// upright (x right, y up, as seen on screen) stays upright on this
    /// page after the page's rotation.
    ///
    /// appearances are drawn in a small upright frame whose y goes up;
    /// on screen y goes down, hence the flip before the page transform.
    pub fn appearance_matrix(&self) -> Matrix {
        let flip = Matrix::new(1.0, 0.0, 0.0, -1.0, 0.0, 0.0);
        flip.then(&self.to_user).without_translation()
    }
}

/// a page setting, either on the page or on the nearest parent that has it.
fn inherited<'a>(doc: &'a Document, page: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
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

/// the part two boxes share, or none when they don't overlap.
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

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < 1e-3
    }

    /// a page box that does not start at zero, to catch forgotten offsets
    const BOX: [f32; 4] = [10.0, 20.0, 110.0, 220.0];

    #[test]
    fn an_upright_page_flips_y() {
        let space = PageSpace::new(BOX, 0);
        assert_eq!((space.width, space.height), (100.0, 200.0));
        assert!(close(
            space.user_point(Point::new(0.0, 0.0)),
            Point::new(10.0, 220.0)
        ));
        assert!(close(
            space.user_point(Point::new(100.0, 200.0)),
            Point::new(110.0, 20.0)
        ));
    }

    #[test]
    fn a_page_turned_a_quarter_shows_its_bottom_left_corner_at_the_top_left() {
        let space = PageSpace::new(BOX, 90);
        assert_eq!((space.width, space.height), (200.0, 100.0));
        assert!(close(
            space.user_point(Point::new(0.0, 0.0)),
            Point::new(10.0, 20.0)
        ));
        // a step right on screen goes up the stored page
        assert!(close(
            space.user_point(Point::new(200.0, 0.0)),
            Point::new(10.0, 220.0)
        ));
        // a step down on screen goes right on the stored page
        assert!(close(
            space.user_point(Point::new(0.0, 100.0)),
            Point::new(110.0, 20.0)
        ));
    }

    #[test]
    fn upside_down_and_three_quarter_turns_land_on_the_right_corners() {
        let half = PageSpace::new(BOX, 180);
        assert!(close(
            half.user_point(Point::new(0.0, 0.0)),
            Point::new(110.0, 20.0)
        ));
        let three_quarters = PageSpace::new(BOX, -90);
        assert_eq!(three_quarters.rotation, 270);
        assert!(close(
            three_quarters.user_point(Point::new(0.0, 0.0)),
            Point::new(110.0, 220.0)
        ));
        assert!(close(
            three_quarters.user_point(Point::new(200.0, 100.0)),
            Point::new(10.0, 20.0)
        ));
    }

    #[test]
    fn rectangles_survive_a_round_trip_at_every_rotation() {
        let rect = Rect::new(5.0, 7.0, 30.0, 11.0);
        for rotation in [0, 90, 180, 270] {
            let space = PageSpace::new(BOX, rotation);
            let back = space.visible_rect(space.user_rect(rect));
            assert!((back.x - rect.x).abs() < 1e-3, "rotation {rotation}");
            assert!((back.y - rect.y).abs() < 1e-3, "rotation {rotation}");
            assert!((back.width - rect.width).abs() < 1e-3);
            assert!((back.height - rect.height).abs() < 1e-3);
        }
    }

    #[test]
    fn appearance_frames_stay_upright_on_screen() {
        for rotation in [0, 90, 180, 270] {
            let space = PageSpace::new(BOX, rotation);
            let frame = space.appearance_matrix();
            // in the frame, x goes right and y goes up as seen on screen.
            // follow both through user space and back to the screen
            let to_screen = |point| space.to_visible.without_translation().apply(point);
            let right = to_screen(frame.apply(Point::new(1.0, 0.0)));
            let up = to_screen(frame.apply(Point::new(0.0, 1.0)));
            assert!(close(right, Point::new(1.0, 0.0)), "rotation {rotation}");
            assert!(close(up, Point::new(0.0, -1.0)), "rotation {rotation}");
        }
    }

    #[test]
    fn odd_rotations_are_treated_as_none() {
        assert_eq!(PageSpace::new(BOX, 45).rotation, 0);
        assert_eq!(PageSpace::new(BOX, 450).rotation, 90);
    }
}
