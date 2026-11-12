//! time stamps from a time stamp authority the person chose (rfc 3161),
//! tested against a fake authority on 127.0.0.1 and cross-checked with
//! `openssl ts` when openssl is installed. no test contacts the internet.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use cms::content_info::ContentInfo;
use cms::signed_data::SignedData;
use der::{Decode, Encode};
use letters_forms::{
    add_document_timestamp, create_self_signed_certificate, sign, sign_with_options,
    verify_document, CertificateRequest, FormError, KeyKind, SignRequest, SignatureAppearance,
    SignatureKind, SignaturePlacement, SigningOptions, TimeSource, TimestampAuthority,
    CLOCK_TIME_NOTE, TIMESTAMP_DISCLOSURE,
};
use sha2::{Digest, Sha256};
use support::tsa::{prepare_openssl_authority, Behaviour, FakeTsa};
use support::{load, sample_form, temp_dir, PageSetup};

const PASSWORD: &str = "correct horse";

fn request(name: &str) -> SignRequest {
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
        placement: SignaturePlacement::Invisible { name: None },
        reason: None,
        location: None,
        name: None,
        contact_info: None,
        appearance: SignatureAppearance::default(),
    }
}

fn authority(tsa: &FakeTsa) -> TimestampAuthority {
    TimestampAuthority {
        url: tsa.url.clone(),
        username: None,
        password: None,
    }
}

fn with_timestamp(tsa: &FakeTsa) -> SigningOptions {
    SigningOptions {
        timestamp: Some(authority(tsa)),
        ..Default::default()
    }
}

fn form() -> Vec<u8> {
    sample_form(&PageSetup::letter(0))
}

#[test]
fn the_disclosure_says_only_a_hash_is_sent() {
    assert!(TIMESTAMP_DISCLOSURE.contains("fingerprint (a SHA-256 hash)"));
    assert!(TIMESTAMP_DISCLOSURE.contains("never leaves this computer"));
}

#[test]
fn a_signature_time_stamp_comes_from_the_chosen_authority() {
    let dir = temp_dir("signature-time-stamp");
    let tsa = FakeTsa::start(Behaviour::Honest, &dir);
    let signed =
        sign_with_options(&form(), &request("Ada Lovelace"), &with_timestamp(&tsa)).unwrap();
    let checked = verify_document(&signed).unwrap();
    assert!(checked.signatures[0].valid, "{:#?}", checked.signatures[0]);
    let details = &checked.details[0];
    assert_eq!(details.kind, SignatureKind::Approval);
    assert_eq!(details.time_source, TimeSource::TimestampAuthority);
    let stamp = details.timestamp.as_ref().unwrap();
    assert!(stamp.is_valid(), "{stamp:#?}");
    assert!(stamp
        .authority
        .as_deref()
        .unwrap()
        .contains("Letters Test Time Stamps"));
    assert!(details
        .time_note
        .contains("vouched for by the time stamp authority Letters Test Time Stamps"));

    if let Some(openssl) = openssl() {
        let (token, signature_value) = signature_token(&signed);
        let token_path = dir.join("token.der");
        std::fs::write(&token_path, &token).unwrap();
        let ca = dir.join("tsa.pem");
        std::fs::write(&ca, &tsa.certificate_pem).unwrap();
        let digest = hex(&Sha256::digest(&signature_value));
        let (ok, text) = run(
            &openssl,
            &[
                "ts",
                "-verify",
                "-in",
                path(&token_path),
                "-token_in",
                "-digest",
                &digest,
                "-CAfile",
                path(&ca),
            ],
        );
        assert!(ok && text.contains("Verification: OK"), "openssl: {text}");
        let (ok, text) = run(
            &openssl,
            &[
                "ts",
                "-reply",
                "-in",
                path(&token_path),
                "-token_in",
                "-text",
            ],
        );
        assert!(
            ok && text.contains("Hash Algorithm: sha256"),
            "openssl: {text}"
        );
    } else {
        eprintln!("openssl was not found; skipping the independent cross-check");
    }
}

