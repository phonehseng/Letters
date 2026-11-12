//! finding a page in a pdf and converting between what the person sees
//! and pdf user space.
//!
//! the app measures from the top left corner of the page as it is shown,
//! with y going down. a pdf measures from the bottom left corner of the
//! page as it is stored, with y going up, before the page's /Rotate turns
//! it and its /CropBox cuts it. pdfium draws pages as they are shown, so a
//! picture of a page, and every word found in it, is in the shown space.

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::geometry::Affine;
use crate::OcrError;

/// how many /Parent links are followed when looking for an inherited page
/// setting. real files have a handful; a loop must not hang us.
const MAX_PAGE_TREE_DEPTH: usize = 64;

/// the id of the page at zero based position `page`.
pub fn page_id(doc: &Document, page: u32) -> Result<ObjectId, OcrError> {
    doc.get_pages()
        .values()
        .nth(page as usize)
        .copied()
        .ok_or(OcrError::UnknownPage(page))
}

/// the page as the person sees it, and how to get to pdf user space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSpace {
    /// shown coordinates (points from the top left) to pdf user space
    pub to_user: Affine,
    /// size of the page as shown, in points
    pub width: f32,
    pub height: f32,
    /// the visible box in user space: left, bottom, right, top
    pub visible_box: [f32; 4],
}

impl PageSpace {
    /// reads the crop box, media box and rotation of a page, following the
    /// page tree for values a page inherits from its parents.
    pub fn of_page(doc: &Document, page_id: ObjectId) -> Result<Self, OcrError> {
        let page = doc
            .get_dictionary(page_id)
            .map_err(|_| OcrError::Damaged("a page is missing".into()))?;
        let media = inherited(doc, page, b"MediaBox")
            .and_then(|object| rect_values(doc, object))
            // us letter is what most readers assume for a page without one
            .unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let crop = inherited(doc, page, b"CropBox")
            .and_then(|object| rect_values(doc, object))
            .and_then(|crop| intersect(crop, media))
            .unwrap_or(media);
        let rotation = inherited(doc, page, b"Rotate")
            .and_then(|object| number(doc, object))
            .unwrap_or(0.0);
        Ok(Self::new(crop, rotation as i64))
    }

    /// the conversion for a visible box in user space, shown turned
    /// clockwise by `rotation` degrees.
    pub fn new(visible_box: [f32; 4], rotation: i64) -> Self {
        let [x0, y0, x1, y1] = visible_box;
        let (width, height) = (x1 - x0, y1 - y0);
        // a /Rotate that isn't a multiple of 90 is invalid; readers ignore it
        let rotation = match rotation.rem_euclid(360) {
            turn @ (90 | 180 | 270) => turn,
            _ => 0,
        };
        // each case says where a step right and a step down on screen go in
        // user space, starting from the shown top left corner
        let to_user = match rotation {
            90 => Affine::new(0.0, 1.0, 1.0, 0.0, x0, y0),
            180 => Affine::new(-1.0, 0.0, 0.0, 1.0, x1, y0),
            270 => Affine::new(0.0, -1.0, -1.0, 0.0, x1, y1),
            _ => Affine::new(1.0, 0.0, 0.0, -1.0, x0, y1),
        };
        let (width, height) = if rotation % 180 == 0 {
            (width, height)
        } else {
            (height, width)
        };
        Self {
            to_user,
            width,
            height,
            visible_box,
        }
    }
}

/// a page setting from the page itself or the nearest parent that has it.
pub fn inherited<'a>(doc: &'a Document, page: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    let mut current = page;
    for _ in 0..MAX_PAGE_TREE_DEPTH {
        if let Ok(value) = current.get(key) {
            return Some(value);
        }
        let parent = current.get(b"Parent").ok()?.as_reference().ok()?;
        current = doc.get_dictionary(parent).ok()?;
    }
    None
}

/// a number, following a reference if the value is one.
pub fn number(doc: &Document, object: &Object) -> Option<f32> {
    let (_, object) = doc.dereference(object).ok()?;
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

/// a pdf rectangle as left, bottom, right, top, whichever corners it gives.
fn rect_values(doc: &Document, object: &Object) -> Option<[f32; 4]> {
    let (_, object) = doc.dereference(object).ok()?;
    let values: Vec<f32> = object
        .as_array()
        .ok()?
        .iter()
        .filter_map(|value| number(doc, value))
        .collect();
    let [a, b, c, d]: [f32; 4] = values.try_into().ok()?;
    Some([a.min(c), b.min(d), a.max(c), b.max(d)])
}

fn intersect(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
    let joint = [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ];
    (joint[2] > joint[0] && joint[3] > joint[1]).then_some(joint)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use lopdf::dictionary;

    fn close(a: Point, b: Point) -> bool {
        a.distance_to(b) < 1e-3
    }

    #[test]
    fn unturned_pages_flip_y() {
        let space = PageSpace::new([0.0, 0.0, 612.0, 792.0], 0);
        assert!(close(
            space.to_user.apply(Point::new(0.0, 0.0)),
            Point::new(0.0, 792.0)
        ));
        assert!(close(
            space.to_user.apply(Point::new(72.0, 72.0)),
            Point::new(72.0, 720.0)
        ));
    }

    #[test]
    fn a_crop_box_moves_the_origin() {
        let space = PageSpace::new([50.0, 100.0, 550.0, 700.0], 0);
        assert_eq!((space.width, space.height), (500.0, 600.0));
        assert!(close(
            space.to_user.apply(Point::new(0.0, 0.0)),
            Point::new(50.0, 700.0)
        ));
    }

    #[test]
    fn turned_pages_swap_sides() {
        // shown turned clockwise: the stored bottom left is the shown top left
        let space = PageSpace::new([0.0, 0.0, 612.0, 792.0], 90);
        assert_eq!((space.width, space.height), (792.0, 612.0));
        assert!(close(
            space.to_user.apply(Point::new(0.0, 0.0)),
            Point::new(0.0, 0.0)
        ));
        assert!(close(
            space.to_user.apply(Point::new(792.0, 0.0)),
            Point::new(0.0, 792.0)
        ));
        let upside_down = PageSpace::new([0.0, 0.0, 612.0, 792.0], 180);
        assert!(close(
            upside_down.to_user.apply(Point::new(0.0, 0.0)),
            Point::new(612.0, 0.0)
        ));
        let anticlockwise = PageSpace::new([0.0, 0.0, 612.0, 792.0], 270);
        assert!(close(
            anticlockwise.to_user.apply(Point::new(0.0, 0.0)),
            Point::new(612.0, 792.0)
        ));
    }

    #[test]
    fn settings_are_inherited_from_the_page_tree() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(dictionary! { "Type" => "Page", "Parent" => pages_id });
        doc.objects.insert(
            pages_id,
            dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
                "MediaBox" => vec![0.into(), 0.into(), 300.into(), 400.into()],
                "Rotate" => 270,
            }
            .into(),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let space = PageSpace::of_page(&doc, page_id).unwrap();
        assert_eq!((space.width, space.height), (400.0, 300.0));
        assert_eq!(page_id_at(&doc, 0), Some(page_id));
        assert_eq!(super::page_id(&doc, 3), Err(OcrError::UnknownPage(3)));
    }

    fn page_id_at(doc: &Document, page: u32) -> Option<ObjectId> {
        super::page_id(doc, page).ok()
    }
}
