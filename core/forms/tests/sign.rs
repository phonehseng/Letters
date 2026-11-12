//! digital signatures: sign, check, tamper, sign again, and cross-check
//! with openssl when it is installed.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use letters_forms::{
    create_self_signed_certificate, fields, sign, verify, CertificateRequest, FormError, KeyKind,
    SignRequest, SignatureAppearance, SignaturePlacement, TrustStatus, VisibleRect,
};
use support::{load, render_page, sample_form, signature_png, temp_dir, PageSetup};

const PASSWORD: &str = "correct horse";

fn certificate(kind: KeyKind, name: &str) -> Vec<u8> {
    let request = CertificateRequest {
        name: name.into(),
        email: Some("student@example.org".into()),
        organization: None,
        valid_days: 365,
        password: PASSWORD.into(),
        key_kind: kind,
    };
    let created = create_self_signed_certificate(&request).unwrap();
    assert!(created.self_signed);
    assert!(
        created.subject.contains("OU=Self-signed"),
        "{}",
        created.subject
    );
    assert!(created.note.contains("self-signed"));
    created.pkcs12
}

fn request(pkcs12: &[u8], placement: SignaturePlacement) -> SignRequest {
    SignRequest {
        pkcs12: pkcs12.to_vec(),
        password: PASSWORD.into(),
        placement,
        reason: Some("I wrote this essay".into()),
        location: Some("Dunedin".into()),
        name: None,
        contact_info: None,
        appearance: SignatureAppearance::default(),
    }
}

fn new_field(x: f32) -> SignaturePlacement {
    SignaturePlacement::NewField {
        page: 0,
        rect: VisibleRect::new(x, 650.0, 180.0, 60.0),
        name: None,
    }
}

fn form() -> Vec<u8> {
    sample_form(&PageSetup::letter(0))
}

#[test]
fn a_signed_document_checks_out() {
    let pkcs12 = certificate(KeyKind::Rsa2048, "Ada Lovelace");
    let original = form();
    let signed = sign(&original, &request(&pkcs12, new_field(72.0))).unwrap();
    // the original bytes are kept exactly: the signature is an incremental update
    assert_eq!(&signed[..original.len()], original.as_slice());

    let reports = verify(&signed).unwrap();
    assert_eq!(reports.len(), 1);
    let report = &reports[0];
    assert!(report.valid, "{report:#?}");
    assert!(report.digest_matches && report.signature_correct);
    assert!(report.covers_whole_file && !report.changed_after_signing);
    assert_eq!(report.signer.as_deref(), Some("Ada Lovelace"));
    assert_eq!(report.reason.as_deref(), Some("I wrote this essay"));
    assert_eq!(report.sub_filter.as_deref(), Some("adbe.pkcs7.detached"));
    assert_eq!(report.trust, TrustStatus::SelfSigned);
    let certificate = report.certificate.as_ref().unwrap();
    assert!(certificate.self_signed);
    assert_eq!(certificate.valid_when_signed, Some(true));
    assert!(report.summary.contains("self-signed"), "{}", report.summary);
    assert!(report.problems.is_empty(), "{:?}", report.problems);

    // the field reads back as signed
    let doc = load(&signed);
    let signature_field = fields(&doc)
        .unwrap()
        .into_iter()
        .find(|field| field.name == "Signature")
        .unwrap();
    assert!(signature_field.signed);
}

#[test]
fn ecdsa_keys_sign_too() {
    let pkcs12 = certificate(KeyKind::P256, "Grace Hopper");
    let signed = sign(&form(), &request(&pkcs12, new_field(72.0))).unwrap();
    let report = &verify(&signed).unwrap()[0];
    assert!(report.valid, "{report:#?}");
    assert_eq!(report.signer.as_deref(), Some("Grace Hopper"));
}

#[test]
fn changing_one_signed_byte_breaks_the_signature() {
    let pkcs12 = certificate(KeyKind::P256, "Ada Lovelace");
    let mut signed = sign(&form(), &request(&pkcs12, new_field(72.0))).unwrap();
    // turn the "fixed" value of the read-only field into "fixer"
    let at = find(&signed, b"(fixed)").unwrap() + 5;
    signed[at] = b'r';
    let report = &verify(&signed).unwrap()[0];
    assert!(!report.digest_matches);
    assert!(!report.valid);
    assert!(
        report.summary.contains("changed after it was signed"),
        "{}",
        report.summary
    );
}

