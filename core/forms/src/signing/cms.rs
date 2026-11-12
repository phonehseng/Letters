//! the signature itself: a cms "SignedData" structure (rfc 5652).
//!
//! it holds the signer's certificate (and any chain the certificate file
//! had), a few signed attributes, and the signature over those attributes.
//! one of the attributes is the sha-256 digest of the pdf's bytes, which
//! is how the signature covers the document.

use std::time::SystemTime;

use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
use cms::content_info::{CmsVersion, ContentInfo};
use cms::signed_data::{
    CertificateSet, EncapsulatedContentInfo, SignedData, SignerIdentifier, SignerInfo, SignerInfos,
};
use der::asn1::{OctetString, SetOfVec};
use der::{Any, Decode, Encode};
use sha2::{Digest, Sha256, Sha384, Sha512};
use spki::AlgorithmIdentifierOwned;
use x509_cert::attr::Attribute;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::Time;
use x509_cert::Certificate;

use super::keys::Signer;
use super::oids;
use crate::error::{FormError, FormResult};

/// ess signing-certificate-v2 (rfc 5035): names the exact certificate used,
/// so it can't be swapped for another one with the same key.
#[derive(Clone, Debug, der::Sequence)]
struct SigningCertificateV2 {
    certs: Vec<EssCertIdV2>,
}

/// the hash algorithm is left out because sha-256 is its default.
#[derive(Clone, Debug, der::Sequence)]
struct EssCertIdV2 {
    cert_hash: OctetString,
    issuer_serial: IssuerSerial,
}

#[derive(Clone, Debug, der::Sequence)]
struct IssuerSerial {
    issuer: Vec<GeneralName>,
    serial_number: SerialNumber,
}

fn failed(error: der::Error) -> FormError {
    FormError::SigningFailed(error.to_string())
}

fn sha256_algorithm() -> AlgorithmIdentifierOwned {
    AlgorithmIdentifierOwned {
        oid: oids::SHA256,
        parameters: None,
    }
}

fn attribute(oid: der::asn1::ObjectIdentifier, value: Any) -> FormResult<Attribute> {
    let mut values = SetOfVec::new();
    values.insert(value).map_err(failed)?;
    Ok(Attribute { oid, values })
}

/// the four signed attributes: what is signed (plain data), its digest,
/// when, and which certificate.
fn signed_attributes(
    signer: &Signer,
    content_digest: &[u8],
    signing_time: SystemTime,
) -> FormResult<SetOfVec<Attribute>> {
    let time = Time::try_from(signing_time).map_err(failed)?;
    let tbs = signer.certificate.tbs_certificate();
    let signing_certificate = SigningCertificateV2 {
        certs: vec![EssCertIdV2 {
            cert_hash: OctetString::new(Sha256::digest(&signer.certificate_der).to_vec())
                .map_err(failed)?,
            issuer_serial: IssuerSerial {
                issuer: vec![GeneralName::DirectoryName(tbs.issuer().clone())],
                serial_number: tbs.serial_number().clone(),
            },
        }],
    };
    let attributes = vec![
        attribute(
            oids::CONTENT_TYPE,
            Any::encode_from(&oids::DATA).map_err(failed)?,
        )?,
        attribute(oids::SIGNING_TIME, Any::encode_from(&time).map_err(failed)?)?,
        attribute(
            oids::MESSAGE_DIGEST,
            Any::encode_from(&OctetString::new(content_digest.to_vec()).map_err(failed)?)
                .map_err(failed)?,
        )?,
        attribute(
            oids::SIGNING_CERTIFICATE_V2,
            Any::encode_from(&signing_certificate).map_err(failed)?,
        )?,
    ];
    // a set must be stored sorted; try_from sorts it
    SetOfVec::try_from(attributes).map_err(failed)
}

