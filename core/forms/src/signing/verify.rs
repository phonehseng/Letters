//! checking the signatures in a document.
//!
//! for each signature letters checks that the signed bytes still have the
//! digest the signature holds, that the signature over that digest is
//! correct for the certificate inside it, and whether anything was added
//! to the file after it was signed. letters does not check whether the
//! certificate should be trusted: it has no list of trusted authorities
//! and does not ask the internet whether a certificate was revoked.

use std::time::SystemTime;

use cms::signed_data::{SignedData, SignerIdentifier, SignerInfo};
use der::asn1::OctetString;
use der::{Decode, Encode};
use lopdf::{Document, Object};
use p256::ecdsa::signature::hazmat::PrehashVerifier;
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::{Pkcs1v15Sign, PublicKey as RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Sha384, Sha512};
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};
use x509_cert::Certificate;

use super::cms::{digest, parse_signed_data};
use super::keys::{common_name, PssParameters};
use super::oids;
use super::sign::readable_date;
use crate::error::FormResult;
use crate::objects::{check_document, get, get_text};
use crate::read::{kind_of, FieldKind};
use crate::tree::collect_fields;

/// said with every result, because a correct signature is not the same as
/// a trusted signer.
pub const TRUST_NOTE: &str = "Letters checks that the signature is correct and the document unchanged. It does not check certificate authorities, trust lists or whether a certificate was revoked, so it can’t confirm who the signer really is.";

/// how far the signer's certificate could be checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrustStatus {
    /// the certificate signed itself: anyone can make one with any name
    SelfSigned,
    /// issued by someone else, whose trustworthiness letters can't check
    NotChecked,
}

/// the signer's certificate, in words.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateSummary {
    pub subject: String,
    pub issuer: String,
    pub common_name: Option<String>,
    pub self_signed: bool,
    pub valid_from: String,
    pub valid_until: String,
    /// whether the signing time falls inside the certificate's dates
    pub valid_when_signed: Option<bool>,
    /// the subjects of the other certificates the signature carries
    pub chain: Vec<String>,
}

/// what was found about one signature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureReport {
    pub field: String,
    /// the certificate's common name (cn)
    pub signer: Option<String>,
    /// the name written in the signature dictionary (/Name)
    pub name: Option<String>,
    /// when, as the signer's computer claimed it (no time stamp authority)
    pub signed_at: Option<String>,
    pub reason: Option<String>,
    pub location: Option<String>,
    pub sub_filter: Option<String>,
    /// the signed bytes still have the digest the signature holds
    pub digest_matches: bool,
    /// the signature is mathematically correct for the certificate's key
    pub signature_correct: bool,
    /// both of the above: these bytes were signed by this certificate's key
    pub valid: bool,
    /// the signature covers the whole file, up to its last byte
    pub covers_whole_file: bool,
    /// bytes were added to the file after this signature
    pub changed_after_signing: bool,
    /// how many incremental updates came after this signature
    pub later_updates: u32,
    /// every later update ends with another signature
    pub later_updates_are_signatures: bool,
    pub certificate: Option<CertificateSummary>,
    pub trust: TrustStatus,
    /// one or two sentences for the person using the app
    pub summary: String,
    /// anything that went wrong while checking
    pub problems: Vec<String>,
}

/// checks every signature in a document.
pub fn verify(doc_bytes: &[u8]) -> FormResult<Vec<SignatureReport>> {
    let doc = Document::load_mem(doc_bytes)?;
    check_document(&doc)?;
    let mut found = Vec::new();
    for field in collect_fields(&doc) {
        if kind_of(&doc, &field) != Some(FieldKind::Signature) {
            continue;
        }
        let Some(Object::Dictionary(signature)) = field.inherited(&doc, b"V") else {
            continue;
        };
        found.push((field.name.clone(), signature.clone()));
    }
    let ranges: Vec<Option<[usize; 4]>> = found
        .iter()
        .map(|(_, dict)| byte_range(&doc, dict))
        .collect();
    let ends: Vec<usize> = ranges
        .iter()
        .flatten()
        .map(|range| range[2] + range[3])
        .collect();
    let revision_ends = revision_ends(doc_bytes);

    let reports = found
        .iter()
        .zip(&ranges)
        .map(|((field, dict), range)| {
            check_one(&doc, doc_bytes, field, dict, *range, &ends, &revision_ends)
        })
        .collect();
    Ok(reports)
}

