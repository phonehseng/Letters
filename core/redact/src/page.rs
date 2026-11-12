//! where a page is and which way up it is shown.
//!
//! the app measures everything in points from the top left corner of the
//! page as the person sees it: after the page's /CropBox has cut it down
//! and after /Rotate has turned it. pdf content is placed in "default user
//! space" instead: origin at the bottom left of the unturned media box,
//! y growing upward. `PageFrame` converts between the two.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::geometry::{Matrix, Point, Rect};
use crate::objects::{number, resolve};

/// a rectangle on a page as the person sees it: points from the visible
/// top left corner, y growing downward.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl PageRect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// the visible box of one page and how it is turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageFrame {
    /// the crop box in default user space
    pub crop: Rect,
    /// clockwise quarter turns: 0, 90, 180 or 270
    pub rotate: u32,
}

impl PageFrame {
    pub fn of_page(doc: &Document, page_id: ObjectId) -> Self {
        let media = inherited(doc, page_id, b"MediaBox")
            .and_then(|object| read_box(doc, object))
            .unwrap_or(Rect::from_corners(0.0, 0.0, 612.0, 792.0));
        let crop = inherited(doc, page_id, b"CropBox")
            .and_then(|object| read_box(doc, object))
            .map(|crop| intersect_or(crop, media))
            .unwrap_or(media);
        let rotate = inherited(doc, page_id, b"Rotate")
            .and_then(|object| number(doc, object))
            .unwrap_or(0.0);
        Self {
            crop,
            rotate: normalise_rotation(rotate),
        }
    }

    /// the matrix from visible points (top left origin, y down) to
    /// default user space.
    pub fn visible_to_user(&self) -> Matrix {
        let Rect { x0, y0, x1, y1 } = self.crop;
        match self.rotate {
            90 => Matrix::new(0.0, 1.0, 1.0, 0.0, x0, y0),
            180 => Matrix::new(-1.0, 0.0, 0.0, 1.0, x1, y0),
            270 => Matrix::new(0.0, -1.0, -1.0, 0.0, x1, y1),
            _ => Matrix::new(1.0, 0.0, 0.0, -1.0, x0, y1),
        }
    }

    pub fn user_to_visible(&self) -> Matrix {
        self.visible_to_user()
            .inverse()
            .expect("quarter turns can always be undone")
    }

    /// width and height of the page as shown.
    #[cfg(test)]
    pub fn visible_size(&self) -> (f64, f64) {
        if self.rotate == 90 || self.rotate == 270 {
            (self.crop.height(), self.crop.width())
        } else {
            (self.crop.width(), self.crop.height())
        }
    }

    pub fn to_user(self, rect: &PageRect) -> Rect {
        let corners = [
            Point::new(rect.x, rect.y),
            Point::new(rect.x + rect.width, rect.y + rect.height),
        ];
        let matrix = self.visible_to_user();
        let a = matrix.apply(corners[0]);
        let b = matrix.apply(corners[1]);
        Rect::from_corners(a.x, a.y, b.x, b.y)
    }

    pub fn to_visible(self, rect: &Rect) -> PageRect {
        let bounds = rect.transform(&self.user_to_visible()).bounds();
        PageRect::new(bounds.x0, bounds.y0, bounds.width(), bounds.height())
    }
}

/// /Rotate must be a multiple of 90; anything else is read as the
/// nearest quarter turn, the way viewers treat it.
fn normalise_rotation(degrees: f64) -> u32 {
    let quarters = (degrees / 90.0).round() as i64;
    (quarters.rem_euclid(4) * 90) as u32
}

fn intersect_or(crop: Rect, media: Rect) -> Rect {
    let clipped = Rect {
        x0: crop.x0.max(media.x0),
        y0: crop.y0.max(media.y0),
        x1: crop.x1.min(media.x1),
        y1: crop.y1.min(media.y1),
    };
    if clipped.is_empty() {
        media
    } else {
        clipped
    }
}

fn read_box(doc: &Document, object: &Object) -> Option<Rect> {
    let array = resolve(doc, object).as_array().ok()?;
    let values: Vec<f64> = array
        .iter()
        .filter_map(|value| number(doc, value))
        .collect();
    match values.as_slice() {
        [x0, y0, x1, y1] => Some(Rect::from_corners(*x0, *y0, *x1, *y1)),
        _ => None,
    }
}

/// the most levels of the page tree that are climbed looking for an
/// inherited value, so a looping tree cannot loop forever.
const MAX_TREE_DEPTH: usize = 64;

/// a page attribute that may be set on the page or on any of the page
/// tree nodes above it (/Resources, /MediaBox, /CropBox, /Rotate).
pub fn inherited<'a>(doc: &'a Document, page_id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut node = doc.get_dictionary(page_id).ok()?;
    for _ in 0..MAX_TREE_DEPTH {
        if let Ok(value) = node.get(key) {
            return Some(value);
        }
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        node = doc.get_dictionary(parent).ok()?;
    }
    None
}

/// the resources a page's content uses, wherever they are stored.
pub fn page_resources(doc: &Document, page_id: ObjectId) -> Dictionary {
    inherited(doc, page_id, b"Resources")
        .and_then(|object| resolve(doc, object).as_dict().ok())
        .cloned()
        .unwrap_or_default()
}

/// the object ids of every page, in reading order.
pub fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rotate: u32) -> PageFrame {
        PageFrame {
            crop: Rect::from_corners(10.0, 20.0, 210.0, 120.0),
            rotate,
        }
    }

    #[test]
    fn the_visible_top_left_is_the_right_corner_for_each_turn() {
        let corner = |rotate| frame(rotate).visible_to_user().apply(Point::new(0.0, 0.0));
        assert_eq!(corner(0), Point::new(10.0, 120.0));
        assert_eq!(corner(90), Point::new(10.0, 20.0));
        assert_eq!(corner(180), Point::new(210.0, 20.0));
        assert_eq!(corner(270), Point::new(210.0, 120.0));
    }

    #[test]
    fn turned_pages_swap_width_and_height() {
        assert_eq!(frame(0).visible_size(), (200.0, 100.0));
        assert_eq!(frame(90).visible_size(), (100.0, 200.0));
    }

    #[test]
    fn rectangles_survive_a_round_trip() {
        for rotate in [0, 90, 180, 270] {
            let frame = frame(rotate);
            let visible = PageRect::new(5.0, 7.0, 30.0, 11.0);
            let back = frame.to_visible(&frame.to_user(&visible));
            assert!((back.x - visible.x).abs() < 1e-9, "rotate {rotate}");
            assert!((back.y - visible.y).abs() < 1e-9, "rotate {rotate}");
            assert!((back.width - visible.width).abs() < 1e-9);
            assert!((back.height - visible.height).abs() < 1e-9);
        }
    }

    #[test]
    fn odd_rotations_snap_to_quarter_turns() {
        assert_eq!(normalise_rotation(-90.0), 270);
        assert_eq!(normalise_rotation(450.0), 90);
        assert_eq!(normalise_rotation(89.0), 90);
    }
}