/// builds the whole signature for a document whose signed bytes have
/// `content_digest` as their sha-256 digest.
pub fn build_signed_data(
    signer: &Signer,
    content_digest: &[u8],
    signing_time: SystemTime,
) -> FormResult<Vec<u8>> {
    let attributes = signed_attributes(signer, content_digest, signing_time)?;
    // the signature covers the attributes encoded as a SET (tag 0x31)
    let signature = signer.key.sign(&attributes.to_der().map_err(failed)?)?;
    let tbs = signer.certificate.tbs_certificate();
    let signer_info = SignerInfo {
        version: CmsVersion::V1,
        sid: SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
            issuer: tbs.issuer().clone(),
            serial_number: tbs.serial_number().clone(),
        }),
        digest_alg: sha256_algorithm(),
        signed_attrs: Some(attributes),
        signature_algorithm: signer.key.algorithm()?,
        signature: OctetString::new(signature).map_err(failed)?,
        unsigned_attrs: None,
    };

    let mut certificates = vec![CertificateChoices::Certificate(signer.certificate.clone())];
    for der_bytes in &signer.chain_der {
        let certificate = Certificate::from_der(der_bytes).map_err(failed)?;
        certificates.push(CertificateChoices::Certificate(certificate));
    }
    let mut digest_algorithms = SetOfVec::new();
    digest_algorithms
        .insert(sha256_algorithm())
        .map_err(failed)?;
    let mut signer_infos = SetOfVec::new();
    signer_infos.insert(signer_info).map_err(failed)?;

    let signed_data = SignedData {
        version: CmsVersion::V1,
        digest_algorithms,
        encap_content_info: EncapsulatedContentInfo {
            econtent_type: oids::DATA,
            econtent: None,
        },
        certificates: Some(CertificateSet(
            SetOfVec::try_from(certificates).map_err(failed)?,
        )),
        crls: None,
        signer_infos: SignerInfos(signer_infos),
    };
    let content_info = ContentInfo {
        content_type: oids::SIGNED_DATA,
        content: Any::encode_from(&signed_data).map_err(failed)?,
    };
    content_info.to_der().map_err(failed)
}

/// reads a signature from a pdf's /Contents, which is padded with zeros
/// after the real structure.
pub fn parse_signed_data(contents: &[u8]) -> Result<SignedData, String> {
    let exact = trim_to_der_length(contents)
        .ok_or("the signature isn't stored in a form Letters can read")?;
    let info = ContentInfo::from_der(exact).map_err(|error| error.to_string())?;
    if info.content_type != oids::SIGNED_DATA {
        return Err("the signature isn't a cms signed-data structure".into());
    }
    info.content
        .decode_as::<SignedData>()
        .map_err(|error| error.to_string())
}

/// the bytes of the first der value, using the length in its header.
pub(crate) fn trim_to_der_length(bytes: &[u8]) -> Option<&[u8]> {
    let first_length = *bytes.get(1)?;
    let (header, length) = if first_length < 0x80 {
        (2, first_length as usize)
    } else {
        let count = (first_length & 0x7F) as usize;
        if count == 0 || count > 4 {
            return None;
        }
        let length = bytes
            .get(2..2 + count)?
            .iter()
            .fold(0usize, |total, byte| total << 8 | *byte as usize);
        (2 + count, length)
    };
    bytes.get(..header + length)
}

/// the digest of some byte slices, for the digest algorithms letters reads.
pub fn digest(algorithm: &der::asn1::ObjectIdentifier, parts: &[&[u8]]) -> Option<Vec<u8>> {
    fn run<D: Digest>(parts: &[&[u8]]) -> Vec<u8> {
        let mut hasher = D::new();
        for part in parts {
            hasher.update(part);
        }
        hasher.finalize().to_vec()
    }
    match *algorithm {
        oids::SHA256 => Some(run::<Sha256>(parts)),
        oids::SHA384 => Some(run::<Sha384>(parts)),
        oids::SHA512 => Some(run::<Sha512>(parts)),
        _ => None,
    }
}

/// adds an unsigned attribute (such as a signature time stamp) to the
/// first signer of an encoded signature. unsigned attributes are outside
/// what the signer signed, so the signature stays valid.
pub fn add_unsigned_attribute(
    cms_der: &[u8],
    oid: der::asn1::ObjectIdentifier,
    value_der: &[u8],
) -> FormResult<Vec<u8>> {
    let info = ContentInfo::from_der(cms_der).map_err(failed)?;
    let mut signed: SignedData = info.content.decode_as().map_err(failed)?;
    let mut signers: Vec<SignerInfo> = signed.signer_infos.0.into_vec();
    let first = signers
        .first_mut()
        .ok_or_else(|| FormError::SigningFailed("the signature has no signer".into()))?;
    let value = Any::from_der(value_der).map_err(failed)?;
    let mut attributes: Vec<Attribute> = first
        .unsigned_attrs
        .take()
        .map(SetOfVec::into_vec)
        .unwrap_or_default();
    attributes.push(attribute(oid, value)?);
    first.unsigned_attrs = Some(SetOfVec::try_from(attributes).map_err(failed)?);
    signed.signer_infos = SignerInfos(SetOfVec::try_from(signers).map_err(failed)?);
    let content_info = ContentInfo {
        content_type: oids::SIGNED_DATA,
        content: Any::encode_from(&signed).map_err(failed)?,
    };
    content_info.to_der().map_err(failed)
}