#[test]
fn changes_after_signing_are_reported() {
    let pkcs12 = certificate(KeyKind::P256, "Ada Lovelace");
    let signed = sign(&form(), &request(&pkcs12, new_field(72.0))).unwrap();
    let changed = append_update(&signed);
    let report = &verify(&changed).unwrap()[0];
    // what was signed is still intact, but something was added afterwards
    assert!(report.valid);
    assert!(report.changed_after_signing);
    assert!(!report.covers_whole_file);
    assert_eq!(report.later_updates, 1);
    assert!(!report.later_updates_are_signatures);
    assert!(
        report.summary.contains("changed after it was signed"),
        "{}",
        report.summary
    );
}

#[test]
fn two_signatures_in_a_row_both_hold() {
    let first_key = certificate(KeyKind::Rsa2048, "Teacher");
    let second_key = certificate(KeyKind::P256, "Student");
    let once = sign(&form(), &request(&first_key, new_field(72.0))).unwrap();
    let twice_request = request(
        &second_key,
        SignaturePlacement::ExistingField {
            field: "sign_here".into(),
        },
    );
    let twice = sign(&once, &twice_request).unwrap();
    assert_eq!(&twice[..once.len()], once.as_slice());

    let reports = verify(&twice).unwrap();
    assert_eq!(reports.len(), 2);
    let first = reports
        .iter()
        .find(|report| report.signer.as_deref() == Some("Teacher"))
        .unwrap();
    let second = reports
        .iter()
        .find(|report| report.signer.as_deref() == Some("Student"))
        .unwrap();
    assert!(load(&twice).xref_recovery.is_none());
    assert!(first.valid && second.valid, "{reports:#?}");
    assert!(first.changed_after_signing && first.later_updates_are_signatures);
    assert_eq!(first.later_updates, 1);
    assert!(
        first.summary.contains("further signatures"),
        "{}",
        first.summary
    );
    assert!(second.covers_whole_file && !second.changed_after_signing);
}

#[test]
fn files_with_cross_reference_streams_sign_too() {
    // the same form saved the compact pdf 1.5 way: objects packed into
    // object streams and the cross-reference table stored as a stream
    let mut doc = load(&form());
    let options = lopdf::SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .build();
    let mut compact = Vec::new();
    doc.save_with_options(&mut compact, options).unwrap();
    assert!(
        find(&compact, b"/XRef").is_some(),
        "the test file should use an xref stream"
    );

    let first_key = certificate(KeyKind::P256, "Ada Lovelace");
    let once = sign(&compact, &request(&first_key, new_field(72.0))).unwrap();
    let twice = sign(
        &once,
        &request(
            &first_key,
            SignaturePlacement::ExistingField {
                field: "sign_here".into(),
            },
        ),
    )
    .unwrap();
    let reports = verify(&twice).unwrap();
    assert_eq!(reports.len(), 2);
    assert!(reports.iter().all(|report| report.valid), "{reports:#?}");
    // the new sections are read as written, not rebuilt by a repair pass
    assert!(load(&twice).xref_recovery.is_none());
    let _ = render_page(&twice, 0, 500, "xref-stream-signed");
}

#[test]
fn invisible_signatures_work() {
    let pkcs12 = certificate(KeyKind::P256, "Ada Lovelace");
    let signed = sign(
        &form(),
        &request(&pkcs12, SignaturePlacement::Invisible { name: None }),
    )
    .unwrap();
    assert!(verify(&signed).unwrap()[0].valid);
}

#[test]
fn refuses_what_cannot_be_signed() {
    let pkcs12 = certificate(KeyKind::P256, "Ada Lovelace");
    let mut wrong_password = request(&pkcs12, new_field(72.0));
    wrong_password.password = "wrong".into();
    assert_eq!(
        sign(&form(), &wrong_password).unwrap_err(),
        FormError::WrongCertificatePassword
    );
    assert_eq!(
        FormError::WrongCertificatePassword.to_string(),
        "The password for this certificate file isn’t right."
    );

    let not_a_signature = request(
        &pkcs12,
        SignaturePlacement::ExistingField {
            field: "name".into(),
        },
    );
    assert_eq!(
        sign(&form(), &not_a_signature).unwrap_err(),
        FormError::NotASignatureField("name".into())
    );

    let existing = request(
        &pkcs12,
        SignaturePlacement::ExistingField {
            field: "sign_here".into(),
        },
    );
    let signed = sign(&form(), &existing).unwrap();
    assert_eq!(
        sign(&signed, &existing).unwrap_err(),
        FormError::AlreadySigned("sign_here".into())
    );

    let mut garbage = request(&pkcs12, new_field(72.0));
    garbage.pkcs12 = b"not a certificate".to_vec();
    assert!(matches!(
        sign(&form(), &garbage).unwrap_err(),
        FormError::BadCertificate(_)
    ));
}

