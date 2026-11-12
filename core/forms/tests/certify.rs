//! certification levels, field locks, signing in turn, what changed after
//! each signature, and the document security store.

mod support;

use letters_forms::{
    add_validation_data, create_self_signed_certificate, fields, save_incremental,
    set_signature_lock, set_signing_order, set_value, sign, sign_with_options, signing_workflow,
    verify_document, CertificateRequest, CertificationLevel, ChangeKind, ChangeVerdict, FieldLock,
    FieldValue, FormError, KeyKind, SignRequest, SignatureAppearance, SignatureKind,
    SignaturePlacement, SigningOptions, ValidationData, VisibleRect,
};
use lopdf::{dictionary, Document, Object};
use support::{load, sample_form, PageSetup};

const PASSWORD: &str = "correct horse";

fn request(name: &str, placement: SignaturePlacement) -> SignRequest {
    let certificate = create_self_signed_certificate(&CertificateRequest {
        name: name.into(),
        email: None,
        organization: None,
        valid_days: 30,
        password: PASSWORD.into(),
        key_kind: KeyKind::P256,
    })
    .unwrap();
    SignRequest {
        pkcs12: certificate.pkcs12,
        password: PASSWORD.into(),
        placement,
        reason: None,
        location: None,
        name: None,
        contact_info: None,
        appearance: SignatureAppearance::default(),
    }
}

fn invisible() -> SignaturePlacement {
    SignaturePlacement::Invisible { name: None }
}

fn certify(level: CertificationLevel) -> Vec<u8> {
    let options = SigningOptions {
        certify: Some(level),
        ..Default::default()
    };
    let form = sample_form(&PageSetup::letter(0));
    sign_with_options(&form, &request("Author", invisible()), &options).unwrap()
}

/// fills a field and saves the change as an incremental update.
fn fill(bytes: &[u8], field: &str, value: &str) -> Vec<u8> {
    let mut doc = load(bytes);
    set_value(&mut doc, field, &FieldValue::Text(value.into())).unwrap();
    save_incremental(bytes, &doc).unwrap()
}

/// adds a sticky note comment to the first page, incrementally.
fn comment(bytes: &[u8]) -> Vec<u8> {
    let mut doc = load(bytes);
    let page = doc.get_pages()[&1];
    let note = doc.add_object(dictionary! {
        "Type" => "Annot", "Subtype" => "Text",
        "Rect" => vec![10.into(), 10.into(), 30.into(), 30.into()],
        "Contents" => Object::string_literal("a comment"),
    });
    let mut annots = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Annots")
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    annots.push(Object::Reference(note));
    doc.get_dictionary_mut(page).unwrap().set("Annots", annots);
    save_incremental(bytes, &doc).unwrap()
}

/// draws something new on the first page, incrementally.
fn draw_on_page(bytes: &[u8]) -> Vec<u8> {
    let mut doc = load(bytes);
    let page = doc.get_pages()[&1];
    let stream = doc.add_object(lopdf::Stream::new(
        dictionary! {},
        b"0 0 1 rg 50 50 100 100 re f".to_vec(),
    ));
    let old = doc
        .get_dictionary(page)
        .unwrap()
        .get(b"Contents")
        .unwrap()
        .clone();
    doc.get_dictionary_mut(page).unwrap().set(
        "Contents",
        Object::Array(vec![old, Object::Reference(stream)]),
    );
    save_incremental(bytes, &doc).unwrap()
}

fn verdict_of(bytes: &[u8], signature: usize) -> (ChangeVerdict, String) {
    let checked = verify_document(bytes).unwrap();
    let changes = &checked.changes[signature];
    (changes.verdict, changes.sentence.clone())
}

#[test]
fn certification_is_reported_with_its_level() {
    let certified = certify(CertificationLevel::FormFillingAndSigning);
    let checked = verify_document(&certified).unwrap();
    assert!(checked.signatures[0].valid);
    assert_eq!(checked.details[0].kind, SignatureKind::Certification);
    assert_eq!(
        checked.certification,
        Some(CertificationLevel::FormFillingAndSigning)
    );
    assert!(checked
        .certification_note
        .unwrap()
        .contains("allowed filling in the form and signing"));
    assert_eq!(checked.changes[0].verdict, ChangeVerdict::Unchanged);
    let file = String::from_utf8_lossy(&certified);
    assert!(file.contains("DocMDP") && file.contains("/Perms"));
}