/// the four numbers of /ByteRange, if they make sense for this file.
fn byte_range(doc: &Document, dict: &lopdf::Dictionary) -> Option<[usize; 4]> {
    let values = get(doc, dict, b"ByteRange")?.as_array().ok()?;
    if values.len() != 4 {
        return None;
    }
    let mut numbers = [0usize; 4];
    for (slot, value) in numbers.iter_mut().zip(values) {
        *slot = usize::try_from(value.as_i64().ok()?).ok()?;
    }
    Some(numbers)
}

/// where each revision of the file ends (just after each %%EOF).
fn revision_ends(bytes: &[u8]) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut from = 0;
    while let Some(position) = bytes[from..]
        .windows(5)
        .position(|window| window == b"%%EOF")
    {
        let mut end = from + position + 5;
        while end < bytes.len() && (bytes[end] == b'\r' || bytes[end] == b'\n') {
            end += 1;
        }
        ends.push(end);
        from = end;
    }
    ends
}

#[allow(clippy::too_many_arguments)]
fn check_one(
    doc: &Document,
    bytes: &[u8],
    field: &str,
    dict: &lopdf::Dictionary,
    range: Option<[usize; 4]>,
    signature_ends: &[usize],
    revision_ends: &[usize],
) -> SignatureReport {
    let mut report = SignatureReport {
        field: field.to_owned(),
        signer: None,
        name: get_text(doc, dict, b"Name"),
        signed_at: get_text(doc, dict, b"M").map(|date| readable_pdf_date(&date)),
        reason: get_text(doc, dict, b"Reason"),
        location: get_text(doc, dict, b"Location"),
        sub_filter: crate::objects::get_name(doc, dict, b"SubFilter")
            .map(|name| String::from_utf8_lossy(&name).into_owned()),
        digest_matches: false,
        signature_correct: false,
        valid: false,
        covers_whole_file: false,
        changed_after_signing: false,
        later_updates: 0,
        later_updates_are_signatures: true,
        certificate: None,
        trust: TrustStatus::NotChecked,
        summary: String::new(),
        problems: Vec::new(),
    };
    let mut signing_moment = None;
    match get(doc, dict, b"Contents") {
        Some(Object::String(contents, _)) => {
            let covered = range.and_then(|range| covered_parts(bytes, range));
            if let Some(range) = range {
                check_covered_bytes(&mut report, bytes, range, signature_ends, revision_ends);
            }
            if covered.is_none() {
                // the byte range no longer fits the file: it was rewritten
                // (or cut) after signing. the signature itself can still say
                // who signed and when.
                report.changed_after_signing = true;
                report.covers_whole_file = false;
                if range.is_none() {
                    report
                        .problems
                        .push("The signature has no byte range.".into());
                }
            }
            signing_moment = check_cms(&mut report, contents, covered.as_ref());
        }
        _ => report
            .problems
            .push("The signature has no signature data.".into()),
    }
    report.valid = report.digest_matches && report.signature_correct;
    report.summary = summary(&report, signing_moment);
    report
}

/// the two covered parts of the file, if the range is well formed: it
/// starts at 0, stays inside the file, and leaves out exactly the
/// signature's hex string, angle brackets included.
fn covered_parts(bytes: &[u8], range: [usize; 4]) -> Option<[&[u8]; 2]> {
    let [first_start, first_length, second_start, second_length] = range;
    let first_end = first_start.checked_add(first_length)?;
    let second_end = second_start.checked_add(second_length)?;
    let well_formed = first_start == 0
        && first_end < second_start
        && second_end <= bytes.len()
        && bytes.get(first_end) == Some(&b'<')
        && bytes.get(second_start - 1) == Some(&b'>');
    well_formed.then(|| [&bytes[..first_end], &bytes[second_start..second_end]])
}

