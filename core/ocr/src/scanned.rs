//! telling whether a page is a scan, so the app can offer to read its text.
//!
//! a scanned page is a picture of a page: one image covering most of it,
//! and little or no real text. this walks the page's drawing instructions,
//! adding up how much of the page images cover and counting the characters
//! of text, separately for visible text and for invisible text (which is
//! what an earlier recognition left behind).

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object};
use serde::Serialize;

use crate::geometry::{Affine, Point, Rect};
use crate::pdf_page::{inherited, page_id, PageSpace};
use crate::text_layer::LAYER_TAG;
use crate::OcrError;

/// a page counts as scanned when images cover at least this share of it...
const SCANNED_COVERAGE: f32 = 0.5;
/// ...and it has fewer visible characters than this (a page number or a
/// scanner's date stamp is allowed).
const MOST_VISIBLE_CHARACTERS: usize = 40;

/// how deep pictures inside drawings inside drawings are followed.
const MAX_NESTING: usize = 4;

/// what a page is made of, as far as recognition cares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageMakeup {
    /// how much of the visible page images cover, 0 to 1
    pub image_coverage: f32,
    /// characters of text a reader shows
    pub visible_characters: usize,
    /// characters of text drawn invisibly, as recognition does
    pub invisible_characters: usize,
    /// whether the page holds a text layer from Letters' recognition
    pub has_ocr_layer: bool,
}

impl PageMakeup {
    /// the page is mostly a picture with little real text.
    pub fn is_scanned(&self) -> bool {
        self.image_coverage >= SCANNED_COVERAGE && self.visible_characters < MOST_VISIBLE_CHARACTERS
    }

    /// recognition already ran on it (here or in another program).
    pub fn has_recognised_text(&self) -> bool {
        self.has_ocr_layer || self.invisible_characters > 0
    }
}

/// whether the zero based `page` looks like a scan, so the app can offer
/// to read its text. a page that was already read still counts as a scan.
pub fn is_scanned(doc: &Document, page: u32) -> Result<bool, OcrError> {
    Ok(page_makeup(doc, page)?.is_scanned())
}

/// looks at what the zero based `page` is made of.
pub fn page_makeup(doc: &Document, page: u32) -> Result<PageMakeup, OcrError> {
    let page_id = page_id(doc, page)?;
    let space = PageSpace::of_page(doc, page_id)?;
    let page_dict = doc
        .get_dictionary(page_id)
        .map_err(|_| OcrError::Damaged("a page is missing".into()))?;
    let resources = inherited(doc, page_dict, b"Resources")
        .and_then(|object| resolve_dict(doc, object))
        .unwrap_or_default();
    let bytes = doc.get_page_content(page_id);
    let mut walker = Walker {
        doc,
        makeup: PageMakeup::default(),
        image_area: 0.0,
        visible: space.visible_box,
    };
    walker.walk(&bytes, &resources, Affine::IDENTITY, 0)?;
    let page_area = (space.width * space.height).max(1.0);
    walker.makeup.image_coverage = (walker.image_area / page_area).min(1.0);
    Ok(walker.makeup)
}

struct Walker<'a> {
    doc: &'a Document,
    makeup: PageMakeup,
    image_area: f32,
    /// images are measured only inside this part of the page
    visible: [f32; 4],
}

/// the drawing state that matters here, saved by q and restored by Q.
#[derive(Clone, Copy)]
struct State {
    matrix: Affine,
    invisible_text: bool,
}

