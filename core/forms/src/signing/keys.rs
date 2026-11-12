//! opening a certificate file (pkcs#12, .p12 or .pfx) and signing with its key.

use der::Decode;
use p12_keystore::{KeyStore, Pkcs12ImportPolicy};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::{Pkcs1v15Sign, PrivateKey as RsaPrivateKey};
use sha2::{Digest, Sha256};
use x509_cert::Certificate;

use super::oids;
use crate::error::{FormError, FormResult};

/// the private key from a certificate file, ready to sign.
pub enum SigningKey {
    /// rsa with pkcs#1 v1.5 padding, the most common kind
    Rsa(RsaPrivateKey),
    /// rsa whose key is marked for pss padding only (id-RSASSA-PSS)
    RsaPss(RsaPrivateKey),
    /// ecdsa on the nist p-256 curve
    P256(p256::ecdsa::SigningKey),
}

/// a key with its certificate and the rest of its chain.
pub struct Signer {
    pub key: SigningKey,
    pub certificate: Certificate,
    pub certificate_der: Vec<u8>,
    /// the certificates that issued it, nearest first, as stored in the file
    pub chain_der: Vec<Vec<u8>>,
}

/// opens a pkcs#12 file with its password.
pub fn open_pkcs12(bytes: &[u8], password: &str) -> FormResult<Signer> {
    let store = KeyStore::from_pkcs12(bytes, password, Pkcs12ImportPolicy::Relaxed)
        .map_err(describe_pkcs12_error)?;
    let (_, chain) = store.private_key_chain().ok_or(FormError::NoPrivateKey)?;
    let certificates = chain.certs();
    let first = certificates
        .first()
        .ok_or_else(|| FormError::BadCertificate("it has a key but no certificate".into()))?;
    let certificate_der = first.as_der().to_vec();
    let certificate = Certificate::from_der(&certificate_der)
        .map_err(|error| FormError::BadCertificate(error.to_string()))?;
    let key = read_private_key(chain.key().as_der())?;
    Ok(Signer {
        key,
        certificate,
        certificate_der,
        chain_der: certificates[1..]
            .iter()
            .map(|cert| cert.as_der().to_vec())
            .collect(),
    })
}

/// a wrong password shows up as a failed integrity check (the mac) or as
/// garbage after decrypting, depending on how the file was made.
fn describe_pkcs12_error(error: p12_keystore::error::Error) -> FormError {
    use p12_keystore::error::Error;
    match error {
        Error::MacError(_) | Error::UnpadError | Error::InvalidPrivateKey => {
            FormError::WrongCertificatePassword
        }
        other => FormError::BadCertificate(other.to_string()),
    }
}

/// reads a pkcs#8 private key of a kind letters can sign with. the key's
/// algorithm says which kind it is; rsa keys marked for pss are signed
/// with pss, as the key asks.
pub fn read_private_key(pkcs8: &[u8]) -> FormResult<SigningKey> {
    let info =
        rsa::pkcs8::PrivateKeyInfoRef::try_from(pkcs8).map_err(|_| FormError::UnsupportedKey)?;
    let bad_key = |_| FormError::BadCertificate("the private key can't be read".into());
    match info.algorithm.oid {
        oids::RSA_ENCRYPTION => {
            let key =
                RsaPrivateKey::from_pkcs1_der(info.private_key.as_bytes()).map_err(bad_key)?;
            Ok(SigningKey::Rsa(key))
        }
        oids::RSASSA_PSS => {
            let key =
                RsaPrivateKey::from_pkcs1_der(info.private_key.as_bytes()).map_err(bad_key)?;
            Ok(SigningKey::RsaPss(key))
        }
        oids::EC_PUBLIC_KEY => {
            use p256::pkcs8::DecodePrivateKey;
            // the curve is checked here: keys on other curves are refused
            let key = p256::ecdsa::SigningKey::from_pkcs8_der(pkcs8)
                .map_err(|_| FormError::UnsupportedKey)?;
            Ok(SigningKey::P256(key))
        }
        _ => Err(FormError::UnsupportedKey),
    }
}