fn check_covered_bytes(
    report: &mut SignatureReport,
    bytes: &[u8],
    range: [usize; 4],
    signature_ends: &[usize],
    revision_ends: &[usize],
) {
    let covered_end = range[2].saturating_add(range[3]);
    report.covers_whole_file = covered_end == bytes.len();
    // trailing line breaks after the last %%EOF don't count as a change
    let trailing_only_whitespace = bytes
        .get(covered_end..)
        .map(|rest| rest.iter().all(|byte| byte.is_ascii_whitespace()))
        .unwrap_or(false);
    report.changed_after_signing = !trailing_only_whitespace;
    let later: Vec<usize> = revision_ends
        .iter()
        .copied()
        .filter(|end| *end > covered_end + 2)
        .collect();
    report.later_updates = later.len() as u32;
    report.later_updates_are_signatures = later.iter().all(|end| {
        signature_ends
            .iter()
            .any(|signature_end| end.abs_diff(*signature_end) <= 2)
    });
    if covered_parts(bytes, range).is_none() {
        report
            .problems
            .push("The byte range doesn’t match where the signature sits in the file.".into());
    }
}

/// reads the cms signature: who signed, when, and with which certificate,
/// then checks it. `covered` is none when the byte range no longer fits
/// the file; the signer is still read, but the document can't be checked.
/// returns the signing time from the signature, if it has one.
fn check_cms(
    report: &mut SignatureReport,
    contents: &[u8],
    covered: Option<&[&[u8]; 2]>,
) -> Option<SystemTime> {
    let signed_data = match parse_signed_data(contents) {
        Ok(signed_data) => signed_data,
        Err(problem) => {
            report
                .problems
                .push(format!("The signature data can’t be read: {problem}."));
            return None;
        }
    };
    let Some(signer_info) = signed_data.signer_infos.0.iter().next() else {
        report
            .problems
            .push("The signature names no signer.".into());
        return None;
    };
    let signing_time = signing_time(signer_info);
    if let Some(time) = signing_time {
        report.signed_at = Some(readable_date(time));
    }
    let certificates = certificates_of(&signed_data);
    let Some(certificate) = find_signer_certificate(&certificates, &signer_info.sid) else {
        report
            .problems
            .push("The signer’s certificate isn’t included in the signature.".into());
        return signing_time;
    };
    report.signer = common_name(&certificate.tbs_certificate().subject().to_string());
    let summary = summarise_certificate(certificate, &certificates, signing_time);
    report.trust = if summary.self_signed {
        TrustStatus::SelfSigned
    } else {
        TrustStatus::NotChecked
    };
    report.certificate = Some(summary);

    let message = match covered {
        Some(covered) => {
            let Some(content_digest) = digest(&signer_info.digest_alg.oid, covered) else {
                report.problems.push(format!(
                    "The digest algorithm {} isn’t supported.",
                    signer_info.digest_alg.oid
                ));
                return signing_time;
            };
            signed_message(report, signer_info, &content_digest)
        }
        // the document's bytes can't be compared any more, but the signed
        // attributes can still be checked against the certificate
        None => {
            report.problems.push(
                "The file was rewritten after signing, so the signed bytes can’t be checked."
                    .into(),
            );
            signer_info
                .signed_attrs
                .as_ref()
                .and_then(|attributes| attributes.to_der().ok())
        }
    };
    let Some(message) = message else {
        return signing_time;
    };
    let key = &certificate
        .tbs_certificate()
        .subject_public_key_info()
        .clone();
    match verify_signature(
        key,
        &signer_info.signature_algorithm,
        &signer_info.digest_alg.oid,
        &message,
        signer_info.signature.as_bytes(),
    ) {
        Ok(()) => report.signature_correct = true,
        Err(problem) => report.problems.push(problem),
    }
    signing_time
}

