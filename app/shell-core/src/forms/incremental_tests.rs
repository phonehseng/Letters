//! the rule that keeps signatures whole: every save of a signed document
//! appends its change (core/forms' save_incremental) instead of rewriting
//! the file. these run the way the app runs: sign through the session,
//! fill a field, save, and check the first signature with the engine.

use std::fs;
use std::path::{Path, PathBuf};

use letters_forms::{
    add_field, create_self_signed_certificate, fields, sign, verify, verify_document,
    CertificateRequest, FieldValue, KeyKind, NewField, NewFieldKind, SignRequest,
    SignatureAppearance, SignaturePlacement, VisibleRect,
};
use lopdf::Document;

use super::sign_saved_document;
use super::signed::{file_starts_with, looks_signed};
use crate::session::EditSession;
use crate::test_pdf::{temp_dir, write_pdf};

fn show(path: &Path) -> Result<PathBuf, String> {
    Ok(path.to_path_buf())
}

fn certificate(name: &str) -> Vec<u8> {
    create_self_signed_certificate(&CertificateRequest {
        name: name.into(),
        email: None,
        organization: None,
        valid_days: 30,
        password: "secret".into(),
        key_kind: KeyKind::P256,
    })
    .unwrap()
    .pkcs12
}

fn sign_as(name: &str) -> impl FnOnce(&[u8]) -> Result<Vec<u8>, letters_forms::FormError> {
    let pkcs12 = certificate(name);
    move |bytes| {
        sign(
            bytes,
            &SignRequest {
                pkcs12,
                password: "secret".into(),
                placement: SignaturePlacement::Invisible { name: None },
                reason: None,
                location: None,
                name: None,
                contact_info: None,
                appearance: SignatureAppearance::default(),
            },
        )
    }
}

fn text_field(name: &str, y: f32) -> NewField {
    NewField {
        name: name.into(),
        page: 0,
        rect: VisibleRect {
            x: 72.0,
            y,
            width: 200.0,
            height: 20.0,
        },
        kind: NewFieldKind::Text {
            max_length: None,
            comb: false,
        },
        tooltip: None,
        required: false,
        read_only: false,
        font_size: 0.0,
        alignment: letters_forms::Alignment::Left,
    }
}

fn fill(
    name: &'static str,
    text: &'static str,
) -> impl FnOnce(&mut Document) -> Result<(), String> {
    move |document| {
        let value = FieldValue::Text(text.into());
        letters_forms::set_value(document, name, &value).map_err(|error| error.to_string())
    }
}

/// a saved form with two empty text fields, signed once through its session.
fn signed_form(test_name: &str) -> (PathBuf, EditSession) {
    let dir = temp_dir(test_name);
    let original = dir.join("form.pdf");
    write_pdf(&original, 1);
    let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
    session
        .apply(
            |document: &mut Document| {
                add_field(document, &text_field("name", 100.0))?;
                add_field(document, &text_field("grade", 140.0)).map(drop)
            },
            show,
        )
        .unwrap();
    session.save().unwrap();
    sign_saved_document(&mut session, false, sign_as("Ada Lovelace"), show).unwrap();
    (original, session)
}

fn value_of(bytes: &[u8], name: &str) -> Option<String> {
    let document = Document::load_mem(bytes).unwrap();
    let field = fields(&document)
        .unwrap()
        .into_iter()
        .find(|field| field.name == name)?;
    field.value
}

#[test]
fn filling_a_field_after_signing_saves_and_the_first_signature_still_checks_out() {
    let (original, mut session) = signed_form("incremental-fill");
    let signed = fs::read(&original).unwrap();

    session.apply(fill("name", "Ada"), show).unwrap();
    session.save().unwrap();

    let saved = fs::read(&original).unwrap();
    assert!(saved.starts_with(&signed));
    assert_eq!(value_of(&saved, "name").as_deref(), Some("Ada"));
    let reports = verify(&saved).unwrap();
    assert_eq!(reports.len(), 1);
    assert!(reports[0].valid, "{:?}", reports[0].problems);
    let checked = verify_document(&saved).unwrap();
    assert_eq!(checked.signatures.len(), 1);
    assert!(checked.signatures[0].valid);
}

#[test]
fn several_saves_after_signing_each_keep_the_signature() {
    let (original, mut session) = signed_form("incremental-twice");

    session.apply(fill("name", "Ada"), show).unwrap();
    session.save().unwrap();
    session.apply(fill("grade", "A"), show).unwrap();
    session.save().unwrap();

    let saved = fs::read(&original).unwrap();
    assert_eq!(value_of(&saved, "grade").as_deref(), Some("A"));
    assert!(verify(&saved).unwrap()[0].valid);
}

#[test]
fn a_second_signer_after_a_fill_keeps_both_signatures_valid() {
    let (original, mut session) = signed_form("incremental-countersign");

    session.apply(fill("name", "Ada"), show).unwrap();
    session.save().unwrap();
    sign_saved_document(&mut session, false, sign_as("Grace Hopper"), show).unwrap();

    let reports = verify(&fs::read(&original).unwrap()).unwrap();
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().all(|report| report.valid));
}

#[test]
fn save_as_of_a_changed_signed_document_keeps_its_signature_and_guard() {
    let (original, mut session) = signed_form("incremental-save-as");
    session.apply(fill("name", "Ada"), show).unwrap();
    let copy = original.with_file_name("form filled.pdf");

    session.save_as(&copy).unwrap();

    assert!(verify(&fs::read(&copy).unwrap()).unwrap()[0].valid);
    assert!(session.original_is_protected(), "the copy is signed too");
}

#[test]
fn a_signed_file_opened_later_is_guarded_from_its_first_edit() {
    let (original, session) = signed_form("incremental-reopen");
    let sessions = original.parent().unwrap().join("sessions again");
    drop(session);

    let mut reopened = EditSession::start(&original, &sessions).unwrap();
    assert!(reopened.original_is_protected());
    reopened.apply(fill("name", "Ada"), show).unwrap();
    reopened.save().unwrap();

    assert!(verify(&fs::read(&original).unwrap()).unwrap()[0].valid);
}

#[test]
fn an_unsigned_file_is_not_guarded_and_saves_whole() {
    let dir = temp_dir("incremental-unsigned");
    let original = dir.join("plain.pdf");
    write_pdf(&original, 1);
    assert!(!looks_signed(&original));
    let session = EditSession::start(&original, &dir.join("sessions")).unwrap();
    assert!(!session.original_is_protected());
}

#[test]
fn starting_with_compares_whole_files() {
    let dir = temp_dir("incremental-prefix");
    let short = dir.join("short.bin");
    let long = dir.join("long.bin");
    fs::write(&short, b"%PDF signed bytes").unwrap();
    fs::write(&long, b"%PDF signed bytes and an update").unwrap();

    assert!(file_starts_with(&long, &short));
    assert!(!file_starts_with(&short, &long));
    fs::write(&long, b"%PDF SIGNED bytes and an update").unwrap();
    assert!(!file_starts_with(&long, &short));
    assert!(!file_starts_with(&dir.join("missing.bin"), &short));
}
