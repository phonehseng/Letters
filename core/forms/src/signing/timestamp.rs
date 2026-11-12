//! trusted time from a time stamp authority (rfc 3161) the person chose:
//! asking for a token over a hash, and checking a token's hash, signature
//! and certificate. no authority is built in; nothing is sent unless the
//! person configured one and asked for a time stamp.

use std::io::Read;
use std::time::{Duration, SystemTime};

use cms::content_info::ContentInfo;
use cms::signed_data::SignedData;
use der::asn1::{Int, OctetString};
use der::{Decode, Encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spki::AlgorithmIdentifierOwned;
use x509_cert::Certificate;

use super::algorithms::verify_signature;
use super::cms::digest;
use super::oids;
use super::tsp::{generalized_time_seconds, parse_response, MessageImprint, TimeStampReq, TstInfo};
use super::verify::{certificates_of, find_signer_certificate};
use crate::error::{FormError, FormResult};

/// said in the interface before anything is sent, word for word.
pub const TIMESTAMP_DISCLOSURE: &str = "To add a trusted time, Letters sends the time stamp server you chose a fingerprint (a SHA-256 hash) of the signature, or of the document for a document time stamp. The document itself never leaves this computer, and a fingerprint can’t be turned back into it.";

/// said about a signature whose time nobody else vouches for.
pub const CLOCK_TIME_NOTE: &str = "The time comes from the signer’s computer clock.";

/// a time stamp authority the person configured. Letters has none built in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimestampAuthority {
    /// such as https://tsa.example.org/tsr
    pub url: String,
    /// for authorities that ask for a name and password (http basic auth)
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// what a time stamp token says, and whether it checks out.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimestampInfo {
    /// the time the authority vouches for, readable (utc)
    pub time: Option<String>,
    /// the authority's certificate subject
    pub authority: Option<String>,
    pub serial_number: Option<String>,
    pub policy: Option<String>,
    /// the token's hash matches what it should cover
    pub imprint_matches: bool,
    /// the authority's signature over the token is correct
    pub signature_correct: bool,
    /// the authority's certificate is marked for time stamping
    pub certificate_for_timestamping: bool,
    pub problems: Vec<String>,
}

impl TimestampInfo {
    pub fn is_valid(&self) -> bool {
        self.imprint_matches && self.signature_correct && self.certificate_for_timestamping
    }
}

/// a token checked against the bytes it should cover. `moment` is the
/// time as a SystemTime, for comparisons.
pub(crate) struct CheckedToken {
    pub info: TimestampInfo,
    pub moment: Option<SystemTime>,
}

/// how long to wait for the authority before giving up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);
/// a token with its certificates is a few kilobytes; more is not an answer.
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

/// asks the authority for a token over the sha-256 of `parts` and checks
/// the answer before returning the token (a ContentInfo, der encoded).
pub(crate) fn fetch_token(authority: &TimestampAuthority, parts: &[&[u8]]) -> FormResult<Vec<u8>> {
    let failed = |why: String| FormError::TimestampFailed(why);
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    let hash = hasher.finalize().to_vec();
    let nonce = random_nonce();
    let request = build_request(&hash, &nonce).map_err(|error| failed(error.to_string()))?;
    let answer = post(authority, &request)?;
    let response = parse_response(&answer)
        .map_err(|why| failed(format!("The server’s answer can’t be read ({why}).")))?;
    // 0 granted, 1 granted with modifications (rfc 3161 section 2.4.2)
    if response.status > 1 {
        let detail = response.status_text.unwrap_or_default();
        return Err(failed(format!(
            "The time stamp server refused the request. {detail}"
        )));
    }
    let token = response
        .token
        .ok_or_else(|| failed("The time stamp server answered without a time stamp.".into()))?;
    let checked = check_token(&token, parts);
    if !checked.info.is_valid() {
        let problems = checked.info.problems.join(" ");
        return Err(failed(format!(
            "The time stamp the server sent doesn’t check out. {problems}"
        )));
    }
    if token_nonce(&token).as_deref() != Some(nonce.as_slice()) {
        return Err(failed(
            "The time stamp server’s answer is for a different request.".into(),
        ));
    }
    Ok(token)
}

/// eight random bytes, the top bit cleared so the integer is positive.
fn random_nonce() -> Vec<u8> {
    let mut bytes: [u8; 8] = rand::random();
    bytes[0] &= 0x7F;
    bytes[0] |= 0x01;
    bytes.to_vec()
}

fn build_request(hash: &[u8], nonce: &[u8]) -> der::Result<Vec<u8>> {
    TimeStampReq {
        version: 1,
        message_imprint: MessageImprint {
            hash_algorithm: AlgorithmIdentifierOwned {
                oid: oids::SHA256,
                parameters: None,
            },
            hashed_message: OctetString::new(hash.to_vec())?,
        },
        req_policy: None,
        nonce: Some(Int::new(nonce)?),
        // the token must carry the authority's certificate to be checkable later
        cert_req: Some(true),
    }
    .to_der()
}