/// what the signature is over: the signed attributes when there are any
/// (after checking their digest attribute), or the document digest itself.
fn signed_message(
    report: &mut SignatureReport,
    signer_info: &SignerInfo,
    content_digest: &[u8],
) -> Option<Vec<u8>> {
    let Some(attributes) = &signer_info.signed_attrs else {
        report.digest_matches = true;
        return Some(content_digest.to_vec());
    };
    let stored = attributes
        .iter()
        .find(|attribute| attribute.oid == oids::MESSAGE_DIGEST)
        .and_then(|attribute| attribute.values.iter().next())
        .and_then(|value| value.decode_as::<OctetString>().ok());
    match stored {
        Some(stored) if stored.as_bytes() == content_digest => report.digest_matches = true,
        Some(_) => report
            .problems
            .push("The document’s bytes don’t match the digest in the signature: it was changed after signing.".into()),
        None => report.problems.push("The signature has no message digest.".into()),
    }
    match attributes.to_der() {
        Ok(der) => Some(der),
        Err(error) => {
            report
                .problems
                .push(format!("The signed attributes can’t be read: {error}."));
            None
        }
    }
}

fn certificates_of(signed_data: &SignedData) -> Vec<Certificate> {
    signed_data
        .certificates
        .iter()
        .flat_map(|set| set.0.iter())
        .filter_map(|choice| match choice {
            cms::cert::CertificateChoices::Certificate(certificate) => Some(certificate.clone()),
            _ => None,
        })
        .collect()
}

fn find_signer_certificate<'a>(
    certificates: &'a [Certificate],
    sid: &SignerIdentifier,
) -> Option<&'a Certificate> {
    certificates.iter().find(|certificate| {
        let tbs = certificate.tbs_certificate();
        match sid {
            SignerIdentifier::IssuerAndSerialNumber(id) => {
                tbs.issuer() == &id.issuer && tbs.serial_number() == &id.serial_number
            }
            SignerIdentifier::SubjectKeyIdentifier(key_id) => tbs
                .get_extension::<x509_cert::ext::pkix::SubjectKeyIdentifier>()
                .ok()
                .flatten()
                .map(|(_, own)| own == *key_id)
                .unwrap_or(false),
        }
    })
}

