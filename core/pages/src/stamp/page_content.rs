//! adding a stamp to a page's drawing instructions, and taking it off again.
//!
//! a page's look comes from its content streams, run one after another.
//! a stamp is added as one more stream of its own, so the page's original
//! streams are never edited (other pages may share them). to make sure
//! the original drawing can't leave a turned or scaled coordinate system
//! behind for the stamp, it is wrapped in a save (q) and restore (Q) pair.
//!
//! every stamp is marked with `/LettersStamp <</Kind /...>> BDC ... EMC`,
//! a "marked content" tag readers ignore, so removing stamps later finds
//! exactly what Letters added and nothing else.

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use crate::changes::Changes;
use crate::tree::{inherited, number, PageFrame};
use crate::PagesError;

use super::{StampKind, StampLayer};

/// the marked content tag on everything Letters draws onto a page.
const STAMP_TAG: &[u8] = b"LettersStamp";

/// a key on the little streams that wrap a page's original content, so
/// they can be recognised and removed with the last stamp.
const WRAP_KEY: &str = "LettersWrap";

/// the most a content stream may expand to when it is read back for
/// removing stamps, so a hostile file can't exhaust memory.
const MAX_CONTENT_BYTES: usize = 64 * 1024 * 1024;

/// an object a stamp's drawing refers to by name, such as its font.
pub(crate) struct NeededResource {
    /// "Font", "ExtGState" or "XObject"
    pub category: &'static str,
    /// the name to use when the page doesn't already use it
    pub base_name: &'static str,
    pub object: ObjectId,
}

/// one stamp for one page.
pub(crate) struct StampDrawing<'a> {
    pub kind: StampKind,
    pub layer: StampLayer,
    pub resources: &'a [NeededResource],
}

/// works out how to add a stamp to a page and records it in `changes`.
///
/// `draw` gets the page's frame and the names its resources ended up with,
/// and returns drawing instructions in upright coordinates: points from the
/// bottom left of the visible page, as the person sees it.
pub(crate) fn plan_stamp(
    doc: &Document,
    page: ObjectId,
    stamp: &StampDrawing,
    changes: &mut Changes,
    draw: impl FnOnce(&PageFrame, &[String]) -> Result<String, PagesError>,
) -> Result<(), PagesError> {
    let frame = PageFrame::of(doc, page);
    let mut resources = own_resources(doc, page);
    let names: Vec<String> = stamp
        .resources
        .iter()
        .map(|needed| add_resource(doc, &mut resources, needed))
        .collect();
    let drawing = draw(&frame, &names)?;

    let mut contents = content_streams(doc, page);
    let already_wrapped = contents.iter().any(|&id| is_wrapper(doc, id));
    if stamp.layer == StampLayer::InFront && !already_wrapped && !contents.is_empty() {
        contents.insert(0, changes.add(wrapper_stream("q\n")));
        contents.push(changes.add(wrapper_stream("\nQ\n")));
    }
    let stamp_stream = changes.add(Stream::new(
        Dictionary::new(),
        marked_stamp(stamp.kind, &frame, &drawing).into_bytes(),
    ));
    match stamp.layer {
        StampLayer::InFront => contents.push(stamp_stream),
        StampLayer::Behind => contents.insert(0, stamp_stream),
    }

    changes.set(page, "Resources", resources);
    changes.set(page, "Contents", references(&contents));
    Ok(())
}

/// the drawing, tagged as a Letters stamp and moved into the upright
/// coordinates the stamp was drawn in.
fn marked_stamp(kind: StampKind, frame: &PageFrame, drawing: &str) -> String {
    let matrix: Vec<String> = frame
        .upright_matrix()
        .iter()
        .map(|&value| number(value))
        .collect();
    format!(
        "\n/{} <</Kind /{}>> BDC\nq\n{} cm\n{drawing}\nQ\nEMC\n",
        String::from_utf8_lossy(STAMP_TAG),
        kind.tag_name(),
        matrix.join(" ")
    )
}

fn wrapper_stream(content: &str) -> Stream {
    let mut dict = Dictionary::new();
    dict.set(WRAP_KEY, Object::Boolean(true));
    Stream::new(dict, content.as_bytes().to_vec())
}

fn is_wrapper(doc: &Document, id: ObjectId) -> bool {
    matches!(doc.get_object(id), Ok(Object::Stream(stream)) if stream.dict.has(WRAP_KEY.as_bytes()))
}

fn references(ids: &[ObjectId]) -> Object {
    Object::Array(ids.iter().map(|&id| Object::Reference(id)).collect())
}

/// a copy of the page's resources that belongs to this page alone.
///
/// resources are often shared between pages, or inherited from the page
/// tree. adding the stamp's font to a shared dictionary would change other
/// pages too, so the page gets its own copy. the objects inside (fonts,
/// images) are still shared by reference, so the copy is small.
fn own_resources(doc: &Document, page: ObjectId) -> Dictionary {
    inherited(doc, page, b"Resources")
        .and_then(|resources| resources.as_dict().ok())
        .cloned()
        .unwrap_or_default()
}

