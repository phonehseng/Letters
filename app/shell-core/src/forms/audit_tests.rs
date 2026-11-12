//! the audit trail run the way the app runs it: entries and audit pages
//! appended to the saved file through the edit session, on a file lopdf
//! saved (which ends right after %%EOF), then a signature that seals them.

use std::fs;
use std::path::{Path, PathBuf};

use letters_forms::{
    add_audit_page, append_audit_entry, audit_trail, create_self_signed_certificate,
    sign_with_options, AuditEvent, AuditVerdict, CertificateRequest, KeyKind, SignRequest,
    SignatureAppearance, SignaturePlacement, SigningOptions,
};

use super::sign_saved_document;
use crate::session::EditSession;
use crate::test_pdf::{temp_dir, write_pdf};

fn show(path: &Path) -> Result<PathBuf, String> {
    Ok(path.to_path_buf())
}

fn prepared() -> AuditEvent {
    AuditEvent::Prepared { note: None }
}

#[test]
fn a_trail_started_on_a_saved_file_is_intact_and_a_signature_seals_it() {
    let dir = temp_dir("audit-session");
    let original = dir.join("form.pdf");
    write_pdf(&original, 1);
    assert!(
        !fs::read(&original).unwrap().ends_with(b"\n"),
        "lopdf ends the file at %%EOF"
    );
    let mut session = EditSession::start(&original, &dir.join("sessions")).unwrap();

    sign_saved_document(
        &mut session,
        false,
        |bytes| append_audit_entry(bytes, "Ada", &prepared()),
        show,
    )
    .unwrap();
    sign_saved_document(&mut session, false, add_audit_page, show).unwrap();
    let report = audit_trail(&fs::read(&original).unwrap()).unwrap();
    assert_eq!(
        report.verdict,
        AuditVerdict::Intact,
        "{:?}",
        report.problems
    );

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
        audit_actor: Some("Ada".into()),
        ..SigningOptions::default()
    };
    sign_saved_document(
        &mut session,
        false,
        |bytes| sign_with_options(bytes, &request, &options),
        show,
    )
    .unwrap();

    let report = audit_trail(&fs::read(&original).unwrap()).unwrap();
    assert_eq!(
        report.verdict,
        AuditVerdict::Intact,
        "{:?}",
        report.problems
    );
    assert!(report.entries.len() >= 2);
    assert!(
        report.entries[0].sealed,
        "the signature seals the entries before it"
    );
}