/// checks a signature over `message` with a public key, for rsa (pkcs#1
/// v1.5 and pss) and ecdsa p-256.
pub(crate) fn verify_signature(
    key: &SubjectPublicKeyInfoOwned,
    algorithm: &AlgorithmIdentifierOwned,
    digest_algorithm: &der::asn1::ObjectIdentifier,
    message: &[u8],
    signature: &[u8],
) -> Result<(), String> {
    let wrong = || "The signature doesn’t match the signer’s certificate.".to_owned();
    let unsupported = || format!("The signature algorithm {} isn’t supported.", algorithm.oid);
    let key_bytes = key.subject_public_key.raw_bytes();
    match algorithm.oid {
        oids::RSA_ENCRYPTION
        | oids::SHA256_WITH_RSA
        | oids::SHA384_WITH_RSA
        | oids::SHA512_WITH_RSA => {
            let public = RsaPublicKey::from_pkcs1_der(key_bytes)
                .map_err(|_| "The certificate’s RSA key can’t be read.".to_owned())?;
            let hash_oid = match algorithm.oid {
                oids::SHA256_WITH_RSA => oids::SHA256,
                oids::SHA384_WITH_RSA => oids::SHA384,
                oids::SHA512_WITH_RSA => oids::SHA512,
                _ => *digest_algorithm,
            };
            let hashed = digest(&hash_oid, &[message]).ok_or_else(unsupported)?;
            let scheme = match hash_oid {
                oids::SHA256 => Pkcs1v15Sign::new::<Sha256>(),
                oids::SHA384 => Pkcs1v15Sign::new::<Sha384>(),
                _ => Pkcs1v15Sign::new::<Sha512>(),
            };
            public
                .verify(scheme, &hashed, signature)
                .map_err(|_| wrong())
        }
        oids::RSASSA_PSS => {
            let public = RsaPublicKey::from_pkcs1_der(key_bytes)
                .map_err(|_| "The certificate’s RSA key can’t be read.".to_owned())?;
            let parameters = algorithm
                .parameters
                .as_ref()
                .and_then(|any| any.decode_as::<PssParameters>().ok())
                .ok_or_else(unsupported)?;
            let hash_oid = parameters
                .hash
                .map(|hash| hash.oid)
                .ok_or_else(unsupported)?;
            let salt = parameters.salt_length.unwrap_or(20) as usize;
            let hashed = digest(&hash_oid, &[message]).ok_or_else(unsupported)?;
            let result = match hash_oid {
                oids::SHA256 => public.verify(
                    rsa::pss::Pss::<Sha256>::new_with_salt(salt),
                    &hashed,
                    signature,
                ),
                oids::SHA384 => public.verify(
                    rsa::pss::Pss::<Sha384>::new_with_salt(salt),
                    &hashed,
                    signature,
                ),
                _ => public.verify(
                    rsa::pss::Pss::<Sha512>::new_with_salt(salt),
                    &hashed,
                    signature,
                ),
            };
            result.map_err(|_| wrong())
        }
        oids::ECDSA_WITH_SHA256 | oids::ECDSA_WITH_SHA384 | oids::ECDSA_WITH_SHA512 => {
            let curve = key
                .algorithm
                .parameters
                .as_ref()
                .and_then(|any| any.decode_as::<der::asn1::ObjectIdentifier>().ok());
            if curve != Some(oids::P256) {
                return Err("Only ECDSA keys on the P-256 curve are supported.".into());
            }
            let public = p256::ecdsa::VerifyingKey::from_sec1_bytes(key_bytes)
                .map_err(|_| "The certificate’s ECDSA key can’t be read.".to_owned())?;
            let hash_oid = match algorithm.oid {
                oids::ECDSA_WITH_SHA384 => oids::SHA384,
                oids::ECDSA_WITH_SHA512 => oids::SHA512,
                _ => oids::SHA256,
            };
            let hashed = digest(&hash_oid, &[message]).ok_or_else(unsupported)?;
            let parsed = p256::ecdsa::Signature::from_der(signature).map_err(|_| wrong())?;
            public.verify_prehash(&hashed, &parsed).map_err(|_| wrong())
        }
        _ => Err(unsupported()),
    }
}

fn signing_time(signer_info: &SignerInfo) -> Option<SystemTime> {
    let attributes = signer_info.signed_attrs.as_ref()?;
    let value = attributes
        .iter()
        .find(|attribute| attribute.oid == oids::SIGNING_TIME)?
        .values
        .iter()
        .next()?;
    let time = x509_cert::time::Time::from_der(&value.to_der().ok()?).ok()?;
    Some(time.to_system_time())
}

fn summarise_certificate(
    certificate: &Certificate,
    all: &[Certificate],
    signing_time: Option<SystemTime>,
) -> CertificateSummary {
    let tbs = certificate.tbs_certificate();
    let validity = tbs.validity();
    let (from, until) = (
        validity.not_before.to_system_time(),
        validity.not_after.to_system_time(),
    );
    let self_signed = tbs.subject() == tbs.issuer() && signs_itself(certificate);
    CertificateSummary {
        subject: tbs.subject().to_string(),
        issuer: tbs.issuer().to_string(),
        common_name: common_name(&tbs.subject().to_string()),
        self_signed,
        valid_from: readable_date(from),
        valid_until: readable_date(until),
        valid_when_signed: signing_time.map(|time| time >= from && time <= until),
        chain: all
            .iter()
            .filter(|other| *other != certificate)
            .map(|other| other.tbs_certificate().subject().to_string())
            .collect(),
    }
}