impl SigningKey {
    /// signs a message (the encoded signed attributes) with sha-256 and
    /// returns the signature bytes as cms stores them.
    pub fn sign(&self, message: &[u8]) -> FormResult<Vec<u8>> {
        let failed = |error: String| FormError::SigningFailed(error);
        let digest = Sha256::digest(message);
        match self {
            SigningKey::Rsa(key) => key
                .sign(Pkcs1v15Sign::new::<Sha256>(), &digest)
                .map_err(|error| failed(error.to_string())),
            SigningKey::RsaPss(key) => key
                .sign_with_rng(&mut rand::rng(), rsa::pss::Pss::<Sha256>::new(), &digest)
                .map_err(|error| failed(error.to_string())),
            SigningKey::P256(key) => {
                use p256::ecdsa::signature::Signer as _;
                let signature: p256::ecdsa::DerSignature = key.sign(message);
                Ok(signature.to_bytes().to_vec())
            }
        }
    }

    /// the signature algorithm as cms names it, with its parameters.
    pub fn algorithm(&self) -> FormResult<spki::AlgorithmIdentifierOwned> {
        let failed = |error: der::Error| FormError::SigningFailed(error.to_string());
        Ok(match self {
            SigningKey::Rsa(_) => spki::AlgorithmIdentifierOwned {
                oid: oids::SHA256_WITH_RSA,
                parameters: Some(der::Any::from(der::asn1::Null)),
            },
            SigningKey::RsaPss(_) => spki::AlgorithmIdentifierOwned {
                oid: oids::RSASSA_PSS,
                parameters: Some(der::Any::encode_from(&PssParameters::sha256()).map_err(failed)?),
            },
            SigningKey::P256(_) => spki::AlgorithmIdentifierOwned {
                oid: oids::ECDSA_WITH_SHA256,
                parameters: None,
            },
        })
    }

    /// about how many bytes a signature takes, for reserving room.
    pub fn signature_size(&self) -> usize {
        use rsa::traits::PublicKeyParts;
        match self {
            SigningKey::Rsa(key) | SigningKey::RsaPss(key) => key.size(),
            SigningKey::P256(_) => 72,
        }
    }
}

/// RSASSA-PSS-params (rfc 4055): the digest, the mask function and the
/// salt length. letters signs with sha-256 throughout and a 32 byte salt.
#[derive(Clone, Debug, PartialEq, Eq, der::Sequence)]
pub struct PssParameters {
    #[asn1(context_specific = "0", tag_mode = "EXPLICIT", optional = "true")]
    pub hash: Option<spki::AlgorithmIdentifierOwned>,
    #[asn1(context_specific = "1", tag_mode = "EXPLICIT", optional = "true")]
    pub mask_generation: Option<spki::AlgorithmIdentifierOwned>,
    #[asn1(context_specific = "2", tag_mode = "EXPLICIT", optional = "true")]
    pub salt_length: Option<u32>,
}

impl PssParameters {
    pub fn sha256() -> Self {
        let sha256 = spki::AlgorithmIdentifierOwned {
            oid: oids::SHA256,
            parameters: None,
        };
        Self {
            hash: Some(sha256.clone()),
            mask_generation: Some(spki::AlgorithmIdentifierOwned {
                oid: oids::MGF1,
                parameters: der::Any::encode_from(&sha256).ok(),
            }),
            salt_length: Some(32),
        }
    }
}

/// "CN=Ada Lovelace,OU=Self-signed" gives "Ada Lovelace".
pub fn common_name(subject: &str) -> Option<String> {
    subject.split(',').find_map(|part| {
        part.trim()
            .strip_prefix("CN=")
            .map(|name| name.replace("\\,", ","))
    })
}
