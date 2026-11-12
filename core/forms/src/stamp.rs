//! putting a picture of a signature on a page.
//!
//! this is a visual signature only: it shows a signature but proves
//! nothing about who placed it. typed and drawn signatures are turned
//! into a png by the ui; this module places that png, keeping its
//! see-through background so the page shows through around the ink.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};

use crate::appearance::upright_matrix;
use crate::error::{FormError, FormResult};
use crate::geometry::{number, PageFrame, VisibleRect};
use crate::image::add_png_image;
use crate::objects::{
    add_page_xobject, append_page_content, check_document, name, page_frame, page_id,
    push_to_array, rect_object, text_string,
};

/// where and how to place a signature picture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureImage {
    /// 0-based page index
    pub page: u32,
    /// in points from the page's visible top left. the picture is stretched
    /// to fill it, so keep its proportions in the ui
    pub rect: VisibleRect,
    /// the picture as a png file; transparency is kept
    pub png: Vec<u8>,
    /// true: a stamp annotation that can still be moved or deleted in pdf
    /// apps. false: part of the page itself
    #[serde(default)]
    pub as_annotation: bool,
}

/// places a signature picture on a page, upright as the reader sees it.
pub fn place_signature_image(doc: &mut Document, request: &SignatureImage) -> FormResult<()> {
    check_document(doc)?;
    if request.rect.is_empty() {
        return Err(FormError::EmptyRect);
    }
    let page = page_id(doc, request.page)?;
    let frame = page_frame(doc, page);
    let image = add_png_image(doc, &request.png)?;
    if request.as_annotation {
        add_stamp(doc, page, &frame, &request.rect, image)
    } else {
        let image_name = add_page_xobject(doc, page, "LettersSig", image)?;
        let matrix = frame.upright_unit_square(&request.rect);
        let content = format!("q {} cm /{image_name} Do Q\n", matrix.to_operands());
        append_page_content(doc, page, content.as_bytes())
    }
}

/// a form xobject that draws an image upright into a box of the given
/// visible size, turned to match the page. also used for the visible part
/// of digital signatures.
pub(crate) fn upright_image_form(
    doc: &mut Document,
    frame: &PageFrame,
    width: f32,
    height: f32,
    image: ObjectId,
) -> ObjectId {
    let content = format!(
        "q {} 0 0 {} 0 0 cm /Im Do Q\n",
        number(width),
        number(height)
    );
    let mut xobjects = Dictionary::new();
    xobjects.set("Im", Object::Reference(image));
    let mut resources = Dictionary::new();
    resources.set("XObject", Object::Dictionary(xobjects));
    doc.add_object(form_xobject(frame, width, height, resources, content))
}

/// a form xobject of a visible size, with a /Matrix that turns it so it
/// is upright on its page.
pub(crate) fn form_xobject(
    frame: &PageFrame,
    width: f32,
    height: f32,
    resources: Dictionary,
    content: String,
) -> Stream {
    let mut dict = Dictionary::new();
    dict.set("Type", name("XObject"));
    dict.set("Subtype", name("Form"));
    dict.set(
        "BBox",
        Object::Array(vec![
            0.into(),
            0.into(),
            crate::objects::real(width),
            crate::objects::real(height),
        ]),
    );
    let matrix = upright_matrix(frame.rotation, width, height);
    dict.set(
        "Matrix",
        Object::Array(
            [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f]
                .iter()
                .map(|value| crate::objects::real(*value))
                .collect(),
        ),
    );
    dict.set("Resources", Object::Dictionary(resources));
    Stream::new(dict, content.into_bytes())
}

fn add_stamp(
    doc: &mut Document,
    page: ObjectId,
    frame: &PageFrame,
    rect: &VisibleRect,
    image: ObjectId,
) -> FormResult<()> {
    let appearance = upright_image_form(doc, frame, rect.width, rect.height, image);
    let mut appearances = Dictionary::new();
    appearances.set("N", Object::Reference(appearance));
    let mut stamp = Dictionary::new();
    stamp.set("Type", name("Annot"));
    stamp.set("Subtype", name("Stamp"));
    stamp.set("Rect", rect_object(&frame.to_user_rect(rect)));
    stamp.set("P", Object::Reference(page));
    // 4 = print
    stamp.set("F", Object::Integer(4));
    stamp.set("Name", name("LettersSignature"));
    stamp.set("Contents", text_string("Signature"));
    stamp.set("AP", Object::Dictionary(appearances));
    let id = doc.add_object(Object::Dictionary(stamp));
    push_to_array(doc, page, b"Annots", Object::Reference(id))
}