impl Walker<'_> {
    fn walk(
        &mut self,
        bytes: &[u8],
        resources: &Dictionary,
        matrix: Affine,
        depth: usize,
    ) -> Result<(), OcrError> {
        let content = Content::decode(bytes).map_err(|error| {
            OcrError::Damaged(format!("the page’s drawing can’t be read: {error}"))
        })?;
        let mut state = State {
            matrix,
            invisible_text: false,
        };
        let mut saved = Vec::new();
        for operation in &content.operations {
            let operands = &operation.operands;
            match operation.operator.as_str() {
                "q" => saved.push(state),
                "Q" => state = saved.pop().unwrap_or(state),
                "cm" => {
                    if let Some(step) = matrix_from(operands) {
                        state.matrix = step.then(&state.matrix);
                    }
                }
                "Tr" => {
                    state.invisible_text = operands.first().and_then(as_number) == Some(3.0);
                }
                "BMC" | "BDC" => {
                    if operands.first().and_then(|tag| tag.as_name().ok()) == Some(LAYER_TAG) {
                        self.makeup.has_ocr_layer = true;
                    }
                }
                "Tj" | "'" | "\"" | "TJ" => self.count_text(operands, state.invisible_text),
                "Do" => {
                    let name = operands.first().and_then(|name| name.as_name().ok());
                    if let Some(name) = name {
                        self.draw_object(name, resources, state.matrix, depth)?;
                    }
                }
                // a small picture written straight into the drawing
                "BI" => self.image_area += self.visible_area(&state.matrix),
                _ => {}
            }
        }
        Ok(())
    }

    fn count_text(&mut self, operands: &[Object], invisible: bool) {
        let characters: usize = operands
            .iter()
            .map(|operand| match operand {
                Object::String(bytes, _) => bytes.len(),
                Object::Array(parts) => parts
                    .iter()
                    .map(|part| part.as_str().map_or(0, <[u8]>::len))
                    .sum(),
                _ => 0,
            })
            .sum();
        if invisible {
            self.makeup.invisible_characters += characters;
        } else {
            self.makeup.visible_characters += characters;
        }
    }

    /// an image adds its area; a drawing (form xobject) is walked in turn.
    fn draw_object(
        &mut self,
        name: &[u8],
        resources: &Dictionary,
        matrix: Affine,
        depth: usize,
    ) -> Result<(), OcrError> {
        let Some(xobjects) = resources
            .get(b"XObject")
            .ok()
            .and_then(|object| resolve_dict(self.doc, object))
        else {
            return Ok(());
        };
        let Some(Object::Stream(stream)) = xobjects
            .get(name)
            .ok()
            .and_then(|object| self.doc.dereference(object).ok())
            .map(|(_, object)| object)
        else {
            return Ok(());
        };
        match stream.dict.get(b"Subtype").and_then(Object::as_name) {
            Ok(b"Image") => self.image_area += self.visible_area(&matrix),
            Ok(b"Form") if depth < MAX_NESTING => {
                let form_matrix = stream
                    .dict
                    .get(b"Matrix")
                    .ok()
                    .and_then(|object| object.as_array().ok())
                    .and_then(|values| matrix_from(values))
                    .unwrap_or(Affine::IDENTITY);
                let form_resources = stream
                    .dict
                    .get(b"Resources")
                    .ok()
                    .and_then(|object| resolve_dict(self.doc, object))
                    .unwrap_or_else(|| resources.clone());
                let bytes = stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone());
                self.walk(
                    &bytes,
                    &form_resources,
                    form_matrix.then(&matrix),
                    depth + 1,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    /// the area of the unit square (where an image is drawn) under `matrix`,
    /// counting only the part inside the visible page.
    fn visible_area(&self, matrix: &Affine) -> f32 {
        let corners = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ]
        .map(|corner| matrix.apply(corner));
        let bounds = Rect::around(&corners);
        let [left, bottom, right, top] = self.visible;
        let width = (bounds.right().min(right) - bounds.x.max(left)).max(0.0);
        let height = (bounds.bottom().min(top) - bounds.y.max(bottom)).max(0.0);
        // a turned image fills less than its bounding box
        let full_area = (matrix.a * matrix.d - matrix.b * matrix.c).abs();
        let box_area = (bounds.width * bounds.height).max(1e-6);
        width * height * (full_area / box_area).min(1.0)
    }
}

fn matrix_from(values: &[Object]) -> Option<Affine> {
    let numbers: Vec<f32> = values.iter().filter_map(as_number).collect();
    let [a, b, c, d, e, f]: [f32; 6] = numbers.try_into().ok()?;
    Some(Affine::new(a, b, c, d, e, f))
}

fn as_number(object: &Object) -> Option<f32> {
    match object {
        Object::Integer(value) => Some(*value as f32),
        Object::Real(value) => Some(*value),
        _ => None,
    }
}

fn resolve_dict(doc: &Document, object: &Object) -> Option<Dictionary> {
    let (_, object) = doc.dereference(object).ok()?;
    object.as_dict().ok().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    /// a letter page drawing `content`, with one image called /Im0.
    fn document(content: &str) -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let image = doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 1, "Height" => 1,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
            },
            vec![128],
        ));
        let contents = doc.add_object(Stream::new(Dictionary::new(), content.as_bytes().to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image } },
            "Contents" => contents,
        });
        doc.objects.insert(
            pages_id,
            dictionary! { "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1 }.into(),
        );
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        doc
    }

    #[test]
    fn a_full_page_image_is_a_scan() {
        let doc = document("q 612 0 0 792 0 0 cm /Im0 Do Q");
        let makeup = page_makeup(&doc, 0).unwrap();
        assert!((makeup.image_coverage - 1.0).abs() < 1e-3);
        assert!(makeup.is_scanned());
        assert!(is_scanned(&doc, 0).unwrap());
    }

    #[test]
    fn a_page_of_text_is_not() {
        let text = "BT /F1 12 Tf 72 700 Td (This page has plenty of real text on it, more than forty letters.) Tj ET";
        let doc = document(&format!("q 612 0 0 792 0 0 cm /Im0 Do Q {text}"));
        assert!(!is_scanned(&doc, 0).unwrap());
    }

    #[test]
    fn a_small_picture_is_not_a_scan() {
        let doc = document("q 100 0 0 100 50 50 cm /Im0 Do Q");
        let makeup = page_makeup(&doc, 0).unwrap();
        assert!(makeup.image_coverage < 0.05);
        assert!(!makeup.is_scanned());
    }

    #[test]
    fn invisible_text_is_counted_apart() {
        let doc = document("q 612 0 0 792 0 0 cm /Im0 Do Q /LettersOcrText BMC BT 3 Tr (hidden words here and there, quite a lot of them) Tj ET EMC");
        let makeup = page_makeup(&doc, 0).unwrap();
        assert_eq!(makeup.visible_characters, 0);
        assert!(makeup.invisible_characters > 40);
        assert!(makeup.has_ocr_layer);
        assert!(makeup.has_recognised_text());
        // a page that was already read is still a scan
        assert!(makeup.is_scanned());
    }

    #[test]
    fn the_part_of_an_image_off_the_page_does_not_count() {
        // twice the page's width, half of it hanging off the right side
        let doc = document("q 1224 0 0 792 0 0 cm /Im0 Do Q");
        let makeup = page_makeup(&doc, 0).unwrap();
        assert!((makeup.image_coverage - 1.0).abs() < 1e-3);
        let doc = document("q 612 0 0 792 306 0 cm /Im0 Do Q");
        assert!((page_makeup(&doc, 0).unwrap().image_coverage - 0.5).abs() < 1e-3);
    }
}
