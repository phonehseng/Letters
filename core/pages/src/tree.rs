//! small helpers for reading and rewriting the page tree.
//!
//! a pdf keeps its pages in a tree: the catalog points at a root /Pages
//! node, which has /Kids that are either pages or more /Pages nodes.
//! some page settings (resources, boxes, rotation) can live on a parent
//! node and be inherited by every page under it. everything in this crate
//! goes through these helpers, so that knowledge lives in one place.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::PagesError;

/// the page settings a page may inherit from the nodes above it.
pub(crate) const INHERITABLE_KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// a real page tree is a few levels deep. this stops a damaged file whose
/// parents point in a circle from looping forever.
const MAX_TREE_DEPTH: usize = 256;

/// the page size readers assume when a file names none: us letter.
const DEFAULT_MEDIA_BOX: Rect = Rect {
    left: 0.0,
    bottom: 0.0,
    right: 612.0,
    top: 792.0,
};

/// refuses to work on a file that still needs its password.
///
/// lopdf opens files with an empty password by itself and removes the
/// /Encrypt entry when it does, so an entry still present means the
/// contents are scrambled and any change would ruin the file.
pub(crate) fn ensure_unlocked(doc: &Document) -> Result<(), PagesError> {
    if doc.trailer.has(b"Encrypt") {
        return Err(PagesError::Locked);
    }
    Ok(())
}

/// the id of the root /Pages node the catalog points at.
pub(crate) fn pages_root(doc: &Document) -> Result<ObjectId, PagesError> {
    let catalog = doc
        .catalog()
        .map_err(|_| PagesError::Damaged("the catalog is missing".into()))?;
    catalog
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| PagesError::Damaged("the catalog has no page list".into()))
}

/// every page in display order.
///
/// walks the tree depth first, the order readers show pages in. a node
/// with /Kids is a branch, anything else is a page. a node seen twice is
/// skipped so a damaged tree can't make a page appear twice or loop.
pub(crate) fn page_ids(doc: &Document) -> Result<Vec<ObjectId>, PagesError> {
    ensure_unlocked(doc)?;
    let root = pages_root(doc)?;
    let mut pages = Vec::new();
    let mut seen = HashSet::new();
    let mut to_visit = vec![(root, 0)];
    while let Some((node, depth)) = to_visit.pop() {
        if depth > MAX_TREE_DEPTH || !seen.insert(node) {
            continue;
        }
        let Ok(dict) = doc.get_dictionary(node) else {
            continue;
        };
        match kids_of(doc, dict) {
            Some(kids) => {
                // pushed in reverse so the first kid is visited first
                for kid in kids.into_iter().rev() {
                    to_visit.push((kid, depth + 1));
                }
            }
            None if node != root => pages.push(node),
            None => {}
        }
    }
    Ok(pages)
}

/// the /Kids of a branch node, or none for a page.
fn kids_of(doc: &Document, node: &Dictionary) -> Option<Vec<ObjectId>> {
    let kids = node.get(b"Kids").ok()?;
    let (_, kids) = doc.dereference(kids).ok()?;
    let kids = kids.as_array().ok()?;
    Some(
        kids.iter()
            .filter_map(|kid| kid.as_reference().ok())
            .collect(),
    )
}

/// every branch node under the root (not the root itself and not pages).
fn branch_ids(doc: &Document, root: ObjectId) -> Vec<ObjectId> {
    let mut branches = Vec::new();
    let mut seen = HashSet::new();
    let mut to_visit = vec![(root, 0)];
    while let Some((node, depth)) = to_visit.pop() {
        if depth > MAX_TREE_DEPTH || !seen.insert(node) {
            continue;
        }
        let Ok(dict) = doc.get_dictionary(node) else {
            continue;
        };
        if let Some(kids) = kids_of(doc, dict) {
            if node != root {
                branches.push(node);
            }
            to_visit.extend(kids.into_iter().map(|kid| (kid, depth + 1)));
        }
    }
    branches
}

/// a setting as written on the page or the nearest node above it that has
/// it, without following a reference (so a shared dictionary stays shared).
pub(crate) fn inherited_raw<'a>(
    doc: &'a Document,
    page: ObjectId,
    key: &[u8],
) -> Option<&'a Object> {
    let mut node = page;
    for _ in 0..MAX_TREE_DEPTH {
        let dict = doc.get_dictionary(node).ok()?;
        if let Ok(value) = dict.get(key) {
            return Some(value);
        }
        node = dict.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

/// like `inherited_raw`, but with a reference followed to what it points at.
pub(crate) fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let raw = inherited_raw(doc, page, key)?;
    doc.dereference(raw).ok().map(|(_, object)| object)
}