#[test]
fn level_two_allows_filling_and_signing_but_not_comments_or_drawing() {
    let certified = certify(CertificationLevel::FormFillingAndSigning);
    let filled = fill(&certified, "name", "Ada");
    let (verdict, sentence) = verdict_of(&filled, 0);
    assert_eq!(verdict, ChangeVerdict::AsIntended, "{sentence}");
    assert!(
        sentence.starts_with("Filled in and signed as intended"),
        "{sentence}"
    );

    let countersigned = sign(&filled, &request("Grace", invisible())).unwrap();
    let checked = verify_document(&countersigned).unwrap();
    assert!(checked.signatures.iter().all(|report| report.valid));
    assert_eq!(checked.changes[0].verdict, ChangeVerdict::AsIntended);
    assert_eq!(checked.changes[1].verdict, ChangeVerdict::Unchanged);

    let commented = comment(&filled);
    let (verdict, sentence) = verdict_of(&commented, 0);
    assert_eq!(verdict, ChangeVerdict::NotAllowed, "{sentence}");
    assert!(
        sentence.contains("Changed in ways the author did not allow: comments"),
        "{sentence}"
    );

    let drawn = draw_on_page(&filled);
    let checked = verify_document(&drawn).unwrap();
    assert_eq!(checked.changes[0].verdict, ChangeVerdict::NotAllowed);
    assert_eq!(
        checked.changes[0].not_allowed[0].kind,
        ChangeKind::PageContent
    );
    assert!(
        checked.signatures[0].valid,
        "the signed part itself is unchanged"
    );
}

#[test]
fn level_three_also_allows_comments() {
    let certified = certify(CertificationLevel::FormFillingSigningAndComments);
    let commented = comment(&fill(&certified, "name", "Ada"));
    assert_eq!(verdict_of(&commented, 0).0, ChangeVerdict::AsIntended);
    assert_eq!(
        verdict_of(&draw_on_page(&commented), 0).0,
        ChangeVerdict::NotAllowed
    );
}

#[test]
fn level_one_allows_no_changes_but_validation_data() {
    let certified = certify(CertificationLevel::NoChanges);
    let error = sign(&certified, &request("Grace", invisible())).unwrap_err();
    assert_eq!(error, FormError::CertifiedNoChanges);
    let (verdict, _) = verdict_of(&fill(&certified, "name", "Ada"), 0);
    assert_eq!(verdict, ChangeVerdict::NotAllowed);
    let extra = ValidationData {
        crls: vec![b"a revocation list the caller had".to_vec()],
        ..Default::default()
    };
    let validated = add_validation_data(&certified, &extra).unwrap();
    assert_eq!(verdict_of(&validated, 0).0, ChangeVerdict::AsIntended);
}

#[test]
fn only_the_first_signature_can_certify() {
    let signed = sign(
        &sample_form(&PageSetup::letter(0)),
        &request("Ada", invisible()),
    )
    .unwrap();
    let options = SigningOptions {
        certify: Some(CertificationLevel::NoChanges),
        ..Default::default()
    };
    let error = sign_with_options(&signed, &request("Grace", invisible()), &options).unwrap_err();
    assert_eq!(error, FormError::CertifyFirst);
}

#[test]
fn without_certification_changes_have_no_limits_but_are_reported() {
    let signed = sign(
        &sample_form(&PageSetup::letter(0)),
        &request("Ada", invisible()),
    )
    .unwrap();
    let filled = fill(&signed, "name", "Ada");
    let checked = verify_document(&filled).unwrap();
    let changes = &checked.changes[0];
    assert_eq!(changes.verdict, ChangeVerdict::NoLimits);
    assert!(changes
        .changes
        .iter()
        .any(|c| c.kind == ChangeKind::FieldValue && c.field.as_deref() == Some("name")));
    assert!(
        changes.sentence.contains("the answer in “name” changed"),
        "{}",
        changes.sentence
    );
    assert_eq!(checked.revisions.len(), 3);
}

