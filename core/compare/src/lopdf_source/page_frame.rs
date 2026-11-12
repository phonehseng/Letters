//! where a page's visible area is and how it is turned.
//!
//! a pdf page draws in "user space": points from the bottom left, y up,
//! anywhere on an endless sheet. what the person sees is the crop box (or
//! the media box when there is none), turned by /Rotate. this file turns
//! user space positions into positions on that visible page.

use lopdf::{Document, Object, ObjectId};

use crate::geometry::{PageSize, QuarterTurn, Rect};

/// how far up the page tree an inherited setting is looked for. real files
/// are a few levels deep; the limit stops a damaged tree from looping.
const MAX_TREE_DEPTH: usize = 32;

/// us letter, used when a page gives no size at all.
const DEFAULT_BOX: [f32; 4] = [0.0, 0.0, 612.0, 792.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PageFrame {
    /// the visible area in user space: left, bottom, right, top
    crop: [f32; 4],
    turn: QuarterTurn,
}

impl PageFrame {
    /// reads the crop box, media box and rotation of a page, following
    /// the page tree for settings the page inherits.
    pub fn read(document: &Document, page: ObjectId) -> PageFrame {
        let media = inherited(document, page, b"MediaBox")
            .and_then(|object| read_box(document, object))
            .unwrap_or(DEFAULT_BOX);
        let crop = inherited(document, page, b"CropBox")
            .and_then(|object| read_box(document, object))
            .map(|crop| intersect(crop, media))
            .unwrap_or(media);
        let degrees = inherited(document, page, b"Rotate")
            .and_then(|object| document.dereference(object).ok())
            .and_then(|(_, object)| object.as_i64().ok())
            .unwrap_or(0);
        PageFrame {
            crop,
            turn: QuarterTurn::from_degrees(degrees),
        }
    }

    pub fn visible_size(&self) -> PageSize {
        let width = self.crop[2] - self.crop[0];
        let height = self.crop[3] - self.crop[1];
        if self.turn.is_sideways() {
            PageSize::new(height, width)
        } else {
            PageSize::new(width, height)
        }
    }

    /// a user space point as a point on the visible page.
    pub fn visible_point(&self, x: f32, y: f32) -> (f32, f32) {
        // first onto the unturned page, measured from its top left
        let unturned_x = x - self.crop[0];
        let unturned_y = self.crop[3] - y;
        // then turned the way /Rotate shows it. the unturned page plays the
        // part of the "upright" frame here
        self.turn
            .point_to_visible(unturned_x, unturned_y, self.visible_size())
    }

    /// the bounding box of some user space points, on the visible page.
    pub fn bounds_on_visible(&self, points: &[(f32, f32)]) -> Rect {
        let mut left = f32::MAX;
        let mut top = f32::MAX;
        let mut right = f32::MIN;
        let mut bottom = f32::MIN;
        for &(x, y) in points {
            let (vx, vy) = self.visible_point(x, y);
            left = left.min(vx);
            right = right.max(vx);
            top = top.min(vy);
            bottom = bottom.max(vy);
        }
        Rect::from_edges(left, top, right, bottom)
    }

    /// a short description of the frame, used to tell drawings apart.
    pub fn fingerprint_parts(&self) -> [u32; 5] {
        [
            self.crop[0].to_bits(),
            self.crop[1].to_bits(),
            self.crop[2].to_bits(),
            self.crop[3].to_bits(),
            self.turn.degrees(),
        ]
    }
}

/// finds a page setting on the page or the nearest page tree node above it.
pub(crate) fn inherited<'a>(
    document: &'a Document,
    page: ObjectId,
    key: &[u8],
) -> Option<&'a Object> {
    let mut node = document.get_dictionary(page).ok()?;
    for _ in 0..MAX_TREE_DEPTH {
        if let Ok(value) = node.get(key) {
            return Some(value);
        }
        let parent = node.get(b"Parent").ok()?.as_reference().ok()?;
        node = document.get_dictionary(parent).ok()?;
    }
    None
}

/// reads a [left bottom right top] rectangle, putting its corners in order.
fn read_box(document: &Document, object: &Object) -> Option<[f32; 4]> {
    let (_, object) = document.dereference(object).ok()?;
    let numbers: Vec<f32> = object
        .as_array()
        .ok()?
        .iter()
        .filter_map(|item| document.dereference(item).ok())
        .filter_map(|(_, item)| number(item))
        .collect();
    if numbers.len() != 4 {
        return None;
    }
    let left = numbers[0].min(numbers[2]);
    let right = numbers[0].max(numbers[2]);
    let bottom = numbers[1].min(numbers[3]);
    let top = numbers[1].max(numbers[3]);
    if right - left < 1.0 || top - bottom < 1.0 {
        return None;
    }
    Some([left, bottom, right, top])
}

/// the overlap of two boxes, or the second when they don't overlap
/// (a crop box outside the paper is ignored, as readers do).
fn intersect(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let left = a[0].max(b[0]);
    let bottom = a[1].max(b[1]);
    let right = a[2].min(b[2]);
    let top = a[3].min(b[3]);
    if right - left < 1.0 || top - bottom < 1.0 {
        b
    } else {
        [left, bottom, right, top]
    }
}

/// a pdf number of either kind as a float.
pub(crate) fn number(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(crop: [f32; 4], degrees: i64) -> PageFrame {
        PageFrame {
            crop,
            turn: QuarterTurn::from_degrees(degrees),
        }
    }

    #[test]
    fn an_unturned_page_flips_y() {
        let page = frame([0.0, 0.0, 612.0, 792.0], 0);
        assert_eq!(page.visible_point(72.0, 720.0), (72.0, 72.0));
        assert_eq!(page.visible_size(), PageSize::new(612.0, 792.0));
    }

    #[test]
    fn a_crop_box_moves_the_origin() {
        let page = frame([36.0, 36.0, 576.0, 756.0], 0);
        assert_eq!(page.visible_point(72.0, 720.0), (36.0, 36.0));
        assert_eq!(page.visible_size(), PageSize::new(540.0, 720.0));
    }

    #[test]
    fn a_page_turned_clockwise_shows_its_top_left_at_the_top_right() {
        let page = frame([0.0, 0.0, 612.0, 792.0], 90);
        assert_eq!(page.visible_size(), PageSize::new(792.0, 612.0));
        // the unturned top left corner (0, 792 in user space)
        assert_eq!(page.visible_point(0.0, 792.0), (792.0, 0.0));
        // the unturned bottom left corner
        assert_eq!(page.visible_point(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn a_page_turned_counter_clockwise_shows_its_top_left_at_the_bottom_left() {
        let page = frame([0.0, 0.0, 612.0, 792.0], 270);
        assert_eq!(page.visible_point(0.0, 792.0), (0.0, 612.0));
    }
}