/// a rectangle in pdf units, with its sides in order whatever order the
/// file wrote the corners in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl Rect {
    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.top - self.bottom
    }

    /// the overlap of two rectangles, or none when they don't overlap.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let overlap = Rect {
            left: self.left.max(other.left),
            bottom: self.bottom.max(other.bottom),
            right: self.right.min(other.right),
            top: self.top.min(other.top),
        };
        (overlap.width() > 0.0 && overlap.height() > 0.0).then_some(overlap)
    }

    pub fn to_object(self) -> Object {
        Object::Array(vec![
            self.left.into(),
            self.bottom.into(),
            self.right.into(),
            self.top.into(),
        ])
    }

    /// reads a box written as four numbers. numbers may be references.
    pub fn from_object(doc: &Document, object: &Object) -> Option<Rect> {
        let (_, object) = doc.dereference(object).ok()?;
        let numbers: Vec<f32> = object
            .as_array()
            .ok()?
            .iter()
            .filter_map(|number| doc.dereference(number).ok()?.1.as_float().ok())
            .collect();
        let [x1, y1, x2, y2] = numbers[..] else {
            return None;
        };
        Some(Rect {
            left: x1.min(x2),
            bottom: y1.min(y2),
            right: x1.max(x2),
            top: y1.max(y2),
        })
    }
}

/// the paper size of a page.
pub(crate) fn media_box(doc: &Document, page: ObjectId) -> Rect {
    inherited(doc, page, b"MediaBox")
        .and_then(|object| Rect::from_object(doc, object))
        .filter(|rect| rect.width() > 0.0 && rect.height() > 0.0)
        .unwrap_or(DEFAULT_MEDIA_BOX)
}

/// the part of the page readers show: the crop box where it overlaps the
/// paper, or the whole paper when there is no usable crop box.
pub(crate) fn visible_box(doc: &Document, page: ObjectId) -> Rect {
    let media = media_box(doc, page);
    inherited(doc, page, b"CropBox")
        .and_then(|object| Rect::from_object(doc, object))
        .and_then(|crop| crop.intersect(&media))
        .unwrap_or(media)
}

/// how far the page is turned clockwise when shown: 0, 90, 180 or 270.
///
/// the pdf rules only allow multiples of 90; anything else is treated as
/// no turn, which is what readers do.
pub(crate) fn rotation(doc: &Document, page: ObjectId) -> u16 {
    let degrees = inherited(doc, page, b"Rotate")
        .and_then(|object| object.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    if degrees % 90 == 0 {
        degrees as u16
    } else {
        0
    }
}

/// a page as the person sees it: which part of the paper is visible and
/// how it is turned. it converts between what the person sees (points from
/// the visible top left) and the pdf's own coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PageFrame {
    pub visible: Rect,
    pub rotation: u16,
}

impl PageFrame {
    pub fn of(doc: &Document, page: ObjectId) -> Self {
        Self {
            visible: visible_box(doc, page),
            rotation: rotation(doc, page),
        }
    }

    fn turned_sideways(&self) -> bool {
        self.rotation == 90 || self.rotation == 270
    }

    /// the width of the page as shown on screen, after turning.
    pub fn shown_width(&self) -> f32 {
        if self.turned_sideways() {
            self.visible.height()
        } else {
            self.visible.width()
        }
    }

    pub fn shown_height(&self) -> f32 {
        if self.turned_sideways() {
            self.visible.width()
        } else {
            self.visible.height()
        }
    }

    /// turns a point measured from the shown top left (y going down) into
    /// pdf coordinates.
    pub fn shown_to_pdf(&self, across: f32, down: f32) -> (f32, f32) {
        let Rect {
            left,
            bottom,
            right,
            top,
        } = self.visible;
        match self.rotation {
            // turned a quarter clockwise: the paper's bottom left corner is
            // now the top left, and its left edge runs along the top
            90 => (left + down, bottom + across),
            180 => (right - across, bottom + down),
            270 => (right - down, top - across),
            _ => (left + across, top - down),
        }
    }

    /// the matrix (for the `cm` operator) that lets a stamp be drawn as if
    /// the page were upright and its visible part started at 0,0 in the
    /// bottom left, whatever the page's rotation and boxes really are.
    pub fn upright_matrix(&self) -> [f32; 6] {
        let Rect {
            left,
            bottom,
            right,
            top,
        } = self.visible;
        match self.rotation {
            90 => [0.0, 1.0, -1.0, 0.0, right, bottom],
            180 => [-1.0, 0.0, 0.0, -1.0, right, top],
            270 => [0.0, -1.0, 1.0, 0.0, left, top],
            _ => [1.0, 0.0, 0.0, 1.0, left, bottom],
        }
    }
}

/// checks a list of chosen pages against the page count: at least one,
/// all of them real pages, none twice. returns them as list positions.
pub(crate) fn check_selection(chosen: &[u32], page_count: usize) -> Result<Vec<usize>, PagesError> {
    if chosen.is_empty() {
        return Err(PagesError::NoPagesChosen);
    }
    let mut seen = HashSet::new();
    let mut positions = Vec::with_capacity(chosen.len());
    for &page in chosen {
        let position = check_page(page, page_count)?;
        if !seen.insert(position) {
            return Err(PagesError::PageChosenTwice(page.saturating_add(1)));
        }
        positions.push(position);
    }
    Ok(positions)
}

/// checks that one page index names a real page.
pub(crate) fn check_page(page: u32, page_count: usize) -> Result<usize, PagesError> {
    let position = page as usize;
    if position >= page_count {
        return Err(PagesError::NoSuchPage(page.saturating_add(1)));
    }
    Ok(position)
}