fn two_signer_form() -> Vec<u8> {
    let mut doc = load(&sample_form(&PageSetup::letter(0)));
    for (name, x) in [("student", 72.0), ("teacher", 320.0)] {
        letters_forms::add_field(
            &mut doc,
            &letters_forms::NewField {
                name: name.into(),
                page: 0,
                rect: VisibleRect::new(x, 680.0, 200.0, 50.0),
                kind: letters_forms::NewFieldKind::Signature,
                tooltip: None,
                required: false,
                read_only: false,
                font_size: 0.0,
                alignment: letters_forms::Alignment::Left,
            },
        )
        .unwrap();
    }
    let lock = |fields: &[&str]| FieldLock::Include {
        fields: fields.iter().map(|f| (*f).to_owned()).collect(),
    };
    set_signature_lock(&mut doc, "student", Some(&lock(&["name", "notes"]))).unwrap();
    set_signature_lock(&mut doc, "teacher", Some(&lock(&["must"]))).unwrap();
    set_signing_order(&mut doc, &["student".into(), "teacher".into()]).unwrap();
    support::save(&mut doc)
}

fn existing(field: &str) -> SignaturePlacement {
    SignaturePlacement::ExistingField {
        field: field.into(),
    }
}

#[test]
fn two_signers_sign_in_turn_and_each_locks_their_fields() {
    let mut bytes = two_signer_form();
    bytes = fill(&bytes, "name", "Ada");
    let in_order = SigningOptions {
        follow_signing_order: true,
        ..Default::default()
    };
    let error =
        sign_with_options(&bytes, &request("Teacher", existing("teacher")), &in_order).unwrap_err();
    assert_eq!(
        error.to_string(),
        "“teacher” is to be signed after “student”."
    );

    bytes = sign_with_options(&bytes, &request("Ada", existing("student")), &in_order).unwrap();
    let doc = load(&bytes);
    let steps = signing_workflow(&doc).unwrap();
    assert!(steps[0].signed && steps[1].is_next, "{steps:?}");
    let name = fields(&doc)
        .unwrap()
        .into_iter()
        .find(|f| f.name == "name")
        .unwrap();
    assert!(
        name.read_only,
        "the student's fields are locked once they sign"
    );
    let mut doc = load(&bytes);
    assert_eq!(
        set_value(&mut doc, "name", &FieldValue::Text("Eve".into())).unwrap_err(),
        FormError::ReadOnly("name".into())
    );

    bytes = fill(&bytes, "must", "graded");
    bytes = sign_with_options(&bytes, &request("Teacher", existing("teacher")), &in_order).unwrap();
    let checked = verify_document(&bytes).unwrap();
    assert!(
        checked.signatures.iter().all(|report| report.valid),
        "{:#?}",
        checked.signatures
    );
    assert_eq!(
        checked.changes[0].verdict,
        ChangeVerdict::AsIntended,
        "{}",
        checked.changes[0].sentence
    );

    // someone unlocks the student's answer and changes it afterwards
    let mut doc = load(&bytes);
    let id = doc
        .objects
        .iter()
        .find(|(_, object)| {
            object
                .as_dict()
                .ok()
                .and_then(|d| d.get(b"T").ok())
                .and_then(|t| t.as_str().ok())
                == Some(b"name")
        })
        .map(|(id, _)| *id)
        .unwrap();
    doc.get_dictionary_mut(id)
        .unwrap()
        .set("Ff", Object::Integer(0));
    set_value(&mut doc, "name", &FieldValue::Text("Eve".into())).unwrap();
    let tampered = save_incremental(&bytes, &doc).unwrap();
    let checked = verify_document(&tampered).unwrap();
    for changes in &checked.changes {
        assert_eq!(
            changes.verdict,
            ChangeVerdict::NotAllowed,
            "{}",
            changes.sentence
        );
    }
    assert!(checked.changes[0]
        .not_allowed
        .iter()
        .any(|c| c.field.as_deref() == Some("name")));
}

