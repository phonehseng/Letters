//! making a self-signed certificate for someone who has none.
//!
//! a self-signed certificate is one a person makes for themselves. it
//! lets them sign documents, and anyone can check the document wasn't
//! changed afterwards, but nobody vouches for the name in it: anyone can
//! make one with any name. letters says so wherever it shows one.

use std::str::FromStr;
use std::time::Duration;

use der::asn1::{Ia5String, OctetString};
use der::Encode;
use p12_keystore::{KeyStore, KeyStoreEntry, PrivateKeyChain};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spki::SubjectPublicKeyInfoOwned;
use x509_cert::builder::profile::BuilderProfile;
use x509_cert::builder::{Builder, CertificateBuilder};
use x509_cert::certificate::TbsCertificate;
use x509_cert::ext::pkix::name::GeneralName;
use x509_cert::ext::pkix::{
    BasicConstraints, ExtendedKeyUsage, KeyUsage, KeyUsages, SubjectAltName,
};
use x509_cert::ext::Extension;
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::Validity;

use super::oids;
use super::sign::readable_date;
use crate::error::{FormError, FormResult};

/// the kind of key to make.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyKind {
    /// rsa with 2048 bits: understood by every pdf reader
    #[default]
    Rsa2048,
    /// ecdsa on p-256: smaller and faster, understood by current readers
    P256,
}

/// who the certificate is for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificateRequest {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub organization: Option<String>,
    /// how many days it can be used for
    pub valid_days: u32,
    /// the password that will protect the certificate file
    pub password: String,
    #[serde(default)]
    pub key_kind: KeyKind,
}

/// the new certificate file and what is in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedCertificate {
    /// a .p12 file holding the private key and the certificate
    pub pkcs12: Vec<u8>,
    pub subject: String,
    pub valid_from: String,
    pub valid_until: String,
    /// always true: nobody else vouches for this certificate
    pub self_signed: bool,
    /// a sentence for the ui explaining what self-signed means
    pub note: String,
}

const SELF_SIGNED_NOTE: &str = "This certificate is self-signed: you made it yourself, so it proves a document hasn’t changed since you signed it, but it doesn’t prove to others who you are. Keep the file and its password safe; anyone who has both can sign as you.";

/// the unit "OU" written into every certificate letters makes, so any pdf
/// reader showing the certificate shows that it is self-signed.
const SELF_SIGNED_UNIT: &str = "Self-signed";

/// makes a key and a self-signed certificate, packed as a password
/// protected pkcs#12 file. made entirely on this computer.
pub fn create_self_signed_certificate(
    request: &CertificateRequest,
) -> FormResult<CreatedCertificate> {
    let failed = |error: String| FormError::CertificateFailed(error);
    if request.name.trim().is_empty() {
        return Err(failed("a name is needed".into()));
    }
    if request.valid_days == 0 || request.valid_days > 36_500 {
        return Err(failed(
            "it must be valid for between 1 day and 100 years".into(),
        ));
    }
    let subject = subject_name(request)?;
    let profile = SelfSignedProfile {
        subject: subject.clone(),
        email: request
            .email
            .clone()
            .filter(|email| !email.trim().is_empty()),
    };
    let validity = Validity::from_now(Duration::from_secs(request.valid_days as u64 * 86_400))
        .map_err(|error| failed(error.to_string()))?;
    let serial = SerialNumber::generate(&mut rand::rng());

    let (certificate_der, pkcs8) = match request.key_kind {
        KeyKind::Rsa2048 => {
            use rsa::pkcs8::EncodePrivateKey;
            let key = rsa::PrivateKey::new(&mut rand::rng(), 2048)
                .map_err(|error| failed(error.to_string()))?;
            let signer = rsa::pkcs1v15::SigningKey::<Sha256>::new(key.clone());
            let public = SubjectPublicKeyInfoOwned::from_key(&key.to_public_key())
                .map_err(|error| failed(error.to_string()))?;
            let builder = CertificateBuilder::new(profile, serial, validity, public)
                .map_err(|error| failed(error.to_string()))?;
            let certificate = builder
                .build::<_, rsa::pkcs1v15::Signature>(&signer)
                .map_err(|error| failed(error.to_string()))?;
            let pkcs8 = key
                .to_pkcs8_der()
                .map_err(|error| failed(error.to_string()))?;
            (
                certificate
                    .to_der()
                    .map_err(|error| failed(error.to_string()))?,
                pkcs8.as_bytes().to_vec(),
            )
        }
        KeyKind::P256 => {
            use p256::elliptic_curve::Generate;
            use p256::pkcs8::EncodePrivateKey;
            let key = p256::ecdsa::SigningKey::generate_from_rng(&mut rand::rng());
            let public = SubjectPublicKeyInfoOwned::from_key(key.verifying_key())
                .map_err(|error| failed(error.to_string()))?;
            let builder = CertificateBuilder::new(profile, serial, validity, public)
                .map_err(|error| failed(error.to_string()))?;
            let certificate = builder
                .build::<_, p256::ecdsa::DerSignature>(&key)
                .map_err(|error| failed(error.to_string()))?;
            let pkcs8 = key
                .to_pkcs8_der()
                .map_err(|error| failed(error.to_string()))?;
            (
                certificate
                    .to_der()
                    .map_err(|error| failed(error.to_string()))?,
                pkcs8.as_bytes().to_vec(),
            )
        }
    };
    let pkcs12 = pack_pkcs12(&certificate_der, &pkcs8, &request.password, &request.name)?;
    Ok(CreatedCertificate {
        pkcs12,
        subject: subject.to_string(),
        valid_from: readable_date(validity.not_before.to_system_time()),
        valid_until: readable_date(validity.not_after.to_system_time()),
        self_signed: true,
        note: SELF_SIGNED_NOTE.to_owned(),
    })
}

