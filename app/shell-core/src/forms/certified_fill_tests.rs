//! filling a certified form after it was signed, the way the app does: the
//! author allows answers, so the signature must stay valid and the verdict
//! must be "as intended", even with formatted and locked fields around.

use std::fs;
use std::path::{Path, PathBuf};

use letters_forms::{
    add_field, create_self_signed_certificate, fill_field, set_field_logic, sign_with_options,
    verify_document, CertificateRequest, CertificationLevel, ChangeVerdict, FieldFormat, FieldLock,
    FieldLogicChanges, FieldValue, KeyKind, NegativeStyle, NewField, NewFieldKind, SeparatorStyle,
    SignRequest, SignatureAppearance, SignaturePlacement, SigningOptions, VisibleRect,
};
use lopdf::Document;

use super::sign_saved_document;
use super::unchanged_drawings::fill_field_keeping_drawings;
use crate::session::EditSession;
use crate::test_pdf::{temp_dir, write_pdf};

fn show(path: &Path) -> Result<PathBuf, String> {
    Ok(path.to_path_buf())
}

fn text_field(name: &str, y: f32) -> NewField {
    NewField {
        name: name.into(),
        page: 0,
        rect: VisibleRect {
            x: 72.0,
            y,
            width: 160.0,
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

fn money() -> FieldLogicChanges {
    FieldLogicChanges {
        calculation: None,
        format: Some(FieldFormat::Number {
            decimals: 2,
            separator: SeparatorStyle::CommaDot,
            negative: NegativeStyle::MinusSign,
            currency: "$".into(),
            currency_before: true,
        }),
        validations: Vec::new(),
    }
}

/// a saved form with a name and a money field (filled), certified to allow
/// filling and signing, with the money field locked.
fn certified_form(test_name: &str) -> (PathBuf, EditSession) {
    let dir = temp_dir(test_name);
    let original = dir.join("claim.pdf");
    write_pdf(&original, 1);
    let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();
    session
        .apply(
            |document: &mut Document| {
                add_field(document, &text_field("name", 100.0))?;
                add_field(document, &text_field("travel", 140.0))?;
                set_field_logic(document, "travel", &money())?;
                fill_field(document, "travel", &FieldValue::Text("120.5".into())).map(drop)
            },
            show,
        )
        .unwrap();
    session.save().unwrap();
    let pkcs12 = create_self_signed_certificate(&CertificateRequest {
        name: "Ada Lovelace".into(),
        email: None,
        organization: None,
        valid_days: 30,
        password: "secret".into(),
        key_kind: KeyKind::P256,
    })
    .unwrap()
    .pkcs12;
    let request = SignRequest {
        pkcs12,
        password: "secret".into(),
        placement: SignaturePlacement::Invisible { name: None },
        reason: None,
        location: None,
        name: None,
        contact_info: None,
        appearance: SignatureAppearance::default(),
    };
    let options = SigningOptions {
        certify: Some(CertificationLevel::FormFillingAndSigning),
        lock: Some(FieldLock::Include {
            fields: vec!["travel".into()],
        }),
        ..SigningOptions::default()
    };
    let sign = |bytes: &[u8]| sign_with_options(bytes, &request, &options);
    sign_saved_document(&mut session, false, sign, show).unwrap();
    (original, session)
}

#[test]
fn filling_an_allowed_field_after_certifying_is_as_intended() {
    let (original, mut session) = certified_form("certified-fill");

    session
        .apply(
            |document: &mut Document| {
                let value = FieldValue::Text("Ada".into());
                fill_field_keeping_drawings(document, "name", &value).map(drop)
            },
            show,
        )
        .unwrap();
    session.save().unwrap();

    let checked = verify_document(&fs::read(&original).unwrap()).unwrap();
    assert!(checked.signatures[0].valid);
    assert_eq!(
        checked.changes[0].verdict,
        ChangeVerdict::AsIntended,
        "{}",
        checked.changes[0].sentence
    );
}

#[test]
fn a_drawing_made_again_exactly_is_put_back_and_a_real_change_kept() {
    let dir = temp_dir("unchanged-drawings");
    let path = dir.join("form.pdf");
    write_pdf(&path, 1);
    let mut document = Document::load(&path).unwrap();
    add_field(&mut document, &text_field("name", 100.0)).unwrap();
    add_field(&mut document, &text_field("travel", 140.0)).unwrap();
    set_field_logic(&mut document, "travel", &money()).unwrap();
    fill_field(&mut document, "travel", &FieldValue::Text("120.5".into())).unwrap();
    let before = document.clone();

    fill_field_keeping_drawings(&mut document, "name", &FieldValue::Text("Ada".into())).unwrap();

    let changed: Vec<_> = document
        .objects
        .iter()
        .filter(|(id, object)| before.objects.get(id) != Some(object))
        .map(|(id, _)| *id)
        .collect();
    let travel_widget_changed = changed.iter().any(|id| {
        document.get_dictionary(*id).is_ok_and(|dict| {
            dict.get(b"T")
                .is_ok_and(|t| t.as_str().is_ok_and(|t| t == b"travel"))
        })
    });
    assert!(
        !travel_widget_changed,
        "the money field was drawn again exactly and put back"
    );
    assert!(!changed.is_empty(), "the filled name field did change");
}