/// sends the request over http(s) as rfc 3161 section 3.4 describes.
fn post(authority: &TimestampAuthority, request: &[u8]) -> FormResult<Vec<u8>> {
    let host = host_of(&authority.url);
    let unreachable = |why: String| {
        FormError::TimestampFailed(format!(
            "The time stamp server at {host} couldn’t be reached ({why})."
        ))
    };
    if !(authority.url.starts_with("https://") || authority.url.starts_with("http://")) {
        return Err(FormError::TimestampFailed(
            "The time stamp server’s address must start with https:// or http://.".into(),
        ));
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(ANSWER_TIMEOUT))
        .user_agent("Letters")
        .build()
        .into();
    let mut call = agent
        .post(&authority.url)
        .header("Content-Type", "application/timestamp-query");
    if let Some(username) = authority
        .username
        .as_deref()
        .filter(|name| !name.is_empty())
    {
        let secret = format!("{username}:{}", authority.password.as_deref().unwrap_or(""));
        call = call.header(
            "Authorization",
            format!("Basic {}", base64(secret.as_bytes())),
        );
    }
    let mut response = call.send(request).map_err(|error| match error {
        ureq::Error::StatusCode(401 | 403) => FormError::TimestampFailed(format!(
            "The time stamp server at {host} didn’t accept the name and password."
        )),
        other => unreachable(other.to_string()),
    })?;
    let mut answer = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_RESPONSE_BYTES)
        .read_to_end(&mut answer)
        .map_err(|error| unreachable(error.to_string()))?;
    Ok(answer)
}

fn host_of(url: &str) -> String {
    let without_scheme = url.split("://").nth(1).unwrap_or(url);
    without_scheme
        .split('/')
        .next()
        .unwrap_or(without_scheme)
        .to_owned()
}

