//! cropping pages and undoing it.

use lopdf::{Document, Object};
use serde::{Deserialize, Serialize};

use crate::changes::Changes;
use crate::tree::{check_selection, inherited, media_box, page_ids, PageFrame, Rect};
use crate::PagesError;

/// a crop box can't be smaller than the smallest page pdf allows.
const SMALLEST_SIDE: f32 = 3.0;

/// how far outside the page a crop may reach and still be accepted, to
/// allow for rounding in the ui. anything this close is pulled in.
const EDGE_TOLERANCE: f32 = 0.5;

/// the part of the page to keep, in points, measured from the top left of
/// the page as the person sees it (after any rotation, and inside any
/// crop already applied).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// settings for `crop`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CropParams {
    pub pages: Vec<u32>,
    pub rect: CropRect,
}

/// settings for `uncrop`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UncropParams {
    pub pages: Vec<u32>,
}

/// crops pages to `rect` by setting their /CropBox.
///
/// cropping only hides the rest of the page: everything outside the box
/// is still in the file, can be shown again with `uncrop`, and can still
/// be copied out by anyone who has the file. to remove content for good,
/// use redaction instead.
pub fn crop(doc: &mut Document, params: &CropParams) -> Result<(), PagesError> {
    let CropRect {
        left,
        top,
        width,
        height,
    } = params.rect;
    let finite = [left, top, width, height]
        .iter()
        .all(|value| value.is_finite());
    if !finite || width < SMALLEST_SIDE || height < SMALLEST_SIDE {
        return Err(PagesError::CropTooSmall);
    }
    let pages = page_ids(doc)?;
    let chosen = check_selection(&params.pages, pages.len())?;
    let mut changes = Changes::new(doc);
    for position in chosen {
        let page = pages[position];
        let frame = PageFrame::of(doc, page);
        let fits = left >= -EDGE_TOLERANCE
            && top >= -EDGE_TOLERANCE
            && left + width <= frame.shown_width() + EDGE_TOLERANCE
            && top + height <= frame.shown_height() + EDGE_TOLERANCE;
        if !fits {
            return Err(PagesError::CropOutsidePage(position as u32 + 1));
        }
        let crop = crop_box(&frame, params.rect);
        changes.set(page, "CropBox", crop.to_object());
    }
    changes.apply(doc);
    Ok(())
}

/// the crop rectangle in the page's own coordinates, kept inside the
/// visible part of the page.
fn crop_box(frame: &PageFrame, rect: CropRect) -> Rect {
    let (x1, y1) = frame.shown_to_pdf(rect.left, rect.top);
    let (x2, y2) = frame.shown_to_pdf(rect.left + rect.width, rect.top + rect.height);
    let crop = Rect {
        left: x1.min(x2),
        bottom: y1.min(y2),
        right: x1.max(x2),
        top: y1.max(y2),
    };
    // the size check above makes the overlap non-empty
    crop.intersect(&frame.visible).unwrap_or(frame.visible)
}

/// shows the whole page again.
pub fn uncrop(doc: &mut Document, params: &UncropParams) -> Result<(), PagesError> {
    let pages = page_ids(doc)?;
    let chosen = check_selection(&params.pages, pages.len())?;
    let mut changes = Changes::new(doc);
    for position in chosen {
        let page = pages[position];
        changes.remove(page, "CropBox");
        // a crop box inherited from the page tree would still apply once
        // the page's own is gone, so the page is given the full paper size
        if inherited_from_parent(doc, page) {
            changes.set(page, "CropBox", media_box(doc, page).to_object());
        }
    }
    changes.apply(doc);
    Ok(())
}

fn inherited_from_parent(doc: &Document, page: lopdf::ObjectId) -> bool {
    doc.get_dictionary(page)
        .and_then(|dict| dict.get(b"Parent"))
        .and_then(Object::as_reference)
        .is_ok_and(|parent| inherited(doc, parent, b"CropBox").is_some())
}
