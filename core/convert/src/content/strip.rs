//! copies of a document drawn without some of their content, for the
//! background pictures of html pages and slides.
//!
//! html and slides show text as real text, so the picture behind it must
//! not show the text a second time. the copy keeps every operator that
//! changes the drawing state and drops only the ones that paint what
//! the writer puts back as real objects: text shows, and (for slides)
//! the shapes and pictures it rebuilt.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use lopdf::content::{Content, Operation};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

use super::{decode_stream, page_content_streams, OpRef};
use crate::source::{inherited, resolve};
use crate::ConvertError;

/// what to leave out of one page, besides all of its text.
#[derive(Clone, Debug, Default)]
pub(crate) struct Removals {
    /// paths drawn again as shapes: painted with `n` (nothing) instead
    pub paths: HashSet<OpRef>,
    /// pictures placed again as pictures: their `Do` is dropped
    pub images: HashSet<OpRef>,
}

/// a pdf file in the temporary folder, deleted when this is dropped.
pub(crate) struct TempPdf {
    path: PathBuf,
}

impl TempPdf {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempPdf {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

static NEXT_COPY: AtomicUsize = AtomicUsize::new(0);

/// writes a copy of `doc` in which each listed page is drawn without its
/// text and without what its `Removals` name. other pages are untouched.
pub(crate) fn write_background_copy(
    doc: &Document,
    pages: &[(ObjectId, Removals)],
) -> Result<TempPdf, ConvertError> {
    let mut copy = doc.clone();
    let mut forms = HashSet::new();
    for (page_id, removals) in pages {
        strip_page(&mut copy, *page_id, removals)?;
        collect_forms(&copy, *page_id, &mut forms);
    }
    for form in forms {
        strip_form_text(&mut copy, form);
    }
    let folder = std::env::temp_dir().join("letters-convert");
    std::fs::create_dir_all(&folder)
        .map_err(|error| ConvertError::RenderFailed(error.to_string()))?;
    let number = NEXT_COPY.fetch_add(1, Ordering::SeqCst);
    let path = folder.join(format!("{}-{number}-background.pdf", std::process::id()));
    let temp = TempPdf { path };
    copy.save(temp.path())
        .map_err(|error| ConvertError::RenderFailed(error.to_string()))?;
    Ok(temp)
}

fn is_text_show(operator: &str) -> bool {
    matches!(operator, "Tj" | "TJ" | "'" | "\"")
}

/// replaces the page's content with one new stream, so streams shared
/// with other pages stay as they were.
fn strip_page(
    doc: &mut Document,
    page_id: ObjectId,
    removals: &Removals,
) -> Result<(), ConvertError> {
    let page = doc
        .get_dictionary(page_id)
        .map_err(|_| ConvertError::Damaged("a page is missing".into()))?;
    let streams = page_content_streams(doc, page);
    let mut kept: Vec<Operation> = Vec::new();
    for (index, stream) in streams.into_iter().enumerate() {
        for (op_index, operation) in decode_stream(doc, stream).into_iter().enumerate() {
            let at = (index, op_index);
            if is_text_show(&operation.operator) || removals.images.contains(&at) {
                continue;
            }
            if removals.paths.contains(&at) {
                kept.push(Operation::new("n", Vec::new()));
            } else {
                kept.push(operation);
            }
        }
    }
    let bytes = Content { operations: kept }
        .encode()
        .map_err(|error| ConvertError::Damaged(error.to_string()))?;
    let stream_id = doc.add_object(Stream::new(Dictionary::new(), bytes));
    let page = doc
        .get_dictionary_mut(page_id)
        .map_err(|_| ConvertError::Damaged("a page is missing".into()))?;
    page.set("Contents", stream_id);
    Ok(())
}

/// every form the page draws, directly or inside other forms. forms in
/// annotations are not reached this way, so their text stays (it isn't
/// part of the page's own text either).
fn collect_forms(doc: &Document, page_id: ObjectId, found: &mut HashSet<ObjectId>) {
    let Ok(page) = doc.get_dictionary(page_id) else {
        return;
    };
    let mut waiting: Vec<&Dictionary> = inherited(doc, page, b"Resources")
        .and_then(|r| r.as_dict().ok())
        .into_iter()
        .collect();
    while let Some(resources) = waiting.pop() {
        let Some(xobjects) = resources
            .get(b"XObject")
            .ok()
            .and_then(|x| resolve(doc, x).as_dict().ok())
        else {
            continue;
        };
        for (_, object) in xobjects.iter() {
            let Ok(id) = object.as_reference() else {
                continue;
            };
            let Ok(stream) = doc.get_object(id).and_then(Object::as_stream) else {
                continue;
            };
            let is_form = matches!(
                stream.dict.get(b"Subtype").and_then(Object::as_name),
                Ok(b"Form")
            );
            if is_form && found.insert(id) {
                if let Some(inner) = stream
                    .dict
                    .get(b"Resources")
                    .ok()
                    .and_then(|r| resolve(doc, r).as_dict().ok())
                {
                    waiting.push(inner);
                }
            }
        }
    }
}

fn strip_form_text(doc: &mut Document, form: ObjectId) {
    let operations = decode_stream(doc, form);
    if !operations.iter().any(|op| is_text_show(&op.operator)) {
        return;
    }
    let kept: Vec<Operation> = operations
        .into_iter()
        .filter(|op| !is_text_show(&op.operator))
        .collect();
    let Ok(bytes) = (Content { operations: kept }).encode() else {
        return;
    };
    if let Ok(Object::Stream(stream)) = doc.get_object_mut(form) {
        // the new content is stored uncompressed, so its old filters go
        stream.dict.remove(b"Filter");
        stream.dict.remove(b"DecodeParms");
        stream.set_content(bytes);
    }
}

/// a count of the painting operators left on each page of a background
/// copy plan, used to skip drawing a background that would be blank.
pub(crate) fn remaining_paint(drawing: &super::PageDrawing, removals: &Removals) -> usize {
    let paths = drawing
        .paths
        .iter()
        .filter(|path| path.origin.is_none_or(|at| !removals.paths.contains(&at)))
        .count();
    let images = drawing
        .images
        .iter()
        .filter(|image| image.origin.is_none_or(|at| !removals.images.contains(&at)))
        .count();
    paths + images + drawing.other_paint
}

/// for tests: which operators of a page survive in a copy.
#[cfg(test)]
pub(crate) fn operators_of_page(doc: &Document, page_id: ObjectId) -> Vec<String> {
    let page = doc.get_dictionary(page_id).unwrap();
    page_content_streams(doc, page)
        .into_iter()
        .flat_map(|stream| decode_stream(doc, stream))
        .map(|op| op.operator)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_and_chosen_paint_are_left_out() {
        let content = "BT /F1 12 Tf 1 0 0 rg 72 700 Td (Hello) Tj ET 0 0 m 10 0 l S 10 10 50 50 re f q 10 0 0 10 0 0 cm /Im1 Do Q";
        let (doc, page_id) = crate::test_pdf::one_page_document(content);

        // a path is named by its painting operator: 10 is the f after
        // the re, and 13 is the Do
        let removals = Removals {
            paths: [(0, 10)].into_iter().collect(),
            images: [(0, 13)].into_iter().collect(),
        };
        let copy = write_background_copy(&doc, &[(page_id, removals)]).unwrap();
        let saved = Document::load(copy.path()).unwrap();
        let page = *saved.get_pages().values().next().unwrap();
        let operators = operators_of_page(&saved, page);
        assert_eq!(
            operators,
            ["BT", "Tf", "rg", "Td", "ET", "m", "l", "S", "re", "n", "q", "cm", "Q"]
        );
        // the original is untouched
        assert!(operators_of_page(&doc, page_id).contains(&"Tj".to_string()));
        let path = copy.path().to_path_buf();
        drop(copy);
        assert!(!path.exists());
    }
}
