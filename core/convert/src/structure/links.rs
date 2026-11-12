//! the links on a page: link annotations leading to a web address or to
//! another page of the document.

use std::collections::HashMap;

use lopdf::{Document, Object, ObjectId};

use crate::content::bounds;
use crate::geometry::Rect;
use crate::source::{decode_text_string, numbers, resolve, PageSpace};

/// a clickable area and where it leads ("https://..." or "#page=3").
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageLink {
    pub rect: Rect,
    pub target: String,
}

/// reads the page's link annotations. `page_numbers` maps page objects
/// to their zero based number, for links inside the document.
pub(crate) fn page_links(
    doc: &Document,
    page_id: ObjectId,
    space: &PageSpace,
    page_numbers: &HashMap<ObjectId, usize>,
) -> Vec<PageLink> {
    let Ok(page) = doc.get_dictionary(page_id) else {
        return Vec::new();
    };
    let annotations = match page.get(b"Annots").map(|a| resolve(doc, a)) {
        Ok(Object::Array(items)) => items.clone(),
        _ => return Vec::new(),
    };
    annotations
        .iter()
        .filter_map(|annotation| resolve(doc, annotation).as_dict().ok())
        .filter(|annotation| {
            matches!(
                annotation.get(b"Subtype").and_then(Object::as_name),
                Ok(b"Link")
            )
        })
        .filter_map(|annotation| {
            let corners = numbers(resolve(doc, annotation.get(b"Rect").ok()?));
            if corners.len() != 4 {
                return None;
            }
            let points = [
                space.to_shown.apply(corners[0], corners[1]),
                space.to_shown.apply(corners[2], corners[3]),
            ];
            let target = link_target(doc, annotation, page_numbers)?;
            Some(PageLink {
                rect: bounds(&points),
                target,
            })
        })
        .collect()
}

fn link_target(
    doc: &Document,
    annotation: &lopdf::Dictionary,
    page_numbers: &HashMap<ObjectId, usize>,
) -> Option<String> {
    if let Ok(action) = annotation.get(b"A").map(|a| resolve(doc, a)) {
        let action = action.as_dict().ok()?;
        return match action.get(b"S").and_then(Object::as_name).ok()? {
            b"URI" => match resolve(doc, action.get(b"URI").ok()?) {
                Object::String(bytes, _) => Some(decode_text_string(bytes)),
                _ => None,
            },
            b"GoTo" => page_target(doc, action.get(b"D").ok()?, page_numbers),
            _ => None,
        };
    }
    page_target(doc, annotation.get(b"Dest").ok()?, page_numbers)
}

/// an explicit destination names its page object first.
fn page_target(
    doc: &Document,
    destination: &Object,
    page_numbers: &HashMap<ObjectId, usize>,
) -> Option<String> {
    let Object::Array(parts) = resolve(doc, destination) else {
        return None;
    };
    let page = page_numbers.get(&parts.first()?.as_reference().ok()?)?;
    Some(format!("#page={}", page + 1))
}

/// the link under a point, if any.
pub(crate) fn link_at(links: &[PageLink], x: f32, y: f32) -> Option<&str> {
    links
        .iter()
        .find(|link| link.rect.contains_point(x, y))
        .map(|link| link.target.as_str())
}