#[test]
fn a_signature_without_a_time_stamp_says_the_time_is_the_signers_clock() {
    let signed = sign(&form(), &request("Grace Hopper")).unwrap();
    let details = &verify_document(&signed).unwrap().details[0];
    assert_eq!(details.time_source, TimeSource::SignerClock);
    assert!(details.timestamp.is_none());
    assert_eq!(details.time_note, CLOCK_TIME_NOTE);
    assert_eq!(
        CLOCK_TIME_NOTE,
        "The time comes from the signer’s computer clock."
    );
}

#[test]
fn a_document_time_stamp_seals_the_file_and_later_tampering_shows() {
    let dir = temp_dir("document-time-stamp");
    let tsa = FakeTsa::start(Behaviour::Honest, &dir);
    let signed = sign(&form(), &request("Ada Lovelace")).unwrap();
    let sealed = add_document_timestamp(&signed, &authority(&tsa)).unwrap();
    assert_eq!(
        &sealed[..signed.len()],
        signed.as_slice(),
        "an incremental update"
    );
    let checked = verify_document(&sealed).unwrap();
    assert_eq!(checked.signatures.len(), 2);
    let first = &checked.signatures[0];
    assert!(
        first.valid && first.later_updates_are_signatures,
        "{first:#?}"
    );
    let stamp = &checked.signatures[1];
    assert!(stamp.valid, "{stamp:#?}");
    assert_eq!(stamp.sub_filter.as_deref(), Some("ETSI.RFC3161"));
    assert_eq!(checked.details[1].kind, SignatureKind::DocumentTimestamp);
    assert_eq!(
        checked.details[1].time_source,
        TimeSource::TimestampAuthority
    );

    if let Some(openssl) = openssl() {
        let (token, covered) = document_token(&sealed);
        let token_path = dir.join("doc-token.der");
        let data_path = dir.join("covered.bin");
        std::fs::write(&token_path, token).unwrap();
        std::fs::write(&data_path, covered).unwrap();
        let ca = dir.join("tsa.pem");
        std::fs::write(&ca, &tsa.certificate_pem).unwrap();
        let (ok, text) = run(
            &openssl,
            &[
                "ts",
                "-verify",
                "-in",
                path(&token_path),
                "-token_in",
                "-data",
                path(&data_path),
                "-CAfile",
                path(&ca),
            ],
        );
        assert!(ok && text.contains("Verification: OK"), "openssl: {text}");
    }

    // one changed byte in the signed part breaks both
    let mut tampered = sealed.clone();
    let at = find(&tampered, b"(fixed)").unwrap() + 5;
    tampered[at] = b'r';
    let checked = verify_document(&tampered).unwrap();
    assert!(!checked.signatures[1].valid);
    assert!(
        !checked.details[1]
            .timestamp
            .as_ref()
            .unwrap()
            .imprint_matches
    );
}

#[test]
fn answers_that_do_not_check_out_are_refused() {
    let dir = temp_dir("bad-time-stamps");
    let cases = [
        (Behaviour::WrongImprint, "doesn’t check out"),
        (Behaviour::WrongNonce, "different request"),
        (Behaviour::Refuse, "refused the request. not today"),
        (
            Behaviour::NoSigningCertificate,
            "doesn’t name the authority’s certificate",
        ),
    ];
    for (behaviour, expected) in cases {
        let tsa = FakeTsa::start(behaviour.clone(), &dir);
        let error = sign_with_options(&form(), &request("Ada"), &with_timestamp(&tsa)).unwrap_err();
        assert!(matches!(error, FormError::TimestampFailed(_)), "{error:?}");
        assert!(
            error.to_string().contains(expected),
            "{behaviour:?}: {error}"
        );
    }
    let nowhere = TimestampAuthority {
        url: "http://127.0.0.1:9/tsr".into(),
        username: None,
        password: None,
    };
    let options = SigningOptions {
        timestamp: Some(nowhere),
        ..Default::default()
    };
    let error = sign_with_options(&form(), &request("Ada"), &options).unwrap_err();
    assert!(error.to_string().contains("couldn’t be reached"), "{error}");
}

