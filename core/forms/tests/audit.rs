//! the audit trail inside the pdf: entries chained by hash, sealed by the
//! next signature or time stamp, and every kind of tampering showing.

mod support;

use letters_forms::{
    add_audit_page, add_document_timestamp, append_audit_entry, audit_trail,
    create_self_signed_certificate, save_incremental, set_value, sign_with_options,
    verify_document, AuditEvent, AuditVerdict, CertificateRequest, CertificationLevel,
    ChangeVerdict, FieldValue, KeyKind, SignRequest, SignatureAppearance, SignaturePlacement,
    SigningOptions, TimestampAuthority, AUDIT_TRAIL_NOTE,
};
use support::tsa::{Behaviour, FakeTsa};
use support::{load, sample_form, temp_dir, PageSetup};

const PASSWORD: &str = "correct horse";

fn request() -> SignRequest {
    let certificate = create_self_signed_certificate(&CertificateRequest {
        name: "Ada Lovelace".into(),
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
        placement: SignaturePlacement::ExistingField {
            field: "sign_here".into(),
        },
        reason: Some("I agree".into()),
        location: None,
        name: None,
        contact_info: None,
        appearance: SignatureAppearance::default(),
    }
}

/// prepared, signed (recorded), saved, then sealed with a document time stamp.
fn full_history() -> Vec<u8> {
    let dir = temp_dir("audit-history");
    let tsa = FakeTsa::start(Behaviour::Honest, &dir);
    let prepared = AuditEvent::Prepared {
        note: Some("Permission slip for the museum trip".into()),
    };
    let mut bytes =
        append_audit_entry(&sample_form(&PageSetup::letter(0)), "Ms Teacher", &prepared).unwrap();
    let options = SigningOptions {
        audit_actor: Some("Ada".into()),
        ..Default::default()
    };
    bytes = sign_with_options(&bytes, &request(), &options).unwrap();
    let mut doc = load(&bytes);
    set_value(
        &mut doc,
        "notes",
        &FieldValue::Text("allergic to peanuts".into()),
    )
    .unwrap();
    bytes = save_incremental(&bytes, &doc).unwrap();
    bytes = append_audit_entry(&bytes, "Ada", &AuditEvent::Saved).unwrap();
    let authority = TimestampAuthority {
        url: tsa.url.clone(),
        username: None,
        password: None,
    };
    add_document_timestamp(&bytes, &authority).unwrap()
}

#[test]
fn the_note_says_what_the_trail_cannot_know() {
    assert!(AUDIT_TRAIL_NOTE.contains("on this computer"));
    assert!(AUDIT_TRAIL_NOTE.contains("can’t know who opened or viewed the file anywhere else"));
    assert!(AUDIT_TRAIL_NOTE.contains("doesn’t use a tracking service"));
}

#[test]
fn a_full_history_is_intact_and_sealed() {
    let bytes = full_history();
    let report = audit_trail(&bytes).unwrap();
    assert_eq!(
        report.verdict,
        AuditVerdict::Intact,
        "{:#?}",
        report.problems
    );
    assert_eq!(report.entries.len(), 3);
    assert!(
        report.entries.iter().all(|entry| entry.sealed),
        "{report:#?}"
    );
    let AuditEvent::Signed {
        signer,
        certificate_fingerprint,
        time_source,
        reason,
        field,
        ..
    } = &report.entries[1].entry.event
    else {
        panic!("the second entry is the signature");
    };
    assert_eq!(signer, "Ada Lovelace");
    assert_eq!(field, "sign_here");
    assert_eq!(certificate_fingerprint.len(), 64);
    assert_eq!(time_source, "the signer’s computer clock");
    assert_eq!(reason.as_deref(), Some("I agree"));
    assert!(
        report
            .summary
            .starts_with("The audit trail is intact: 3 entries"),
        "{}",
        report.summary
    );
    // the signature and the time stamp still check out with the trail in the file
    let checked = verify_document(&bytes).unwrap();
    assert!(checked.signatures.iter().all(|signature| signature.valid));
}

/// changes one byte of the newest copy of an entry's text in the file.
fn tamper(bytes: &[u8], needle: &str, replacement: &str) -> Vec<u8> {
    assert_eq!(needle.len(), replacement.len());
    let mut out = bytes.to_vec();
    let at = out
        .windows(needle.len())
        .rposition(|window| window == needle.as_bytes())
        .unwrap_or_else(|| panic!("{needle} not found"));
    out[at..at + needle.len()].copy_from_slice(replacement.as_bytes());
    out
}

#[test]
fn changing_any_entry_breaks_the_trail_and_the_seal() {
    let bytes = full_history();
    for (needle, replacement) in [
        ("\"actor\": \"Ms Teacher\"", "\"actor\": \"Mr Teacher\""),
        (
            "\"signer\": \"Ada Lovelace\"",
            "\"signer\": \"Eve Lovelace\"",
        ),
        ("\"type\": \"saved\"", "\"type\": \"SAVED\""),
    ] {
        let tampered = tamper(&bytes, needle, replacement);
        let report = audit_trail(&tampered).unwrap();
        assert_eq!(report.verdict, AuditVerdict::Broken, "{needle}");
        assert!(!report.problems.is_empty());
        let checked = verify_document(&tampered).unwrap();
        assert!(
            !checked.signatures.last().unwrap().valid,
            "the time stamp no longer covers it"
        );
    }
}

#[test]
fn taking_entries_out_later_shows() {
    let bytes = full_history();
    // a later save that writes the trail again without its last entry,
    // with a perfectly consistent chain
    let mut doc = load(&bytes);
    let stream_id = doc
        .objects
        .iter()
        .filter(|(_, object)| {
            object.as_stream().is_ok_and(|stream| {
                stream.dict.get(b"Type").ok().and_then(|t| t.as_name().ok())
                    == Some(b"EmbeddedFile")
            })
        })
        .map(|(id, _)| *id)
        .max()
        .unwrap();
    let stream = doc
        .get_object(stream_id)
        .unwrap()
        .as_stream()
        .unwrap()
        .clone();
    let mut entries: Vec<serde_json::Value> = serde_json::from_slice(&stream.content).unwrap();
    entries.pop();
    let mut shorter = stream.clone();
    shorter.set_content(serde_json::to_vec_pretty(&entries).unwrap());
    *doc.get_object_mut(stream_id).unwrap() = lopdf::Object::Stream(shorter);
    let rewritten = save_incremental(&bytes, &doc).unwrap();
    let report = audit_trail(&rewritten).unwrap();
    assert_eq!(report.verdict, AuditVerdict::Broken);
    assert!(
        report.problems[0].contains("changed or removed audit entries"),
        "{:?}",
        report.problems
    );
}

#[test]
fn entries_after_the_last_seal_are_marked_unsealed() {
    let bytes = full_history();
    let note = AuditEvent::Note {
        text: "printed for the office".into(),
    };
    let later = append_audit_entry(&bytes, "Ada", &note).unwrap();
    let report = audit_trail(&later).unwrap();
    assert_eq!(report.verdict, AuditVerdict::Intact);
    assert!(!report.entries[3].sealed && report.entries[2].sealed);
    assert!(
        report.summary.contains("The last 1 aren’t sealed yet"),
        "{}",
        report.summary
    );
}

#[test]
fn audit_entries_are_allowed_after_a_certification() {
    let options = SigningOptions {
        certify: Some(CertificationLevel::FormFillingAndSigning),
        audit_actor: Some("Ms Teacher".into()),
        ..Default::default()
    };
    let certified =
        sign_with_options(&sample_form(&PageSetup::letter(0)), &request(), &options).unwrap();
    let saved = append_audit_entry(&certified, "Ada", &AuditEvent::Saved).unwrap();
    let checked = verify_document(&saved).unwrap();
    assert_eq!(
        checked.changes[0].verdict,
        ChangeVerdict::AsIntended,
        "{}",
        checked.changes[0].sentence
    );
    assert_eq!(audit_trail(&saved).unwrap().verdict, AuditVerdict::Intact);
}

#[test]
fn the_trail_can_be_added_as_a_readable_page() {
    let bytes = append_audit_entry(
        &sample_form(&PageSetup::letter(0)),
        "Ms Teacher",
        &AuditEvent::Created,
    )
    .unwrap();
    let with_page = add_audit_page(&bytes).unwrap();
    let doc = load(&with_page);
    assert_eq!(doc.get_pages().len(), 2);
    let text = doc.extract_text(&[2]).unwrap_or_default();
    assert!(
        text.contains("Audit trail")
            || String::from_utf8_lossy(&with_page).contains("(Audit trail)"),
        "{text}"
    );
    assert_eq!(
        audit_trail(&with_page).unwrap().verdict,
        AuditVerdict::Intact
    );
}