/// "CN=Ada Lovelace,OU=Self-signed,O=...", with the characters that have
/// a meaning in such names escaped (rfc 4514).
fn subject_name(request: &CertificateRequest) -> FormResult<Name> {
    let mut parts = vec![format!("CN={}", escape(request.name.trim()))];
    parts.push(format!("OU={SELF_SIGNED_UNIT}"));
    if let Some(organization) = request
        .organization
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        parts.push(format!("O={}", escape(organization.trim())));
    }
    Name::from_str(&parts.join(","))
        .map_err(|error| FormError::CertificateFailed(error.to_string()))
}

fn escape(value: &str) -> String {
    let mut out = String::new();
    for (index, character) in value.chars().enumerate() {
        let leading = index == 0 && (character == '#' || character == ' ');
        if leading || ",+\"\\<>;=".contains(character) {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

/// the settings of a self-signed signing certificate: the issuer is the
/// subject, it can't issue other certificates, and it is for signing.
struct SelfSignedProfile {
    subject: Name,
    email: Option<String>,
}

impl BuilderProfile for SelfSignedProfile {
    fn get_issuer(&self, subject: &Name) -> Name {
        subject.clone()
    }

    fn get_subject(&self) -> Name {
        self.subject.clone()
    }

    fn build_extensions(
        &self,
        public_key: spki::SubjectPublicKeyInfoRef<'_>,
        _issuer_key: spki::SubjectPublicKeyInfoRef<'_>,
        _tbs: &TbsCertificate,
    ) -> x509_cert::builder::Result<Vec<Extension>> {
        let mut extensions = vec![
            extension(
                oids::BASIC_CONSTRAINTS,
                true,
                &BasicConstraints {
                    ca: false,
                    path_len_constraint: None,
                },
            )?,
            extension(
                oids::KEY_USAGE,
                true,
                &KeyUsage(KeyUsages::DigitalSignature | KeyUsages::NonRepudiation),
            )?,
            extension(
                oids::EXTENDED_KEY_USAGE,
                false,
                &ExtendedKeyUsage(vec![oids::DOCUMENT_SIGNING, oids::EMAIL_PROTECTION]),
            )?,
        ];
        // a subject key identifier (the sha-256 of the key, cut to 20 bytes)
        let key_bytes = public_key.subject_public_key.raw_bytes();
        let key_id = OctetString::new(Sha256::digest(key_bytes)[..20].to_vec())?;
        extensions.push(extension(
            const_oid::ObjectIdentifier::new_unwrap("2.5.29.14"),
            false,
            &x509_cert::ext::pkix::SubjectKeyIdentifier(key_id),
        )?);
        if let Some(email) = &self.email {
            let address = Ia5String::new(email.trim())?;
            extensions.push(extension(
                oids::SUBJECT_ALT_NAME,
                false,
                &SubjectAltName(vec![GeneralName::Rfc822Name(address)]),
            )?);
        }
        Ok(extensions)
    }
}

fn extension(
    oid: const_oid::ObjectIdentifier,
    critical: bool,
    value: &impl Encode,
) -> x509_cert::builder::Result<Extension> {
    Ok(Extension {
        extn_id: oid,
        critical,
        extn_value: OctetString::new(value.to_der()?)?,
    })
}

/// packs the key and certificate into a pkcs#12 file, encrypted with
/// aes-256 and a key made from the password (the library's modern default).
fn pack_pkcs12(
    certificate: &[u8],
    pkcs8: &[u8],
    password: &str,
    alias: &str,
) -> FormResult<Vec<u8>> {
    let failed =
        |error: p12_keystore::error::Error| FormError::CertificateFailed(error.to_string());
    let key = p12_keystore::PrivateKey::from_der(pkcs8).map_err(failed)?;
    let certificate = p12_keystore::Certificate::from_der(certificate).map_err(failed)?;
    let key_id = Sha256::digest(certificate.as_der())[..20].to_vec();
    let mut store = KeyStore::new();
    store.add_entry(
        alias,
        KeyStoreEntry::PrivateKeyChain(PrivateKeyChain::new(key_id, key, vec![certificate])),
    );
    store.writer(password).write().map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_escaped() {
        assert_eq!(escape("Smith, Jane"), "Smith\\, Jane");
        assert_eq!(escape("#1"), "\\#1");
    }
}