/// the page settings that must be written on a page so it looks the same
/// once it no longer sits under its old parents.
fn settings_to_make_explicit(doc: &Document, page: ObjectId) -> Vec<(&'static [u8], Object)> {
    let Ok(dict) = doc.get_dictionary(page) else {
        return Vec::new();
    };
    INHERITABLE_KEYS
        .iter()
        .filter(|key| !dict.has(key))
        .filter_map(|&key| Some((key, inherited_raw(doc, page, key)?.clone())))
        .collect()
}

/// rewrites the page tree as one flat list holding exactly `order`.
///
/// inherited settings are first copied onto each page, so flattening can't
/// change how any page looks, and then the old branch nodes are dropped.
/// one flat list is valid pdf, every reader handles it, and it keeps every
/// later operation simple. callers check `order` beforehand; this only
/// fails when the catalog itself is unreadable, before anything changes.
pub(crate) fn set_page_order(doc: &mut Document, order: &[ObjectId]) -> Result<(), PagesError> {
    let root = pages_root(doc)?;
    doc.get_dictionary(root)
        .map_err(|_| PagesError::Damaged("the page list can’t be read".into()))?;

    let explicit: Vec<_> = order
        .iter()
        .map(|&page| (page, settings_to_make_explicit(doc, page)))
        .collect();
    let branches = branch_ids(doc, root);

    for (page, settings) in explicit {
        if let Ok(dict) = doc.get_dictionary_mut(page) {
            for (key, value) in settings {
                dict.set(key, value);
            }
            dict.set("Type", Object::Name(b"Page".to_vec()));
            dict.set("Parent", Object::Reference(root));
        }
    }
    for branch in branches {
        doc.objects.remove(&branch);
    }
    if let Ok(root_dict) = doc.get_dictionary_mut(root) {
        for key in INHERITABLE_KEYS {
            root_dict.remove(key);
        }
        root_dict.remove(b"Parent");
        root_dict.set("Type", Object::Name(b"Pages".to_vec()));
        root_dict.set(
            "Kids",
            Object::Array(order.iter().map(|&page| Object::Reference(page)).collect()),
        );
        root_dict.set("Count", Object::Integer(order.len() as i64));
    }
    Ok(())
}

/// formats a number for a content stream: no exponent, no needless zeros.
pub(crate) fn number(value: f32) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded == rounded.trunc() {
        format!("{}", rounded as i64)
    } else {
        let text = format!("{rounded:.3}");
        text.trim_end_matches('0').to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rotation: u16) -> PageFrame {
        PageFrame {
            visible: Rect {
                left: 10.0,
                bottom: 20.0,
                right: 210.0,
                top: 120.0,
            },
            rotation,
        }
    }

    /// applies a `cm` matrix to a point.
    fn apply(matrix: [f32; 6], x: f32, y: f32) -> (f32, f32) {
        (
            matrix[0] * x + matrix[2] * y + matrix[4],
            matrix[1] * x + matrix[3] * y + matrix[5],
        )
    }

    #[test]
    fn sideways_pages_swap_their_shown_size() {
        assert_eq!(
            (frame(0).shown_width(), frame(0).shown_height()),
            (200.0, 100.0)
        );
        assert_eq!(
            (frame(90).shown_width(), frame(90).shown_height()),
            (100.0, 200.0)
        );
    }

    #[test]
    fn the_shown_top_left_is_the_right_paper_corner() {
        assert_eq!(frame(0).shown_to_pdf(0.0, 0.0), (10.0, 120.0));
        assert_eq!(frame(90).shown_to_pdf(0.0, 0.0), (10.0, 20.0));
        assert_eq!(frame(180).shown_to_pdf(0.0, 0.0), (210.0, 20.0));
        assert_eq!(frame(270).shown_to_pdf(0.0, 0.0), (210.0, 120.0));
    }

    #[test]
    fn the_upright_matrix_agrees_with_the_point_conversion() {
        for rotation in [0, 90, 180, 270] {
            let frame = frame(rotation);
            let height = frame.shown_height();
            for (across, down) in [(0.0, 0.0), (30.0, 5.0), (frame.shown_width(), height)] {
                // the upright system measures up from the bottom
                let upright = apply(frame.upright_matrix(), across, height - down);
                assert_eq!(
                    upright,
                    frame.shown_to_pdf(across, down),
                    "rotation {rotation}"
                );
            }
        }
    }

    #[test]
    fn numbers_are_written_plainly() {
        assert_eq!(number(12.0), "12");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(-3.25), "-3.25");
        assert_eq!(number(1.0 / 3.0), "0.333");
    }

    #[test]
    fn selections_are_checked() {
        assert_eq!(check_selection(&[2, 0], 3), Ok(vec![2, 0]));
        assert_eq!(check_selection(&[], 3), Err(PagesError::NoPagesChosen));
        assert_eq!(check_selection(&[3], 3), Err(PagesError::NoSuchPage(4)));
        assert_eq!(
            check_selection(&[1, 1], 3),
            Err(PagesError::PageChosenTwice(2))
        );
    }
}