#[test]
fn the_security_store_holds_chains_and_given_revocation_data() {
    let signed = sign(
        &sample_form(&PageSetup::letter(0)),
        &request("Ada", invisible()),
    )
    .unwrap();
    let data = ValidationData {
        certificates: vec![],
        crls: vec![b"crl bytes".to_vec()],
        ocsp_responses: vec![b"ocsp bytes".to_vec()],
    };
    let stored = add_validation_data(&signed, &data).unwrap();
    assert_eq!(&stored[..signed.len()], signed.as_slice());
    let doc: Document = load(&stored);
    let catalog = doc.catalog().unwrap();
    let dss = doc
        .get_dictionary(catalog.get(b"DSS").unwrap().as_reference().unwrap())
        .unwrap();
    for key in [b"Certs".as_slice(), b"CRLs", b"OCSPs"] {
        assert_eq!(
            dss.get(key).unwrap().as_array().unwrap().len(),
            1,
            "{}",
            String::from_utf8_lossy(key)
        );
    }
    let vri = dss.get(b"VRI").unwrap().as_dict().unwrap();
    let key = vri.iter().next().unwrap().0;
    assert_eq!(key.len(), 40, "a sha-1 in hex");
    assert!(key
        .iter()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()));
    let checked = verify_document(&stored).unwrap();
    assert!(checked.signatures[0].valid);
    assert_eq!(checked.changes[0].verdict, ChangeVerdict::AsIntended);
    // adding again reuses what is stored
    let again = add_validation_data(&stored, &data).unwrap();
    let doc = load(&again);
    let dss = doc
        .get_dictionary(
            doc.catalog()
                .unwrap()
                .get(b"DSS")
                .unwrap()
                .as_reference()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(dss.get(b"CRLs").unwrap().as_array().unwrap().len(), 1);
}

/// redraws a field's appearance with other words, leaving its answer as
/// it was: the field then shows text that isn't its answer.
fn redraw_without_answer(bytes: &[u8], field: &str, words: &str) -> Vec<u8> {
    let mut doc = load(bytes);
    let id = doc
        .objects
        .iter()
        .find(|(_, object)| {
            object
                .as_dict()
                .ok()
                .and_then(|dict| dict.get(b"T").ok())
                .and_then(|title| title.as_str().ok())
                == Some(field.as_bytes())
        })
        .map(|(id, _)| *id)
        .expect("the field is in the form");
    let drawing = format!("BT /Helv 10 Tf 2 4 Td ({words}) Tj ET");
    let stream = doc.add_object(lopdf::Stream::new(
        dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 200.into(), 20.into()] },
        drawing.into_bytes(),
    ));
    doc.get_dictionary_mut(id)
        .unwrap()
        .set("AP", dictionary! { "N" => Object::Reference(stream) });
    save_incremental(bytes, &doc).unwrap()
}

#[test]
fn a_field_redrawn_without_its_answer_changing_is_not_filling_in() {
    // the "shadow attack": after certifying, only a field's drawing is
    // replaced, so it shows words its answer doesn't hold
    let certified = certify(CertificationLevel::FormFillingAndSigning);
    let shadowed = redraw_without_answer(&certified, "name", "I AGREE TO PAY 1000");
    let checked = verify_document(&shadowed).unwrap();
    let changes = &checked.changes[0];
    assert_eq!(
        changes.verdict,
        ChangeVerdict::NotAllowed,
        "{}",
        changes.sentence
    );
    assert_eq!(changes.not_allowed[0].kind, ChangeKind::FieldDrawing);
    assert!(
        changes
            .sentence
            .contains("redrawn without its answer changing"),
        "{}",
        changes.sentence
    );

    // filling the same field in, answer and drawing together, is still fine
    let filled = fill(&certified, "name", "Ada");
    assert_eq!(verdict_of(&filled, 0).0, ChangeVerdict::AsIntended);
}