#[test]
fn pdfium_opens_and_draws_the_signed_file() {
    let pkcs12 = certificate(KeyKind::Rsa2048, "Ada Lovelace");
    let mut with_picture = request(&pkcs12, new_field(72.0));
    with_picture.appearance = SignatureAppearance {
        image_png: Some(signature_png(120, 40)),
        show_details: true,
    };
    let signed = sign(&form(), &with_picture).unwrap();
    let image = render_page(&signed, 0, 1000, "signed-render");
    let rect = VisibleRect::new(72.0, 650.0, 180.0, 60.0);
    assert!(
        image.ink_in(rect) > 200,
        "the visible signature isn't drawn"
    );
    // and signing twice still opens
    let twice = sign(
        &signed,
        &request(
            &pkcs12,
            SignaturePlacement::ExistingField {
                field: "sign_here".into(),
            },
        ),
    )
    .unwrap();
    let _ = render_page(&twice, 0, 500, "signed-twice-render");
}

/// adds an incremental update by hand: one new object and a new
/// cross-reference section pointing back to the old one.
fn append_update(bytes: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(bytes);
    let startxref = text.rfind("startxref").unwrap();
    let previous: usize = text[startxref + 9..]
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let doc = load(bytes);
    let root = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
    let number = doc.max_id + 1;
    let mut out = bytes.to_vec();
    let offset = out.len();
    out.extend_from_slice(format!("{number} 0 obj\n(added later)\nendobj\n").as_bytes());
    let xref = out.len();
    out.extend_from_slice(
        format!(
            "xref\n{number} 1\n{offset:010} 00000 n \ntrailer\n<< /Size {} /Root {} {} R /Prev {previous} >>\nstartxref\n{xref}\n%%EOF\n",
            number + 1,
            root.0,
            root.1
        )
        .as_bytes(),
    );
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// openssl, if this computer has it: on the path or in the usual places.
fn openssl() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("openssl"),
        PathBuf::from(r"C:\msys64\ucrt64\bin\openssl.exe"),
        PathBuf::from(r"C:\Program Files\Git\usr\bin\openssl.exe"),
        PathBuf::from(r"C:\Program Files\Git\mingw64\bin\openssl.exe"),
    ];
    candidates.into_iter().find(|path| {
        Command::new(path)
            .arg("version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    })
}

fn run(openssl: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(openssl).args(args).output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

/// writes the signature's cms structure and the signed bytes to files and
/// asks openssl to check one against the other.
fn openssl_verifies(openssl: &Path, signed: &[u8], dir: &Path, label: &str) {
    let doc = load(signed);
    let catalog = doc.catalog().unwrap();
    let form = doc
        .get_dictionary(catalog.get(b"AcroForm").unwrap().as_reference().unwrap())
        .unwrap();
    let mut found = None;
    for field in form.get(b"Fields").unwrap().as_array().unwrap() {
        let dict = doc.get_dictionary(field.as_reference().unwrap()).unwrap();
        if let Ok(value) = dict.get(b"V") {
            if let Ok(id) = value.as_reference() {
                if let Ok(signature) = doc.get_dictionary(id) {
                    if signature.has(b"ByteRange") {
                        found = Some(signature.clone());
                    }
                }
            }
        }
    }
    let signature = found.expect("a signature dictionary");
    let range: Vec<usize> = signature
        .get(b"ByteRange")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_i64().unwrap() as usize)
        .collect();
    let contents = signature.get(b"Contents").unwrap().as_str().unwrap();
    // the cms structure without the zero padding after it
    let length = if contents[1] & 0x80 == 0 {
        contents[1] as usize + 2
    } else {
        let count = (contents[1] & 0x7F) as usize;
        contents[2..2 + count]
            .iter()
            .fold(0usize, |total, byte| total << 8 | *byte as usize)
            + 2
            + count
    };
    let mut data = signed[range[0]..range[0] + range[1]].to_vec();
    data.extend_from_slice(&signed[range[2]..range[2] + range[3]]);
    let cms_path = dir.join(format!("{label}.p7s"));
    let data_path = dir.join(format!("{label}.bin"));
    std::fs::write(&cms_path, &contents[..length]).unwrap();
    std::fs::write(&data_path, &data).unwrap();
    let (ok, text) = run(
        openssl,
        &[
            "cms",
            "-verify",
            "-binary",
            "-noverify",
            "-inform",
            "DER",
            "-in",
            cms_path.to_str().unwrap(),
            "-content",
            data_path.to_str().unwrap(),
            "-out",
            dir.join(format!("{label}.out")).to_str().unwrap(),
        ],
    );
    assert!(
        ok && text.contains("Verification successful"),
        "openssl said for {label}: {text}"
    );
}

#[test]
fn openssl_agrees_with_our_signatures() {
    let Some(openssl) = openssl() else {
        eprintln!("openssl was not found; skipping the independent cross-check");
        return;
    };
    let dir = temp_dir("openssl-cross-check");
    for (kind, label) in [(KeyKind::Rsa2048, "rsa"), (KeyKind::P256, "ecdsa")] {
        let pkcs12 = certificate(kind, "Cross Check");
        // our certificate file opens in openssl with its password
        let p12_path = dir.join(format!("{label}.p12"));
        std::fs::write(&p12_path, &pkcs12).unwrap();
        let (ok, text) = run(
            &openssl,
            &[
                "pkcs12",
                "-in",
                p12_path.to_str().unwrap(),
                "-passin",
                &format!("pass:{PASSWORD}"),
                "-nokeys",
                "-info",
            ],
        );
        assert!(
            ok && text.contains("CN=Cross Check") || ok && text.contains("CN = Cross Check"),
            "openssl could not read our p12: {text}"
        );

        let signed = sign(&form(), &request(&pkcs12, new_field(72.0))).unwrap();
        openssl_verifies(&openssl, &signed, &dir, label);
    }
}

#[test]
fn certificate_files_made_by_openssl_sign_too_including_pss_keys() {
    let Some(openssl) = openssl() else {
        eprintln!("openssl was not found; skipping certificate files made by openssl");
        return;
    };
    let dir = temp_dir("openssl-made-keys");
    for (algorithm, label) in [
        ("rsa", "openssl-rsa"),
        ("rsa-pss", "openssl-pss"),
        ("ec", "openssl-ec"),
    ] {
        let key = dir.join(format!("{label}.key"));
        let cert = dir.join(format!("{label}.crt"));
        let p12 = dir.join(format!("{label}.p12"));
        let mut args = vec!["req", "-x509", "-newkey", algorithm];
        match algorithm {
            "ec" => args.extend(["-pkeyopt", "ec_paramgen_curve:P-256"]),
            _ => args.extend(["-pkeyopt", "rsa_keygen_bits:2048"]),
        }
        args.extend([
            "-keyout",
            key.to_str().unwrap(),
            "-out",
            cert.to_str().unwrap(),
            "-days",
            "30",
            "-nodes",
            "-subj",
            "/CN=Made By OpenSSL",
        ]);
        let (ok, text) = run(&openssl, &args);
        assert!(ok, "openssl req failed: {text}");
        let (ok, text) = run(
            &openssl,
            &[
                "pkcs12",
                "-export",
                "-inkey",
                key.to_str().unwrap(),
                "-in",
                cert.to_str().unwrap(),
                "-out",
                p12.to_str().unwrap(),
                "-passout",
                &format!("pass:{PASSWORD}"),
            ],
        );
        assert!(ok, "openssl pkcs12 failed: {text}");
        let pkcs12 = std::fs::read(&p12).unwrap();
        let signed = sign(&form(), &request(&pkcs12, new_field(72.0))).unwrap();
        let report = &verify(&signed).unwrap()[0];
        assert!(report.valid, "{label}: {report:#?}");
        assert_eq!(report.signer.as_deref(), Some("Made By OpenSSL"));
        openssl_verifies(&openssl, &signed, &dir, label);

        let mut wrong = request(&pkcs12, new_field(72.0));
        wrong.password = "nope".into();
        assert_eq!(
            sign(&form(), &wrong).unwrap_err(),
            FormError::WrongCertificatePassword,
            "{label}"
        );
    }
}
