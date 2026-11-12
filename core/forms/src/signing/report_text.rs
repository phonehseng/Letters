//! the words of a signature report: the certificate summary, readable
//! dates and the summary sentences the interface shows.

use std::time::SystemTime;

use der::Encode;
use x509_cert::Certificate;

use super::keys::common_name;
use super::oids;
use super::sign::readable_date;

use super::algorithms::verify_signature;
use super::verify::{CertificateSummary, SignatureReport, TrustStatus};

pub(super) fn summarise_certificate(
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
pub(super) fn signs_itself(certificate: &Certificate) -> bool {
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
pub(super) fn readable_pdf_date(date: &str) -> String {
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
pub(super) fn spoken_date(time: SystemTime) -> String {
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
pub(super) fn summary(report: &SignatureReport, signing_time: Option<SystemTime>) -> String {
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