#[test]
fn authorities_that_ask_for_a_password_get_it() {
    let dir = temp_dir("time-stamp-password");
    let tsa = FakeTsa::start(Behaviour::Password("student".into(), "s3cret".into()), &dir);
    let mut options = with_timestamp(&tsa);
    let error = sign_with_options(&form(), &request("Ada"), &options).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("didn’t accept the name and password"),
        "{error}"
    );
    if let Some(stamp) = options.timestamp.as_mut() {
        stamp.username = Some("student".into());
        stamp.password = Some("s3cret".into());
    }
    let signed = sign_with_options(&form(), &request("Ada"), &options).unwrap();
    assert_eq!(
        verify_document(&signed).unwrap().details[0].time_source,
        TimeSource::TimestampAuthority
    );
}

#[test]
fn tokens_made_by_openssl_are_accepted() {
    let Some(openssl) = openssl() else {
        eprintln!("openssl was not found; skipping");
        return;
    };
    let dir = temp_dir("openssl-time-stamps");
    prepare_openssl_authority(&openssl, &dir);
    let tsa = FakeTsa::start(Behaviour::OpenSsl(openssl.clone()), &dir);
    let signed = sign_with_options(&form(), &request("Ada"), &with_timestamp(&tsa)).unwrap();
    let checked = verify_document(&signed).unwrap();
    let stamp = checked.details[0].timestamp.as_ref().unwrap();
    assert!(stamp.is_valid(), "{stamp:#?}");
    assert!(stamp
        .authority
        .as_deref()
        .unwrap()
        .contains("OpenSSL Test TSA"));
    let sealed = add_document_timestamp(&signed, &authority(&tsa)).unwrap();
    let checked = verify_document(&sealed).unwrap();
    assert!(
        checked.signatures.iter().all(|report| report.valid),
        "{:#?}",
        checked.signatures
    );
}

// helpers

fn openssl() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(r"C:\msys64\ucrt64\bin\openssl.exe"),
        PathBuf::from("openssl"),
        PathBuf::from(r"C:\Program Files\Git\usr\bin\openssl.exe"),
    ];
    candidates.into_iter().find(|candidate| {
        Command::new(candidate)
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

fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// the /Contents and /ByteRange of every signature dictionary, in file order.
fn signature_dictionaries(bytes: &[u8]) -> Vec<(Vec<u8>, Vec<usize>)> {
    let doc = load(bytes);
    let mut found = Vec::new();
    for object in doc.objects.values() {
        let Ok(dict) = object.as_dict() else { continue };
        let (Ok(contents), Ok(range)) = (dict.get(b"Contents"), dict.get(b"ByteRange")) else {
            continue;
        };
        let contents = contents.as_str().unwrap().to_vec();
        let range: Vec<usize> = range
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_i64().unwrap() as usize)
            .collect();
        found.push((contents, range));
    }
    found.sort_by_key(|(_, range)| range[1]);
    found
}

fn trimmed(contents: &[u8]) -> Vec<u8> {
    let any = der::Any::from_der(&contents[..der_length(contents)]).unwrap();
    any.to_der().unwrap()
}

fn der_length(bytes: &[u8]) -> usize {
    let first = bytes[1] as usize;
    if first < 0x80 {
        return 2 + first;
    }
    let count = first & 0x7F;
    let length = bytes[2..2 + count]
        .iter()
        .fold(0usize, |total, byte| total << 8 | *byte as usize);
    2 + count + length
}

/// the signature time stamp token of the first signature, and the
/// signature value it covers.
fn signature_token(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let (contents, _) = &signature_dictionaries(bytes)[0];
    let info = ContentInfo::from_der(&trimmed(contents)).unwrap();
    let signed: SignedData = info.content.decode_as().unwrap();
    let signer = signed.signer_infos.0.iter().next().unwrap();
    let attribute = signer
        .unsigned_attrs
        .as_ref()
        .unwrap()
        .iter()
        .next()
        .unwrap();
    let token = attribute.values.iter().next().unwrap().to_der().unwrap();
    (token, signer.signature.as_bytes().to_vec())
}

/// the token of the last (document) time stamp and the bytes it covers.
fn document_token(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let dictionaries = signature_dictionaries(bytes);
    let (contents, range) = dictionaries.last().unwrap();
    let mut covered = bytes[range[0]..range[0] + range[1]].to_vec();
    covered.extend_from_slice(&bytes[range[2]..range[2] + range[3]]);
    (trimmed(contents), covered)
}
