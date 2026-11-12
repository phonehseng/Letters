//! the order the tab key moves through a page's fields (/Tabs, iso 32000
//! 12.5 table 30) and the page's annotation list kept in the same order.

use lopdf::{Document, Object, ObjectId};
use serde::{Deserialize, Serialize};

use crate::error::{FormError, FormResult};
use crate::geometry::VisibleRect;
use crate::objects::{check_document, get_rect, name, page_frame, page_id, references_in};
use crate::tree::collect_fields;

/// how the tab key moves through a page's fields.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TabOrder {
    /// across each row, rows from the top (/Tabs /R)
    Rows,
    /// down each column, columns from the left (/Tabs /C)
    Columns,
    /// the order of the document's tag structure (/Tabs /S), for tagged files
    Structure,
    /// exactly this order of field names; fields not named follow in their
    /// current order
    Manual { fields: Vec<String> },
}

/// sets the tab order of one page. the page's annotation list is put in
/// the same order, so readers that ignore /Tabs agree.
pub fn set_tab_order(doc: &mut Document, page: u32, order: &TabOrder) -> FormResult<()> {
    check_document(doc)?;
    let page_id = page_id(doc, page)?;
    let annotations = references_in(doc, doc.get_dictionary(page_id)?, b"Annots");
    let frame = page_frame(doc, page_id);
    let rect_of = |doc: &Document, id: ObjectId| {
        doc.get_dictionary(id)
            .ok()
            .and_then(|dict| get_rect(doc, dict, b"Rect"))
            .map(|rect| frame.to_visible_rect(&rect))
    };
    let mut sorted = annotations.clone();
    let tabs = match order {
        TabOrder::Rows | TabOrder::Structure => {
            sorted.sort_by(|a, b| reading_order(rect_of(doc, *a), rect_of(doc, *b), false));
            if matches!(order, TabOrder::Rows) {
                "R"
            } else {
                "S"
            }
        }
        TabOrder::Columns => {
            sorted.sort_by(|a, b| reading_order(rect_of(doc, *a), rect_of(doc, *b), true));
            "C"
        }
        TabOrder::Manual { fields } => {
            let nodes = collect_fields(doc);
            let mut chosen: Vec<ObjectId> = Vec::new();
            for wanted in fields {
                let node = nodes
                    .iter()
                    .find(|node| &node.name == wanted)
                    .ok_or_else(|| FormError::UnknownField(wanted.clone()))?;
                chosen.extend(node.widgets.iter().filter(|w| annotations.contains(w)));
            }
            let rest: Vec<ObjectId> = annotations
                .iter()
                .filter(|id| !chosen.contains(id))
                .copied()
                .collect();
            sorted = chosen;
            sorted.extend(rest);
            ""
        }
    };
    let dict = doc.get_dictionary_mut(page_id)?;
    dict.set(
        "Annots",
        Object::Array(sorted.into_iter().map(Object::Reference).collect()),
    );
    if tabs.is_empty() {
        // no /Tabs: readers follow the annotation list, which is now the manual order
        dict.remove(b"Tabs");
    } else {
        dict.set("Tabs", name(tabs));
    }
    Ok(())
}

/// rows (or columns) first, with half a box of tolerance for "the same row".
fn reading_order(
    a: Option<VisibleRect>,
    b: Option<VisibleRect>,
    columns: bool,
) -> std::cmp::Ordering {
    let (Some(a), Some(b)) = (a, b) else {
        return std::cmp::Ordering::Equal;
    };
    let (a_main, a_cross, b_main, b_cross, size) = if columns {
        (a.x, a.y, b.x, b.y, a.width.min(b.width))
    } else {
        (a.y, a.x, b.y, b.x, a.height.min(b.height))
    };
    if (a_main - b_main).abs() <= size / 2.0 {
        a_cross.total_cmp(&b_cross)
    } else {
        a_main.total_cmp(&b_main)
    }
}

/// the field names of a page in tab order (each field once).
pub fn tab_order(doc: &Document, page: u32) -> FormResult<Vec<String>> {
    check_document(doc)?;
    let page_id = page_id(doc, page)?;
    let annotations = references_in(doc, doc.get_dictionary(page_id)?, b"Annots");
    let nodes = collect_fields(doc);
    let mut names = Vec::new();
    for annotation in annotations {
        if let Some(node) = nodes.iter().find(|node| node.widgets.contains(&annotation)) {
            if !names.contains(&node.name) {
                names.push(node.name.clone());
            }
        }
    }
    Ok(names)
}