/// standard base64 (rfc 4648 section 4), for the basic auth header.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let values = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let joined = u32::from_be_bytes([0, values[0], values[1], values[2]]);
        for position in 0..4 {
            if position <= chunk.len() {
                let index = (joined >> (18 - 6 * position)) & 0x3F;
                out.push(ALPHABET[index as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// the signed data of a token, if it is one.
fn token_signed_data(token: &[u8]) -> Result<SignedData, String> {
    let info = ContentInfo::from_der(token).map_err(|error| error.to_string())?;
    if info.content_type != oids::SIGNED_DATA {
        return Err("it isn’t a signed time stamp".into());
    }
    info.content
        .decode_as::<SignedData>()
        .map_err(|error| error.to_string())
}

/// the encoded TSTInfo inside a token's signed data.
fn tst_info_bytes(signed: &SignedData) -> Result<Vec<u8>, String> {
    if signed.encap_content_info.econtent_type != oids::TST_INFO {
        return Err("it doesn’t hold time stamp information".into());
    }
    let content = signed
        .encap_content_info
        .econtent
        .as_ref()
        .ok_or("it holds no time stamp information")?;
    Ok(content.value().to_vec())
}

fn token_nonce(token: &[u8]) -> Option<Vec<u8>> {
    let signed = token_signed_data(token).ok()?;
    let info = TstInfo::from_der(&tst_info_bytes(&signed).ok()?).ok()?;
    info.nonce.map(|nonce| nonce.as_bytes().to_vec())
}

/// checks a token against the bytes it should cover: its hash, the
/// authority's signature and that the certificate is for time stamping.
pub(crate) fn check_token(token: &[u8], covered: &[&[u8]]) -> CheckedToken {
    let mut info = TimestampInfo {
        time: None,
        authority: None,
        serial_number: None,
        policy: None,
        imprint_matches: false,
        signature_correct: false,
        certificate_for_timestamping: false,
        problems: Vec::new(),
    };
    let moment = match read_and_check(token, covered, &mut info) {
        Ok(moment) => moment,
        Err(problem) => {
            info.problems
                .push(format!("The time stamp can’t be read: {problem}."));
            None
        }
    };
    CheckedToken { info, moment }
}

fn read_and_check(
    token: &[u8],
    covered: &[&[u8]],
    info: &mut TimestampInfo,
) -> Result<Option<SystemTime>, String> {
    let signed = token_signed_data(token)?;
    let content = tst_info_bytes(&signed)?;
    let tst = TstInfo::from_der(&content).map_err(|error| error.to_string())?;
    info.policy = Some(tst.policy.to_string());
    info.serial_number = Some(hex(tst.serial_number.as_bytes()));
    let moment = generalized_time_seconds(&tst.gen_time)
        .map(|seconds| SystemTime::UNIX_EPOCH + Duration::from_secs(seconds.max(0) as u64));
    info.time = moment.map(super::sign::readable_date);

    let imprint = &tst.message_imprint;
    match digest(&imprint.hash_algorithm.oid, covered) {
        Some(expected) if expected == imprint.hashed_message.as_bytes() => {
            info.imprint_matches = true
        }
        Some(_) => info
            .problems
            .push("The time stamp is for different bytes: they changed after it was made.".into()),
        None => info
            .problems
            .push("The time stamp uses a hash Letters can’t check.".into()),
    }
    check_authority_signature(&signed, &content, info);
    Ok(moment)
}

/// the authority's signature over the time stamp information.
fn check_authority_signature(signed: &SignedData, content: &[u8], info: &mut TimestampInfo) {
    let Some(signer) = signed.signer_infos.0.iter().next() else {
        info.problems
            .push("The time stamp names no authority.".into());
        return;
    };
    let certificates = certificates_of(signed);
    let Some(certificate) = find_signer_certificate(&certificates, &signer.sid) else {
        info.problems
            .push("The time stamp doesn’t include the authority’s certificate.".into());
        return;
    };
    info.authority = Some(certificate.tbs_certificate().subject().to_string());
    info.certificate_for_timestamping = is_for_timestamping(certificate);
    if !info.certificate_for_timestamping {
        info.problems
            .push("The authority’s certificate isn’t marked for time stamping.".into());
    }
    let Some(attributes) = &signer.signed_attrs else {
        info.problems
            .push("The time stamp has no signed attributes.".into());
        return;
    };
    if !names_its_certificate(attributes, certificate) {
        info.problems.push(
            "The time stamp doesn’t name the authority’s certificate, as time stamps must (rfc 3161 2.4.2).".into(),
        );
        return;
    }
    let stored = attributes
        .iter()
        .find(|attribute| attribute.oid == oids::MESSAGE_DIGEST)
        .and_then(|attribute| attribute.values.iter().next())
        .and_then(|value| value.decode_as::<OctetString>().ok());
    let computed = digest(&signer.digest_alg.oid, &[content]);
    if stored.as_ref().map(OctetString::as_bytes) != computed.as_deref() {
        info.problems
            .push("The time stamp information was changed after the authority signed it.".into());
        return;
    }
    let Ok(message) = attributes.to_der() else {
        return;
    };
    let key = certificate.tbs_certificate().subject_public_key_info();
    let signature = signer.signature.as_bytes();
    match verify_signature(
        key,
        &signer.signature_algorithm,
        &signer.digest_alg.oid,
        &message,
        signature,
    ) {
        Ok(()) => info.signature_correct = true,
        Err(problem) => info.problems.push(problem),
    }
}

/// rfc 3161 section 2.4.2 requires the ess signing certificate attribute
/// (rfc 2634, or its sha-2 version rfc 5035), so a token can't be passed
/// off as signed by another certificate with the same key. for the sha-2
/// version the certificate's sha-256 must appear in it; the older sha-1
/// version is only checked for being there, since letters has no sha-1.
fn names_its_certificate(
    attributes: &x509_cert::attr::Attributes,
    certificate: &Certificate,
) -> bool {
    if let Some(version_two) = attributes
        .iter()
        .find(|a| a.oid == oids::SIGNING_CERTIFICATE_V2)
    {
        let Ok(der) = certificate.to_der() else {
            return false;
        };
        let hash = Sha256::digest(&der);
        let encoded = version_two
            .values
            .iter()
            .next()
            .and_then(|value| value.to_der().ok());
        return encoded.is_some_and(|bytes| {
            bytes
                .windows(hash.len())
                .any(|window| window == hash.as_slice())
        });
    }
    attributes
        .iter()
        .any(|attribute| attribute.oid == oids::SIGNING_CERTIFICATE)
}

/// rfc 3161 section 2.3: the authority's certificate must carry the
/// time stamping extended key usage.
fn is_for_timestamping(certificate: &Certificate) -> bool {
    certificate
        .tbs_certificate()
        .get_extension::<x509_cert::ext::pkix::ExtendedKeyUsage>()
        .ok()
        .flatten()
        .is_some_and(|(_, usage)| usage.0.contains(&oids::TIME_STAMPING))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

/// the hash a signature time stamp covers: the signer's signature value
/// (rfc 3161 appendix a).
pub(crate) fn signature_value_of(cms_der: &[u8]) -> Option<Vec<u8>> {
    let info = ContentInfo::from_der(cms_der).ok()?;
    let signed = info.content.decode_as::<SignedData>().ok()?;
    let signer = signed.signer_infos.0.iter().next()?;
    Some(signer.signature.as_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_rfc_examples() {
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(
            base64(b"Aladdin:open sesame"),
            "QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
    }

    #[test]
    fn hosts_are_shown_without_paths() {
        assert_eq!(host_of("https://tsa.example.org/tsr"), "tsa.example.org");
        assert_eq!(host_of("http://127.0.0.1:8080"), "127.0.0.1:8080");
    }
}
