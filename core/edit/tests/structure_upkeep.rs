//! structure stays intact around the newer edits: marked content stays
//! balanced and keeps its MCIDs, /ActualText follows text the person
//! changed, signed files need consent and locked files are refused.

mod support;

use letters_edit::{
    content_edit_check, marked_content_balanced, marked_content_ids, move_paths, page_objects,
    reflow_paragraph, EditConsent, EditError, ReflowOptions,
};
use lopdf::{Dictionary, Document, Object};
use support::{load, page_content, PageSetup, PdfBuilder};

/// two tagged paragraphs; the first also says what it reads as.
const TAGGED: &str = "\
/P <</MCID 0 /ActualText (Old words for the first paragraph)>> BDC
BT /F1 10 Tf 14 TL 72 700 Td (Old words for the first) Tj T* (paragraph.) Tj ET
EMC
/P <</MCID 1>> BDC
BT /F1 10 Tf 14 TL 72 650 Td (The second paragraph sits below it.) Tj ET
EMC
0 0 1 rg 72 600 50 20 re f";

fn tagged_pdf() -> Vec<u8> {
    let mut pdf = PdfBuilder::new();
    pdf.page(&PageSetup::letter(), &[TAGGED], "/Font << /F1 3 0 R >>");
    pdf.finish()
}

#[test]
fn reflow_keeps_marked_content_balanced_and_in_order() {
    let mut doc = load(&tagged_pdf());
    let objects = page_objects(&doc, 0).unwrap();
    let first = objects.text_blocks[0].id.clone();
    let text = "New words for the first paragraph, now long enough to need a third line in this narrow column of text.";
    let options = ReflowOptions {
        bottom_limit: Some(780.0),
        ..ReflowOptions::default()
    };
    let result = reflow_paragraph(&mut doc, 0, &first, text, &options).unwrap();
    assert!(result.applied);
    let content = page_content(&doc, 0);
    assert!(marked_content_balanced(content.as_bytes()), "{content}");
    assert_eq!(
        marked_content_ids(content.as_bytes()),
        [0, 1],
        "MCIDs keep their order"
    );
    // strings are written in hex, so any byte is safe
    let hex: String = text.bytes().map(|byte| format!("{byte:02X}")).collect();
    assert!(
        content.contains(&format!("/ActualText <{hex}>")),
        "the reading text follows: {content}"
    );
    // the second paragraph moved inside its own sequence
    let second_start = content.find("/MCID 1").unwrap();
    let second_text = content
        .find("<546865207365636F6E64")
        .or_else(|| content.find("(The second"))
        .unwrap();
    assert!(
        second_text > second_start,
        "the moved text is still inside MCID 1"
    );
}

#[test]
fn moving_a_path_does_not_touch_marked_content() {
    let mut doc = load(&tagged_pdf());
    let path = page_objects(&doc, 0).unwrap().paths[0].id.clone();
    move_paths(&mut doc, 0, &[path], 10.0, 10.0, &EditConsent::default()).unwrap();
    let content = page_content(&doc, 0);
    assert!(marked_content_balanced(content.as_bytes()));
    assert!(
        content.contains("/ActualText (Old words for the first paragraph)"),
        "untouched text keeps its reading"
    );
}

/// the tagged page with a signed signature field.
fn signed(doc: &mut Document) {
    let mut signature = Dictionary::new();
    signature.set("Type", Object::Name(b"Sig".to_vec()));
    signature.set("Name", Object::string_literal("Ada Lovelace"));
    signature.set("M", Object::string_literal("D:20261112090000Z"));
    let value = doc.add_object(signature);
    let mut field = Dictionary::new();
    field.set("FT", Object::Name(b"Sig".to_vec()));
    field.set("T", Object::string_literal("Approval"));
    field.set("V", Object::Reference(value));
    let field = doc.add_object(field);
    let mut form = Dictionary::new();
    form.set("Fields", Object::Array(vec![Object::Reference(field)]));
    form.set("SigFlags", Object::Integer(3));
    let form = doc.add_object(form);
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    doc.get_dictionary_mut(root)
        .unwrap()
        .set("AcroForm", Object::Reference(form));
}

#[test]
fn signed_documents_need_consent_before_any_content_edit() {
    let mut doc = load(&tagged_pdf());
    signed(&mut doc);
    let check = content_edit_check(&doc);
    assert_eq!(check.signatures.len(), 1);
    assert_eq!(check.signatures[0].signer.as_deref(), Some("Ada Lovelace"));
    assert!(check.message.unwrap().contains("signed by Ada Lovelace"));
    let first = page_objects(&doc, 0).unwrap().text_blocks[0].id.clone();
    let refused =
        reflow_paragraph(&mut doc, 0, &first, "Changed.", &ReflowOptions::default()).unwrap_err();
    assert!(matches!(refused, EditError::SignaturesWouldBreak(_)));
    assert!(refused.to_string().contains("make the signature invalid"));
    let agreed = ReflowOptions {
        consent: EditConsent {
            invalidate_signatures: true,
        },
        ..ReflowOptions::default()
    };
    assert!(
        reflow_paragraph(&mut doc, 0, &first, "Changed.", &agreed)
            .unwrap()
            .applied
    );
}

#[test]
fn locked_documents_must_be_unlocked_first() {
    let mut doc = load(&tagged_pdf());
    let encryption = doc.add_object(Dictionary::new());
    doc.trailer.set("Encrypt", Object::Reference(encryption));
    assert!(content_edit_check(&doc).locked);
    let error = move_paths(
        &mut doc,
        0,
        &["path:x".into()],
        1.0,
        1.0,
        &EditConsent::default(),
    )
    .unwrap_err();
    assert_eq!(error, EditError::Locked);
}