/// adds one resource under a name the page doesn't use yet, and returns it.
fn add_resource(doc: &Document, resources: &mut Dictionary, needed: &NeededResource) -> String {
    let mut category = resources
        .get(needed.category.as_bytes())
        .ok()
        .and_then(|object| doc.dereference(object).ok())
        .and_then(|(_, object)| object.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    let name = free_name(&category, needed.base_name);
    category.set(name.as_bytes(), Object::Reference(needed.object));
    resources.set(needed.category, category);
    name
}

/// `base`, or `base2`, `base3`... whichever the dictionary doesn't have.
fn free_name(dict: &Dictionary, base: &str) -> String {
    if !dict.has(base.as_bytes()) {
        return base.to_owned();
    }
    (2..)
        .map(|suffix| format!("{base}{suffix}"))
        .find(|name| !dict.has(name.as_bytes()))
        .unwrap_or_else(|| base.to_owned())
}

/// the ids of the page's content streams, in drawing order.
fn content_streams(doc: &Document, page: ObjectId) -> Vec<ObjectId> {
    doc.get_page_contents(page)
}

/// works out how to remove one kind of stamp from a page. returns whether
/// the page had any to remove.
pub(crate) fn plan_removal(
    doc: &Document,
    page: ObjectId,
    kind: StampKind,
    changes: &mut Changes,
) -> bool {
    let mut kept = Vec::new();
    let mut removed_any = false;
    let mut other_stamps_left = false;
    for id in content_streams(doc, page) {
        let Some(stamp_content) = stamp_content_of(doc, id) else {
            kept.push(id);
            continue;
        };
        let filtered = without_stamps(&stamp_content, kind);
        other_stamps_left |= filtered.other_stamps_left;
        if !filtered.removed_any {
            kept.push(id);
            continue;
        }
        removed_any = true;
        if filtered.operations.is_empty() {
            continue;
        }
        match (Content {
            operations: filtered.operations,
        })
        .encode()
        {
            // the stream may be shared, so the shortened copy is a new object
            Ok(bytes) => kept.push(changes.add(Stream::new(Dictionary::new(), bytes))),
            Err(_) => kept.push(id),
        }
    }
    if !removed_any {
        return false;
    }
    if !other_stamps_left {
        // with no stamp left, the wrapping pair has nothing to protect
        kept.retain(|&id| !is_wrapper(doc, id));
    }
    changes.set(page, "Contents", references(&kept));
    true
}

/// the operations of a content stream, if it contains any Letters stamp.
fn stamp_content_of(doc: &Document, id: ObjectId) -> Option<Vec<Operation>> {
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let bytes = stream
        .get_plain_content_with_limit(MAX_CONTENT_BYTES)
        .ok()?;
    if !contains(&bytes, STAMP_TAG) {
        return None;
    }
    Content::decode(&bytes)
        .ok()
        .map(|content| content.operations)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

struct Filtered {
    operations: Vec<Operation>,
    removed_any: bool,
    /// stamps of other kinds that stay
    other_stamps_left: bool,
}

/// drops every marked section tagged as a stamp of `kind`, with everything
/// inside it, and keeps all other operations as they were.
fn without_stamps(operations: &[Operation], kind: StampKind) -> Filtered {
    let mut kept = Vec::with_capacity(operations.len());
    let mut removed_any = false;
    let mut other_stamps_left = false;
    // how deeply nested we are inside a section being dropped
    let mut skip_depth = 0usize;
    for operation in operations {
        let opens = matches!(operation.operator.as_str(), "BMC" | "BDC");
        let closes = operation.operator == "EMC";
        if skip_depth > 0 {
            if opens {
                skip_depth += 1;
            } else if closes {
                skip_depth -= 1;
            }
            continue;
        }
        match stamp_kind_of(operation) {
            Some(found) if found == kind.tag_name().as_bytes() => {
                skip_depth = 1;
                removed_any = true;
                continue;
            }
            Some(_) => other_stamps_left = true,
            None => {}
        }
        kept.push(operation.clone());
    }
    Filtered {
        operations: kept,
        removed_any,
        other_stamps_left,
    }
}

/// the kind named by a `/LettersStamp <</Kind /...>> BDC` operation.
fn stamp_kind_of(operation: &Operation) -> Option<&[u8]> {
    if operation.operator != "BDC" {
        return None;
    }
    let [tag, properties] = &operation.operands[..] else {
        return None;
    };
    if tag.as_name().ok()? != STAMP_TAG {
        return None;
    }
    properties.as_dict().ok()?.get(b"Kind").ok()?.as_name().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_names_skip_names_already_taken() {
        let mut dict = Dictionary::new();
        assert_eq!(free_name(&dict, "LettersFont"), "LettersFont");
        dict.set("LettersFont", Object::Null);
        dict.set("LettersFont2", Object::Null);
        assert_eq!(free_name(&dict, "LettersFont"), "LettersFont3");
    }

    #[test]
    fn removing_one_kind_keeps_the_others_and_the_original_drawing() {
        let content = b"0 0 1 rg 0 0 10 10 re f\n\
            /LettersStamp <</Kind /PageNumbers>> BDC q /Span BMC (x) Tj EMC Q EMC\n\
            /LettersStamp <</Kind /Watermark>> BDC q Q EMC";
        let operations = Content::decode(content).unwrap().operations;

        let filtered = without_stamps(&operations, StampKind::PageNumbers);

        assert!(filtered.removed_any);
        assert!(filtered.other_stamps_left);
        let operators: Vec<&str> = filtered
            .operations
            .iter()
            .map(|operation| operation.operator.as_str())
            .collect();
        assert_eq!(operators, ["rg", "re", "f", "BDC", "q", "Q", "EMC"]);
    }
}