/// whether a certificate's own key verifies its signature.
fn signs_itself(certificate: &Certificate) -> bool {
    let Ok(tbs) = certificate.tbs_certificate().to_der() else {
        return false;
    };
    let key = certificate.tbs_certificate().subject_public_key_info();
    let algorithm = certificate.signature_algorithm();
    let Some(signature) = certificate.signature().as_bytes() else {
        return false;
    };
    verify_signature(key, algorithm, &oids::SHA256, &tbs, signature).is_ok()
}

/// a pdf date like D:20261111120506Z (or with +01'00') made readable.
fn readable_pdf_date(date: &str) -> String {
    let digits: String = date.trim_start_matches("D:").chars().take(14).collect();
    if digits.len() < 12 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return date.to_owned();
    }
    let rest = &date.trim_start_matches("D:")[digits.len().min(date.len() - 2)..];
    let zone = if rest.starts_with('Z') || rest.is_empty() {
        "UTC".to_owned()
    } else {
        rest.replace('\'', "")
    };
    format!(
        "{}-{}-{} {}:{} {zone}",
        &digits[0..4],
        &digits[4..6],
        &digits[6..8],
        &digits[8..10],
        &digits[10..12]
    )
}

/// "3 March 2026": the kind of date people say, for the summary sentence.
fn spoken_date(time: SystemTime) -> String {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let (year, month, day, _, _, _) = super::sign::utc_parts(time);
    format!("{day} {} {year}", MONTHS[(month as usize).clamp(1, 12) - 1])
}

/// the plain sentences the ui shows for one signature, such as "Signed by
/// Ada Lovelace on 3 March 2026. The document was changed after signing."
fn summary(report: &SignatureReport, signing_time: Option<SystemTime>) -> String {
    let who = report
        .signer
        .clone()
        .or(report.name.clone())
        .unwrap_or_else(|| "an unknown signer".into());
    let when = signing_time
        .map(spoken_date)
        .or_else(|| report.signed_at.clone())
        .map(|time| format!(" on {time}"))
        .unwrap_or_default();
    let state = if !report.digest_matches
        && (report.signer.is_some() || report.changed_after_signing)
    {
        "The document was changed after signing, so this signature no longer covers it.".to_owned()
    } else if !report.valid {
        "This signature couldn’t be confirmed.".to_owned()
    } else if report.changed_after_signing && report.later_updates_are_signatures {
        "The signed part is unchanged; later additions are further signatures.".to_owned()
    } else if report.changed_after_signing {
        "The signed part is unchanged, but the document was changed after signing.".to_owned()
    } else {
        "The document hasn’t changed since it was signed.".to_owned()
    };
    let trust = match report.trust {
        TrustStatus::SelfSigned => format!(
            "The certificate is self-signed, so Letters can’t confirm that {who} is who they say."
        ),
        TrustStatus::NotChecked => {
            "Letters doesn’t check who issued the certificate or whether it was revoked.".to_owned()
        }
    };
    format!("Signed by {who}{when}. {state} {trust}")
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_use_spoken_dates() {
        let time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_772_496_000);
        assert_eq!(spoken_date(time), "3 March 2026");
    }

    #[test]
    fn reads_pdf_dates() {
        assert_eq!(
            readable_pdf_date("D:20261111120506Z"),
            "2026-11-11 12:05 UTC"
        );
        assert_eq!(
            readable_pdf_date("D:20260102030405+01'00'"),
            "2026-01-02 03:04 +0100"
        );
    }

    #[test]
    fn finds_revision_ends() {
        assert_eq!(revision_ends(b"abc%%EOF\nmore%%EOF\r\n"), vec![9, 20]);
    }

    #[test]
    fn common_names_come_out_of_subjects() {
        assert_eq!(
            common_name("CN=Ada Lovelace,OU=Self-signed"),
            Some("Ada Lovelace".into())
        );
    }
}
